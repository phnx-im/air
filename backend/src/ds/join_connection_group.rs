// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::{LeafCredential, LeafCredentialError},
    identifiers::QsReference,
    messages::client_ds::{AadMessage, AadPayload, JoinConnectionGroupParamsAad},
    time::TimeStamp,
};
use airprotos::client::app_data::ClientAppData;
use apqmls::extension::APQMLS_COMPONENT_ID;
use mimi_room_policy::RoleIndex;
use mls_assist::{
    group::{
        self, ProcessedAssistedMessage,
        apq::{ApqGroupRef, ApqRetainedWelcomeInfo},
        errors::{ProcessApqAssistedMessageError, ProcessAssistedMessageError},
    },
    messages::{AssistedMessageIn, SerializedMlsMessage},
    openmls::{
        error::LibraryError,
        group::MergeCommitError,
        prelude::{
            GroupEpoch, LeafNode, LeafNodeIndex, ProcessedMessageContent, Proposal, Sender,
            StagedCommit,
        },
    },
    provider_traits::MlsAssistProvider,
};
use thiserror::Error;
use tls_codec::DeserializeBytes;
use tonic::Status;
use tracing::error;

use crate::errors::CborMlsAssistStorage;

use super::{
    apq::ApqExternalCommit,
    group_state::{DsGroupState, MemberProfile, leaf_credential_matches_flag},
};

/// Reject any proposal an external commit joining a connection group must not
/// carry.
///
/// Permitted are the `ExternalInit` every external commit is built on and the
/// PSK proposal carrying the connection offer. An APQ join in addition carries
/// the combiner's `ApqInfo` in an `AppDataUpdate` proposal on both legs, plus
/// the combiner PSK on the T leg. A joiner has no standing to propose anything
/// else, membership changes least of all. The group bootstrap blob for the
/// joiner's sibling emulator clients does not ride in the commit either: it
/// travels as a request parameter and reaches only the sibling queues, as a DS
/// echo.
fn validate_join_proposal(proposal: &Proposal, apq: bool) -> Result<(), JoinConnectionGroupError> {
    match proposal {
        Proposal::ExternalInit(_) | Proposal::PreSharedKey(_) => Ok(()),
        Proposal::AppDataUpdate(update) if apq && update.component_id() == APQMLS_COMPONENT_ID => {
            Ok(())
        }
        Proposal::Add(_)
        | Proposal::Update(_)
        | Proposal::Remove(_)
        | Proposal::ReInit(_)
        | Proposal::GroupContextExtensions(_)
        | Proposal::AppDataUpdate(_)
        | Proposal::AppEphemeral(_)
        | Proposal::SelfRemove
        | Proposal::Custom(_) => {
            tracing::warn!(
                proposal_type = ?proposal.proposal_type(),
                "Unexpected proposal in a connection-group external commit"
            );
            Err(JoinConnectionGroupError::InvalidMessage(
                "unexpected proposal in a connection-group external commit",
            ))
        }
    }
}

/// Reject an external commit whose proposals a connection-group join must not
/// contain. See [`validate_join_proposal`] for what is permitted.
fn validate_join_proposals(
    staged_commit: &StagedCommit,
    apq: bool,
) -> Result<(), JoinConnectionGroupError> {
    for proposal in staged_commit.queued_proposals() {
        validate_join_proposal(proposal.proposal(), apq)?;
    }
    Ok(())
}

/// Only a virtual client has siblings to echo to.
fn validate_virtual_client_leaf(leaf: &LeafNode) -> Result<(), JoinConnectionGroupError> {
    if ClientAppData::leaf_is_virtual_client(leaf) {
        Ok(())
    } else {
        Err(JoinConnectionGroupError::InvalidMessage(
            "group bootstrap requires a virtual-client joiner leaf",
        ))
    }
}

pub(super) struct JoinConnectionGroupOutcome {
    pub(super) message: SerializedMlsMessage,
    /// The epoch of the staged snapshot, present iff the join carried a group
    /// bootstrap.
    pub(super) snapshot_epoch: Option<GroupEpoch>,
}

impl DsGroupState {
    /// Accept an external commit joining a connection group.
    ///
    /// With `bootstrap_requested`, the joiner's sibling emulator clients get an
    /// echo of the operation, so the joining leaf must be a virtual-client leaf
    /// and the pre-commit state is staged as an epoch snapshot for them.
    pub(super) fn join_connection_group(
        &mut self,
        external_commit: AssistedMessageIn,
        qs_client_reference: QsReference,
        bootstrap_requested: bool,
    ) -> Result<JoinConnectionGroupOutcome, JoinConnectionGroupError> {
        // Process message (but don't apply it yet). This performs mls-assist-level validations.
        let processed_assisted_message_plus = self
            .group()
            .process_assisted_message(self.provider.crypto(), external_commit)?;

        let ProcessedAssistedMessage::Commit(processed_message, _group_info) =
            &processed_assisted_message_plus.processed_assisted_message
        else {
            return Err(JoinConnectionGroupError::InvalidMessage("expected commit"));
        };
        let ProcessedMessageContent::StagedCommitMessage(staged_commit) =
            processed_message.content()
        else {
            return Err(JoinConnectionGroupError::InvalidMessage(
                "expected staged commit",
            ));
        };
        if !matches!(processed_message.sender(), Sender::NewMemberCommit) {
            return Err(JoinConnectionGroupError::InvalidMessage(
                "expected new member commit",
            ));
        }
        if !self.self_group_flag_unchanged(staged_commit) {
            return Err(JoinConnectionGroupError::InvalidMessage(
                "commit would toggle the self-group flag",
            ));
        }
        let joiner_leaf = staged_commit.update_path_leaf_node().ok_or(
            JoinConnectionGroupError::InvalidMessage("update path leaf node not found"),
        )?;
        let joiner_credential = LeafCredential::from_credential(joiner_leaf.credential())?;
        self.validate_connection_group_join(staged_commit, &joiner_credential, false)?;
        if bootstrap_requested {
            validate_virtual_client_leaf(joiner_leaf)?;
        }
        let joiner_index = self.group().ext_commit_sender_index(staged_commit)?;
        let aad_payload =
            self.admit_connection_group_joiner(processed_message.tail_aad(), &joiner_credential)?;

        // The siblings apply the commit on top of the state the joiner used, so
        // capture it before the commit is accepted.
        let staged_snapshot =
            bootstrap_requested.then(|| (self.group().epoch(), self.epoch_snapshot()));

        let retained_welcome_info = self.group.accept_processed_message(
            self.provider.storage(),
            processed_assisted_message_plus.processed_assisted_message,
        )?;

        self.insert_joiner_profile(joiner_index, qs_client_reference, aad_payload);
        self.stage_welcome_info(retained_welcome_info);

        let message = processed_assisted_message_plus.serialized_mls_message;
        let snapshot_epoch = staged_snapshot.map(|(epoch, snapshot)| {
            self.stage_epoch_snapshot(epoch, snapshot.with_join_commit(&message));
            epoch
        });

        Ok(JoinConnectionGroupOutcome {
            message,
            snapshot_epoch,
        })
    }

    /// The APQ variant of [`Self::join_connection_group`]. The snapshot it
    /// stages covers both legs and is keyed by the T leg's group id.
    pub(super) fn apq_join_connection_group(
        t: &mut Self,
        pq: &mut Self,
        t_message: AssistedMessageIn,
        pq_message: AssistedMessageIn,
        qs_client_reference: QsReference,
        bootstrap_requested: bool,
    ) -> Result<JoinConnectionGroupOutcome, JoinConnectionGroupError> {
        let processed_assisted_message_plus = ApqGroupRef::from_groups(&mut t.group, &mut pq.group)
            .process_apq_assisted_message(t.provider.crypto(), t_message, pq_message, |_, _| {
                true
            })?;

        let ApqExternalCommit {
            t_staged_commit,
            pq_staged_commit,
            aad,
            t_new_leaf,
            pq_new_leaf,
            new_credential: joiner_credential,
            new_sender_index: joiner_index,
        } = Self::validate_apq_external_commit(
            t,
            pq,
            &processed_assisted_message_plus.processed_assisted_message,
        )?;
        t.validate_connection_group_join(t_staged_commit, &joiner_credential, true)?;
        validate_join_proposals(pq_staged_commit, true)?;
        if bootstrap_requested {
            // The legs share a signature key, but being a virtual client is a
            // leaf extension, so each leaf carries it on its own.
            validate_virtual_client_leaf(t_new_leaf)?;
            validate_virtual_client_leaf(pq_new_leaf)?;
        }
        let aad_payload = t.admit_connection_group_joiner(aad, &joiner_credential)?;

        // The siblings apply both commits on top of the state the joiner used,
        // so capture it before the commits are accepted.
        let staged_snapshot = bootstrap_requested.then(|| {
            let snapshot = t.epoch_snapshot().with_pq_leg(
                pq.group().group_info().clone(),
                pq.group().export_ratchet_tree(),
            );
            (t.group().epoch(), snapshot)
        });

        let ApqRetainedWelcomeInfo {
            t_retained_welcome_info,
            pq_retained_welcome_info,
        } = ApqGroupRef::from_groups(&mut t.group, &mut pq.group).accept_apq_processed_message(
            t.provider.storage(),
            pq.provider.storage(),
            processed_assisted_message_plus.processed_assisted_message,
        )?;

        // Profiles are never maintained in PQ group state
        t.insert_joiner_profile(joiner_index, qs_client_reference, aad_payload);
        t.stage_welcome_info(t_retained_welcome_info);
        pq.stage_welcome_info_without_profile_keys(pq_retained_welcome_info);

        let t_serialized_message = processed_assisted_message_plus.t_serialized_message;
        let pq_serialized_message = processed_assisted_message_plus.pq_serialized_message;
        let snapshot_epoch = staged_snapshot.map(|(epoch, snapshot)| {
            t.stage_epoch_snapshot(
                epoch,
                snapshot.with_apq_join_commits(&t_serialized_message, &pq_serialized_message),
            );
            epoch
        });

        Ok(JoinConnectionGroupOutcome {
            message: SerializedMlsMessage::combine_apq(t_serialized_message, pq_serialized_message),
            snapshot_epoch,
        })
    }

    /// Checks the parts of a join that are specific to connection groups
    fn validate_connection_group_join(
        &self,
        staged_commit: &StagedCommit,
        joiner_credential: &LeafCredential,
        apq: bool,
    ) -> Result<(), JoinConnectionGroupError> {
        validate_join_proposals(staged_commit, apq)?;
        if self.is_self_group() {
            return Err(JoinConnectionGroupError::InvalidMessage(
                "connection group must not be a self-group",
            ));
        }
        if !leaf_credential_matches_flag(joiner_credential, false) {
            return Err(JoinConnectionGroupError::InvalidMessage(
                "connection group joiner must carry a user credential",
            ));
        }
        Ok(())
    }

    /// Parses the join AAD, checks that the group has exactly one member (the
    /// inviter) and admits the joiner into the room state.
    ///
    /// The inviter created the room state before it knew the joiner's user id,
    /// so the joiner is not in it yet. Record the joiner as if the inviter had
    /// added them. Both clients apply the same change locally.
    fn admit_connection_group_joiner(
        &mut self,
        aad: &[u8],
        joiner_credential: &LeafCredential,
    ) -> Result<JoinConnectionGroupParamsAad, JoinConnectionGroupError> {
        let aad_message = AadMessage::tls_deserialize_exact_bytes(aad)?;
        // TODO: Check version of Aad Message
        let AadPayload::JoinConnectionGroup(aad_payload) = aad_message.into_payload() else {
            return Err(JoinConnectionGroupError::InvalidMessage(
                "wrong AAD payload",
            ));
        };

        let mut member_indices = self.member_profiles.keys();
        let (Some(&inviter_index), None) = (member_indices.next(), member_indices.next()) else {
            return Err(JoinConnectionGroupError::NotAConnectionGroup);
        };
        let inviter = self
            .leaf_credential(inviter_index)
            .ok_or(JoinConnectionGroupError::InvalidMessage("unknown inviter"))?;
        self.room_state_change_role(
            &inviter.room_policy_identity(),
            &joiner_credential.room_policy_identity(),
            RoleIndex::Regular,
        )
        .ok_or(JoinConnectionGroupError::InvalidMessage(
            "failed to admit joiner into the room state",
        ))?;

        Ok(aad_payload)
    }

    /// Records the joiner's profile. Call after the commit was accepted, so
    /// that the activity epoch is the joiner's first epoch.
    fn insert_joiner_profile(
        &mut self,
        leaf_index: LeafNodeIndex,
        qs_client_reference: QsReference,
        aad_payload: JoinConnectionGroupParamsAad,
    ) {
        let member_profile = MemberProfile {
            leaf_index,
            client_queue_config: qs_client_reference,
            activity_time: TimeStamp::now(),
            activity_epoch: self.group().epoch(),
            encrypted_user_profile_key: aad_payload.encrypted_user_profile_key,
        };
        self.member_profiles.insert(leaf_index, member_profile);

        #[cfg(debug_assertions)]
        self.check_member_profiles("join_connection_group");
    }
}

/// Potential errors when joining a connection group.
#[derive(Debug, Error)]
pub(crate) enum JoinConnectionGroupError {
    #[error("Invalid assisted message: {0}")]
    InvalidMessage(&'static str),
    #[error("Not a connection group")]
    NotAConnectionGroup,
    #[error("Invalid joiner credential: {0}")]
    InvalidCredential(#[from] LeafCredentialError),
    #[error("Invalid AAD: {0}")]
    InvalidAad(#[from] tls_codec::Error),
    #[error("Error processing message: {0}")]
    ProcessingError(#[from] ProcessAssistedMessageError),
    #[error("Error processing APQ message: {0}")]
    ApqProcessingError(#[from] ProcessApqAssistedMessageError),
    #[error(transparent)]
    LibraryError(#[from] LibraryError),
    #[error("Error merging commit: {0}")]
    MergeCommitError(#[from] MergeCommitError<group::errors::StorageError<CborMlsAssistStorage>>),
}

impl From<JoinConnectionGroupError> for Status {
    fn from(error: JoinConnectionGroupError) -> Self {
        use JoinConnectionGroupError::*;
        match error {
            InvalidMessage(_) | NotAConnectionGroup | InvalidCredential(_) | InvalidAad(_) => {
                Status::invalid_argument(error.to_string())
            }
            ProcessingError(_) | ApqProcessingError(_) | LibraryError(_) | MergeCommitError(_) => {
                error!(%error, "Failed to join connection group");
                Status::internal("Failed to join connection group")
            }
        }
    }
}

#[cfg(test)]
mod test {
    use airprotos::client::component::AIR_COMPONENT_ID;
    use mls_assist::{
        openmls::{
            prelude::{
                AppDataUpdateProposal, AppEphemeralProposal, Ciphersuite, CustomProposal,
                ExternalInitProposal, OpenMlsProvider, PreSharedKeyProposal,
            },
            schedule::{ExternalPsk, PreSharedKeyId, Psk},
        },
        openmls_rust_crypto::OpenMlsRustCrypto,
    };

    use super::*;

    fn psk_proposal() -> Proposal {
        let provider = OpenMlsRustCrypto::default();
        let psk_id = PreSharedKeyId::new(
            Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519,
            provider.rand(),
            Psk::External(ExternalPsk::new(vec![1u8; 32])),
        )
        .unwrap();
        Proposal::PreSharedKey(Box::new(PreSharedKeyProposal::new(psk_id)))
    }

    fn app_data_update_proposal(component_id: u16) -> Proposal {
        Proposal::AppDataUpdate(Box::new(AppDataUpdateProposal::update(
            component_id,
            vec![1u8; 8],
        )))
    }

    #[test]
    fn external_init_and_psk_are_permitted() {
        for apq in [false, true] {
            validate_join_proposal(
                &Proposal::ExternalInit(Box::new(ExternalInitProposal::from(vec![1u8; 32]))),
                apq,
            )
            .unwrap();
            validate_join_proposal(&psk_proposal(), apq).unwrap();
        }
    }

    #[test]
    fn the_combiner_app_data_update_is_permitted_only_for_apq() {
        validate_join_proposal(&app_data_update_proposal(APQMLS_COMPONENT_ID), true).unwrap();
        for (proposal, apq) in [
            (app_data_update_proposal(APQMLS_COMPONENT_ID), false),
            (app_data_update_proposal(AIR_COMPONENT_ID), true),
        ] {
            assert!(
                matches!(
                    validate_join_proposal(&proposal, apq),
                    Err(JoinConnectionGroupError::InvalidMessage(_))
                ),
                "{proposal:?} was permitted with apq = {apq}"
            );
        }
    }

    #[test]
    fn proposals_outside_the_allowlist_are_rejected() {
        let rejected = [
            Proposal::SelfRemove,
            Proposal::Custom(Box::new(CustomProposal::new(0xf00d, vec![1u8; 8]))),
            app_data_update_proposal(AIR_COMPONENT_ID),
            Proposal::AppEphemeral(Box::new(AppEphemeralProposal::new(
                AIR_COMPONENT_ID,
                vec![1u8; 8],
            ))),
        ];
        for proposal in rejected {
            let result = validate_join_proposal(&proposal, false);
            assert!(
                matches!(result, Err(JoinConnectionGroupError::InvalidMessage(_))),
                "{proposal:?} was permitted"
            );
        }
    }
}
