// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::LeafCredential,
    identifiers::QsReference,
    messages::client_ds::{AadMessage, AadPayload},
    time::TimeStamp,
};
use mimi_room_policy::RoleIndex;
use mls_assist::{
    group::{
        self, ProcessedAssistedMessage,
        apq::{ApqGroupRef, ApqRetainedWelcomeInfo},
    },
    messages::{AssistedMessageIn, SerializedMlsMessage},
    openmls::{framing::Sender, group::MergeCommitError, prelude::ProcessedMessageContent},
    provider_traits::MlsAssistProvider,
};
use thiserror::Error;
use tls_codec::DeserializeBytes;
use tonic::Status;
use tracing::error;

use crate::errors::CborMlsAssistStorage;

use super::group_state::{DsGroupState, MemberProfile, leaf_credential_matches_flag};

impl DsGroupState {
    pub(super) fn join_connection_group(
        &mut self,
        external_commit: AssistedMessageIn,
        qs_client_reference: QsReference,
    ) -> Result<SerializedMlsMessage, JoinConnectionGroupError> {
        // Process message (but don't apply it yet). This performs mls-assist-level validations.
        let processed_assisted_message_plus = self
            .group()
            .process_assisted_message(self.provider.crypto(), external_commit)
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

        // The external commit joining the client into the group should contain only the path.
        let joiner_credential = if let ProcessedMessageContent::StagedCommitMessage(staged_commit) =
            processed_message.content()
        {
            if staged_commit.add_proposals().count() > 0
                || staged_commit.update_proposals().count() > 0
                || staged_commit.remove_proposals().count() > 0
            {
                return Err(JoinConnectionGroupError::InvalidMessage);
            }
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
            joiner_credential
        } else {
            tracing::warn!("Invalid message: External commit contained unexpected proposals.");
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

        // Check that the group indeed has exactly one member (prior to the new one joining). That
        // member is the inviter.
        let mut member_indices = self.member_profiles.keys();
        let (Some(&inviter_index), None) = (member_indices.next(), member_indices.next()) else {
            return Err(JoinConnectionGroupError::NotAConnectionGroup);
        };

        // The inviter created the room state before it knew the joiner's user id, so the joiner is
        // not in it yet. Record the joiner as if the inviter had added them. Both clients apply
        // the same change locally.
        let inviter = self
            .leaf_credential(inviter_index)
            .ok_or(JoinConnectionGroupError::InvalidMessage)?;
        self.room_state_change_role(
            &inviter.room_policy_identity(),
            &joiner_credential.room_policy_identity(),
            RoleIndex::Regular,
        )
        .ok_or(JoinConnectionGroupError::InvalidMessage)?;

        // Get the sender's credential s.t. we can identify them later.
        let sender_credential = processed_message.credential().clone();

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
            client_queue_config: qs_client_reference,
            activity_time: TimeStamp::now(),
            activity_epoch: self.group().epoch(),
            encrypted_user_profile_key: aad_payload.encrypted_user_profile_key,
        };

        self.member_profiles.insert(sender, member_profile);
        self.stage_welcome_info(retained_welcome_info);

        // Finally, we create the message for distribution.
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
            .process_apq_assisted_message(t.provider.crypto(), t_message, pq_message, |_, _| true)
            .map_err(|error| {
                error!(%error, "Failed to process APQ message");
                JoinConnectionGroupError::ProcessingError
            })?;

        // Perform DS-level validation
        let apq_processed_message = &processed_assisted_message_plus
            .processed_assisted_message
            .processed_message;
        let t_processed_message = &apq_processed_message.t_message;
        let pq_processed_message = &apq_processed_message.pq_message;

        let (
            ProcessedMessageContent::StagedCommitMessage(t_staged_commit),
            ProcessedMessageContent::StagedCommitMessage(pq_staged_commit),
        ) = (
            &t_processed_message.content(),
            &pq_processed_message.content(),
        )
        else {
            error!("Invalid message content; expected staged commit");
            return Err(JoinConnectionGroupError::InvalidMessage);
        };

        for (state, staged_commit) in [(&t, &t_staged_commit), (&pq, &pq_staged_commit)] {
            // The external commit joining the client into the group should contain only the path.
            if staged_commit.add_proposals().count() > 0
                || staged_commit.update_proposals().count() > 0
                || staged_commit.remove_proposals().count() > 0
            {
                error!("External commit contained unexpected proposals");
                return Err(JoinConnectionGroupError::InvalidMessage);
            }
            if !state.self_group_flag_unchanged(staged_commit) {
                error!("Commit would toggle the self-group flag");
                return Err(JoinConnectionGroupError::InvalidMessage);
            }
            // A connection group is never a self-group.
            if state.is_self_group() {
                error!("Connection group must not be a self-group");
                return Err(JoinConnectionGroupError::InvalidMessage);
            }
        }

        let (Sender::NewMemberCommit, Sender::NewMemberCommit) =
            (t_processed_message.sender(), pq_processed_message.sender())
        else {
            error!("Invalid sender; expected new member commit");
            return Err(JoinConnectionGroupError::InvalidMessage);
        };

        // Bind the two legs at the new leaf: the T and PQ update paths must be signed with the same
        // signature key.
        let t_new_leaf = t_staged_commit.update_path_leaf_node().ok_or_else(|| {
            error!("T update path leaf node not found");
            JoinConnectionGroupError::InvalidMessage
        })?;
        let pq_new_leaf = pq_staged_commit.update_path_leaf_node().ok_or_else(|| {
            error!("PQ update path leaf node not found");
            JoinConnectionGroupError::InvalidMessage
        })?;
        if t_new_leaf.signature_key() != pq_new_leaf.signature_key() {
            error!("T and PQ update path signature keys do not match");
            return Err(JoinConnectionGroupError::InvalidMessage);
        }

        // The joiner's leaf must carry a user credential. The PQ leaf is bound to it by the shared
        // signature key above.
        let joiner_credential =
            LeafCredential::from_credential(t_new_leaf.credential()).map_err(|error| {
                error!(%error, "Joiner leaf credential is invalid");
                JoinConnectionGroupError::InvalidMessage
            })?;
        if !leaf_credential_matches_flag(&joiner_credential, false) {
            error!("Connection group joiner must carry a user credential");
            return Err(JoinConnectionGroupError::InvalidMessage);
        }

        let t_new_sender_index =
            t.group
                .ext_commit_sender_index(t_staged_commit)
                .map_err(|error| {
                    error!(%error, "Error getting T sender index");
                    JoinConnectionGroupError::InvalidMessage
                })?;
        let pq_new_sender_index =
            pq.group
                .ext_commit_sender_index(pq_staged_commit)
                .map_err(|error| {
                    error!(%error, "Error getting PQ sender index");
                    JoinConnectionGroupError::InvalidMessage
                })?;
        if t_new_sender_index != pq_new_sender_index {
            error!("T and PQ sender indices do not match");
            return Err(JoinConnectionGroupError::InvalidMessage);
        }

        let aad_message: AadMessage = AadMessage::tls_deserialize_exact_bytes(
            t_processed_message.tail_aad(),
        )
        .map_err(|error| {
            error!(%error, "Failed to deserialize AAD");
            JoinConnectionGroupError::InvalidMessage
        })?;
        let AadPayload::JoinConnectionGroup(aad_payload) = aad_message.into_payload() else {
            error!("Wrong AAD payload");
            return Err(JoinConnectionGroupError::InvalidMessage);
        };

        // Check that the group indeed has exactly one member (prior to the new one joining). That
        // member is the inviter.
        let mut member_indices = t.member_profiles.keys();
        let (Some(&inviter_index), None) = (member_indices.next(), member_indices.next()) else {
            return Err(JoinConnectionGroupError::NotAConnectionGroup);
        };

        // The inviter created the room state before it knew the joiner's user id, so the joiner is
        // not in it yet. Record the joiner as if the inviter had added them. Both clients apply the
        // same change locally.
        let inviter = t
            .leaf_credential(inviter_index)
            .ok_or(JoinConnectionGroupError::InvalidMessage)?;
        t.room_state_change_role(
            &inviter.room_policy_identity(),
            &joiner_credential.room_policy_identity(),
            RoleIndex::Regular,
        )
        .ok_or(JoinConnectionGroupError::InvalidMessage)?;

        let ApqRetainedWelcomeInfo {
            t_retained_welcome_info,
            pq_retained_welcome_info,
        } = ApqGroupRef::from_groups(&mut t.group, &mut pq.group).accept_apq_processed_message(
            t.provider.storage(),
            pq.provider.storage(),
            processed_assisted_message_plus.processed_assisted_message,
        )?;

        let member_profile = MemberProfile {
            leaf_index: t_new_sender_index,
            client_queue_config: qs_client_reference,
            activity_time: TimeStamp::now(),
            activity_epoch: t.group().epoch(),
            encrypted_user_profile_key: aad_payload.encrypted_user_profile_key,
        };
        t.member_profiles.insert(t_new_sender_index, member_profile);
        // Profiles are never maintained in PQ group state

        #[cfg(debug_assertions)]
        t.check_member_profiles("apq_join_connection_group");

        t.stage_welcome_info(t_retained_welcome_info);
        pq.stage_welcome_info_without_profile_keys(pq_retained_welcome_info);

        Ok(processed_assisted_message_plus.serialized_apq_message)
    }
}

/// Potential errors when joining a connection group.
#[derive(Debug, Error)]
pub(crate) enum JoinConnectionGroupError {
    /// Invalid assisted message.
    #[error("Invalid assisted message")]
    InvalidMessage,
    /// Error processing message.
    #[error("Error processing message")]
    ProcessingError,
    /// Not a connection group.
    #[error("Not a connection group")]
    NotAConnectionGroup,
    #[error("Error merging commit")]
    MergeCommitError(#[from] MergeCommitError<group::errors::StorageError<CborMlsAssistStorage>>),
}

impl From<JoinConnectionGroupError> for Status {
    fn from(e: JoinConnectionGroupError) -> Self {
        let msg = e.to_string();
        match e {
            JoinConnectionGroupError::InvalidMessage
            | JoinConnectionGroupError::NotAConnectionGroup => Status::invalid_argument(msg),
            JoinConnectionGroupError::ProcessingError => Status::internal(msg),
            JoinConnectionGroupError::MergeCommitError(merge_commit_error) => {
                error!(%merge_commit_error, "failed merging commit");
                Status::internal(msg)
            }
        }
    }
}
