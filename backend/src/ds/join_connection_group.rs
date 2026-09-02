// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::LeafCredential,
    messages::client_ds::{AadMessage, AadPayload, JoinConnectionGroupParams},
    mls_group_config::leaf_node_is_virtual_client,
    time::TimeStamp,
};
use mls_assist::{
    group::ProcessedAssistedMessage,
    messages::SerializedMlsMessage,
    openmls::{
        group::StagedCommit,
        prelude::{GroupEpoch, ProcessedMessageContent, Proposal},
    },
    provider_traits::MlsAssistProvider,
};
use tls_codec::DeserializeBytes;

use crate::errors::JoinConnectionGroupError;

use super::group_state::{DsGroupState, MemberProfile, leaf_credential_matches_flag};

/// Reject any proposal an external commit joining a connection group must not
/// carry.
///
/// Permitted are the `ExternalInit` every external commit is built on and the
/// PSK proposal carrying the connection offer. A joiner has no standing to
/// propose anything else, membership changes least of all. The group bootstrap
/// blob for the joiner's sibling emulator clients does not ride in the commit
/// either: it travels as a request parameter and reaches only the sibling
/// queues, as a DS echo.
fn validate_join_proposal(proposal: &Proposal) -> Result<(), JoinConnectionGroupError> {
    match proposal {
        Proposal::ExternalInit(_) | Proposal::PreSharedKey(_) => Ok(()),
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
            Err(JoinConnectionGroupError::InvalidMessage)
        }
    }
}

/// Reject an external commit whose proposals a connection-group join must not
/// contain. See [`validate_join_proposal`] for what is permitted.
fn validate_join_proposals(staged_commit: &StagedCommit) -> Result<(), JoinConnectionGroupError> {
    for proposal in staged_commit.queued_proposals() {
        validate_join_proposal(proposal.proposal())?;
    }
    Ok(())
}

pub(super) struct JoinConnectionGroupOutcome {
    pub(super) message: SerializedMlsMessage,
    /// The epoch the group was at before the commit. A snapshot is staged at
    /// this epoch iff the join carried a group bootstrap.
    pub(super) pre_commit_epoch: GroupEpoch,
}

impl DsGroupState {
    /// Accept an external commit joining a connection group.
    ///
    /// With `bootstrap_requested`, the echo of the operation carries a group
    /// bootstrap for the joiner's sibling emulator clients, so the joining
    /// leaf must be a virtual-client leaf and the pre-commit state is staged
    /// as an epoch snapshot for them.
    pub(super) fn join_connection_group(
        &mut self,
        params: JoinConnectionGroupParams,
        bootstrap_requested: bool,
    ) -> Result<JoinConnectionGroupOutcome, JoinConnectionGroupError> {
        // Process message (but don't apply it yet). This performs mls-assist-level validations.
        let processed_assisted_message_plus = self
            .group()
            .process_assisted_message(self.provider.crypto(), params.external_commit)
            .map_err(|e| {
                tracing::warn!(
                    "Processing error: Could not process assisted message: {:?}",
                    e
                );
                JoinConnectionGroupError::ProcessingError
            })?;

        // Perform DS-level validation
        // Make sure that we have the right message type.
        let processed_message =
            if let ProcessedAssistedMessage::Commit(processed_message, _group_info) =
                &processed_assisted_message_plus.processed_assisted_message
            {
                processed_message
            } else {
                // This should be a commit.
                tracing::warn!("Invalid message: Processed message does not contain a commit.");
                return Err(JoinConnectionGroupError::InvalidMessage);
            };

        // The external commit joining the client into the group carries the path plus, at most, the
        // proposals `validate_join_proposals` permits.
        if let ProcessedMessageContent::StagedCommitMessage(staged_commit) =
            processed_message.content()
        {
            validate_join_proposals(staged_commit)?;
            if !self.self_group_flag_unchanged(staged_commit) {
                tracing::warn!("Commit would toggle the self-group flag");
                return Err(JoinConnectionGroupError::InvalidMessage);
            }
            // A connection group is never a self-group, and its joiner's leaf must carry a user
            // credential.
            if self.is_self_group() {
                tracing::warn!("Connection group must not be a self-group");
                return Err(JoinConnectionGroupError::InvalidMessage);
            }
            let joiner_leaf = staged_commit
                .update_path_leaf_node()
                .ok_or(JoinConnectionGroupError::InvalidMessage)?;
            let joiner_credential = LeafCredential::from_credential(joiner_leaf.credential())
                .map_err(|_| JoinConnectionGroupError::InvalidMessage)?;
            if !leaf_credential_matches_flag(&joiner_credential, false) {
                tracing::warn!("Connection group joiner must carry a user credential");
                return Err(JoinConnectionGroupError::InvalidMessage);
            }
            // Only a virtual client has siblings to echo to.
            if bootstrap_requested && !leaf_node_is_virtual_client(joiner_leaf) {
                tracing::warn!("Group bootstrap requires a virtual-client joiner leaf");
                return Err(JoinConnectionGroupError::InvalidMessage);
            }
        } else {
            tracing::warn!("Invalid message: Commit content is not a staged commit.");
            return Err(JoinConnectionGroupError::InvalidMessage);
        };

        let aad_message = AadMessage::tls_deserialize_exact_bytes(processed_message.tail_aad())
            .map_err(|_| {
                tracing::warn!("Invalid message: Failed to deserialize AAD.");
                JoinConnectionGroupError::InvalidMessage
            })?;
        // TODO: Check version of Aad Message
        let aad_payload = if let AadPayload::JoinConnectionGroup(aad) = aad_message.into_payload() {
            aad
        } else {
            tracing::warn!("Invalid message: Wrong AAD payload.");
            return Err(JoinConnectionGroupError::InvalidMessage);
        };

        // Check if the group indeed only has one user (prior to the new one joining).
        if self.member_profiles.len() > 1 {
            return Err(JoinConnectionGroupError::NotAConnectionGroup);
        }

        // Get the sender's credential s.t. we can identify them later.
        let sender_credential = processed_message.credential().clone();

        // The siblings apply the commit on top of the state the joiner used, so
        // capture it before the commit is accepted.
        let pre_commit_epoch = self.group().epoch();
        let staged_snapshot = bootstrap_requested.then(|| self.epoch_snapshot());

        // Finalize processing.
        let retained_welcome_info = self.group.accept_processed_message(
            self.provider.storage(),
            processed_assisted_message_plus.processed_assisted_message,
        )?;

        // Let's figure out the leaf index of the new member.
        let sender = if let Some(sender) = self.group().members().find_map(|m| {
            if m.credential == sender_credential {
                Some(m.index)
            } else {
                None
            }
        }) {
            sender
        } else {
            tracing::warn!("Could not find sender in group.");
            return Err(JoinConnectionGroupError::ProcessingError);
        };

        let member_profile = MemberProfile {
            leaf_index: sender,
            client_queue_config: params.qs_client_reference,
            activity_time: TimeStamp::now(),
            activity_epoch: self.group().epoch(),
            encrypted_user_profile_key: aad_payload.encrypted_user_profile_key,
        };

        self.member_profiles.insert(sender, member_profile);
        self.stage_welcome_info(retained_welcome_info);

        // Finally, we create the message for distribution.
        let message = processed_assisted_message_plus.serialized_mls_message;

        if let Some(snapshot) = staged_snapshot {
            self.stage_epoch_snapshot(pre_commit_epoch, snapshot.with_join_commit(&message));
        }

        Ok(JoinConnectionGroupOutcome {
            message,
            pre_commit_epoch,
        })
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

    #[test]
    fn external_init_and_psk_are_permitted() {
        validate_join_proposal(&Proposal::ExternalInit(Box::new(
            ExternalInitProposal::from(vec![1u8; 32]),
        )))
        .unwrap();
        validate_join_proposal(&psk_proposal()).unwrap();
    }

    #[test]
    fn proposals_outside_the_allowlist_are_rejected() {
        let rejected = [
            Proposal::SelfRemove,
            Proposal::Custom(Box::new(CustomProposal::new(0xf00d, vec![1u8; 8]))),
            Proposal::AppDataUpdate(Box::new(AppDataUpdateProposal::update(
                AIR_COMPONENT_ID,
                vec![1u8; 8],
            ))),
            Proposal::AppEphemeral(Box::new(AppEphemeralProposal::new(
                AIR_COMPONENT_ID,
                vec![1u8; 8],
            ))),
        ];
        for proposal in rejected {
            let result = validate_join_proposal(&proposal);
            assert!(
                matches!(result, Err(JoinConnectionGroupError::InvalidMessage)),
                "{proposal:?} was permitted"
            );
        }
    }
}
