// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::{LeafCredential, LeafCredentialError},
    identifiers::QsReference,
    messages::client_ds::{AadMessage, AadPayload, JoinConnectionGroupParamsAad},
    time::TimeStamp,
};
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
        prelude::{LeafNodeIndex, ProcessedMessageContent, Sender, StagedCommit},
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

/// Whether the commit carries add, update or remove proposals.
fn has_membership_proposals(staged_commit: &StagedCommit) -> bool {
    staged_commit.add_proposals().next().is_some()
        || staged_commit.update_proposals().next().is_some()
        || staged_commit.remove_proposals().next().is_some()
}

impl DsGroupState {
    pub(super) fn join_connection_group(
        &mut self,
        external_commit: AssistedMessageIn,
        qs_client_reference: QsReference,
    ) -> Result<SerializedMlsMessage, JoinConnectionGroupError> {
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
        self.validate_connection_group_join(staged_commit, &joiner_credential)?;
        let joiner_index = self.group().ext_commit_sender_index(staged_commit)?;
        let aad_payload =
            self.admit_connection_group_joiner(processed_message.tail_aad(), &joiner_credential)?;

        let retained_welcome_info = self.group.accept_processed_message(
            self.provider.storage(),
            processed_assisted_message_plus.processed_assisted_message,
        )?;

        self.insert_joiner_profile(joiner_index, qs_client_reference, aad_payload);
        self.stage_welcome_info(retained_welcome_info);

        Ok(processed_assisted_message_plus.serialized_mls_message)
    }

    pub(super) fn apq_join_connection_group(
        t: &mut Self,
        pq: &mut Self,
        t_message: AssistedMessageIn,
        pq_message: AssistedMessageIn,
        qs_client_reference: QsReference,
    ) -> Result<SerializedMlsMessage, JoinConnectionGroupError> {
        let processed_assisted_message_plus = ApqGroupRef::from_groups(&mut t.group, &mut pq.group)
            .process_apq_assisted_message(t.provider.crypto(), t_message, pq_message, |_, _| {
                true
            })?;

        let ApqExternalCommit {
            t_staged_commit,
            pq_staged_commit,
            aad,
            new_credential: joiner_credential,
            new_sender_index: joiner_index,
        } = Self::validate_apq_external_commit(
            t,
            pq,
            &processed_assisted_message_plus.processed_assisted_message,
        )?;
        t.validate_connection_group_join(t_staged_commit, &joiner_credential)?;
        if has_membership_proposals(pq_staged_commit) {
            return Err(JoinConnectionGroupError::InvalidMessage(
                "PQ external commit contained unexpected proposals",
            ));
        }
        let aad_payload = t.admit_connection_group_joiner(aad, &joiner_credential)?;

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

        Ok(processed_assisted_message_plus.serialized_apq_message)
    }

    /// Checks the parts of a join that are specific to connection groups
    fn validate_connection_group_join(
        &self,
        staged_commit: &StagedCommit,
        joiner_credential: &LeafCredential,
    ) -> Result<(), JoinConnectionGroupError> {
        if has_membership_proposals(staged_commit) {
            return Err(JoinConnectionGroupError::InvalidMessage(
                "external commit contained unexpected proposals",
            ));
        }
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
