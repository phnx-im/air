// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{credentials::VerifiableClientCredential, time::Duration, utils::removed_clients};
use mimi_room_policy::RoleIndex;
use mls_assist::{
    group::{ProcessedAssistedMessage, apq::ApqGroupRef},
    messages::SerializedMlsMessage,
    openmls::prelude::Sender,
    provider_traits::MlsAssistProvider,
};
use mls_assist::{
    messages::AssistedMessageIn,
    openmls::prelude::{LeafNodeIndex, ProcessedMessageContent},
};
use tracing::error;

use crate::errors::ResyncClientError;

use super::process::USER_EXPIRATION_DAYS;

use super::group_state::DsGroupState;

impl DsGroupState {
    /// Change the room-state role of every removed client to `Outsider`, using `sender` as the
    /// acting party.
    fn change_removed_roles_to_outsider(
        &mut self,
        sender: &VerifiableClientCredential,
        removed_indices: &[LeafNodeIndex],
    ) -> Result<(), ResyncClientError> {
        for &removed_index in removed_indices {
            let removed = self
                .leaf_credential(removed_index)
                .ok_or(ResyncClientError::InvalidMessage)?;
            self.room_state_change_role(sender.user_id(), removed.user_id(), RoleIndex::Outsider)
                .ok_or_else(|| {
                    error!(%removed_index, "Failed to change role of removed client");
                    ResyncClientError::InvalidMessage
                })?;
        }
        Ok(())
    }

    /// An external commit re-adds the client at the leftmost blank leaf. If it differs from the
    /// original leaf index, re-key the member profile accordingly, so that fan-out exclusion and QS
    /// references stay correct.
    fn rekey_sender_profile(&mut self, old_index: LeafNodeIndex, new_index: LeafNodeIndex) {
        if new_index != old_index
            && let Some(mut profile) = self.member_profiles.remove(&old_index)
        {
            profile.leaf_index = new_index;
            // The external committer joins at the leftmost blank leaf. At this point all profiles
            // of clients removed by this commit (including the sender's old entry) have been
            // removed, so the new index is guaranteed to be vacant.
            debug_assert!(!self.member_profiles.contains_key(&new_index));
            self.member_profiles.insert(new_index, profile);
        }
    }

    pub(crate) fn resync_client(
        &mut self,
        external_commit: AssistedMessageIn,
        sender_index: LeafNodeIndex,
    ) -> Result<SerializedMlsMessage, ResyncClientError> {
        // Process message (but don't apply it yet). This performs mls-assist-level validations.
        let processed_assisted_message_plus = self
            .group()
            .process_assisted_message(self.provider.crypto(), external_commit)
            .map_err(|_| ResyncClientError::ProcessingError)?;

        // Perform DS-level validation
        // Make sure that we have the right message type.
        let processed_message =
            if let ProcessedAssistedMessage::Commit(processed_message, _group_info) =
                &processed_assisted_message_plus.processed_assisted_message
            {
                processed_message
            } else {
                // This should be a commit.
                return Err(ResyncClientError::InvalidMessage);
            };

        let ProcessedMessageContent::StagedCommitMessage(staged_commit_message) =
            processed_message.content()
        else {
            // This should be a staged commit message.
            return Err(ResyncClientError::InvalidMessage);
        };

        // Check if it's an external commit.
        if !matches!(processed_message.sender(), Sender::NewMemberCommit) {
            return Err(ResyncClientError::InvalidMessage);
        }

        if !staged_commit_message
            .remove_proposals()
            .any(|p| p.remove_proposal().removed() == sender_index)
        {
            // There must be a remove proposal for the sender.
            //
            // Note: The commit might contain multiple remove proposals.
            return Err(ResyncClientError::InvalidMessage);
        }

        let sender = self
            .leaf_credential(sender_index)
            .ok_or(ResyncClientError::InvalidMessage)?;

        // Collect all removed clients except the sender.
        let mut removed_indices = removed_clients(staged_commit_message);
        let sender_index_pos = removed_indices
            .iter()
            .position(|&index| index == sender_index)
            .ok_or_else(|| {
                error!(%sender_index, "Sender not found in removed clients");
                ResyncClientError::InvalidMessage
            })?;
        removed_indices.swap_remove(sender_index_pos);

        // Change room state roles of removed clients to outsider.
        self.change_removed_roles_to_outsider(&sender, &removed_indices)?;

        // Everything seems to be okay.
        // Now we have to update the group state and distribute.

        let new_sender_index = self
            .group()
            .ext_commit_sender_index(staged_commit_message)
            .map_err(|error| {
                error!(%error, "Error getting sender index");
                ResyncClientError::InvalidMessage
            })?;

        // We just accept the message into the group state.
        self.group.accept_processed_message(
            self.provider.storage(),
            processed_assisted_message_plus.processed_assisted_message,
            Duration::days(USER_EXPIRATION_DAYS),
        )?;

        self.remove_profiles(removed_indices);

        self.rekey_sender_profile(sender_index, new_sender_index);

        Ok(processed_assisted_message_plus.serialized_mls_message)
    }

    pub(crate) fn apq_resync_client(
        t_group_state: &mut Self,
        pq_group_state: &mut Self,
        t_message: AssistedMessageIn,
        pq_message: AssistedMessageIn,
        t_sender_index: LeafNodeIndex,
    ) -> Result<SerializedMlsMessage, ResyncClientError> {
        let processed_assisted_message_plus =
            ApqGroupRef::from_groups(&mut t_group_state.group, &mut pq_group_state.group)
                .process_apq_assisted_message(
                    t_group_state.provider.crypto(),
                    t_message,
                    pq_message,
                    |_, _| true,
                )
                .map_err(|error| {
                    error!(%error,"Failed to process APQ message");
                    ResyncClientError::ProcessingError
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
            return Err(ResyncClientError::InvalidMessage);
        };

        let (Sender::NewMemberCommit, Sender::NewMemberCommit) =
            (t_processed_message.sender(), pq_processed_message.sender())
        else {
            error!("Invalid sender; expected new member commit");
            return Err(ResyncClientError::InvalidMessage);
        };

        // Bind the two legs at the new leaf: the T and PQ update paths must be signed with the same
        // signature key.
        let t_new_leaf_key = t_staged_commit
            .update_path_leaf_node()
            .ok_or_else(|| {
                error!("T update path leaf node not found");
                ResyncClientError::InvalidMessage
            })?
            .signature_key();
        let pq_new_leaf_key = pq_staged_commit
            .update_path_leaf_node()
            .ok_or_else(|| {
                error!("PQ update path leaf node not found");
                ResyncClientError::InvalidMessage
            })?
            .signature_key();
        if t_new_leaf_key != pq_new_leaf_key {
            error!("T and PQ update path signature keys do not match");
            return Err(ResyncClientError::InvalidMessage);
        }

        let t_sender = t_group_state
            .leaf_credential(t_sender_index)
            .ok_or(ResyncClientError::InvalidMessage)?;

        // Collect all removed clients except the sender
        let mut t_removed_indices = removed_clients(t_staged_commit);
        let mut pq_removed_indices = removed_clients(pq_staged_commit);
        t_removed_indices.sort_unstable();
        pq_removed_indices.sort_unstable();
        if t_removed_indices != pq_removed_indices {
            error!("T and PQ removed clients do not match");
            return Err(ResyncClientError::InvalidMessage);
        }
        let Some(sender_index_pos) = t_removed_indices
            .iter()
            .position(|&index| index == t_sender_index)
        else {
            error!(%t_sender_index, "Sender not found in removed clients");
            return Err(ResyncClientError::InvalidMessage);
        };
        t_removed_indices.swap_remove(sender_index_pos);

        // Change room state roles of removed clients to outsider (in T group)
        t_group_state.change_removed_roles_to_outsider(&t_sender, &t_removed_indices)?;

        let t_new_sender_index = t_group_state
            .group
            .ext_commit_sender_index(t_staged_commit)
            .map_err(|error| {
                error!(%error, "Error getting T sender index");
                ResyncClientError::InvalidMessage
            })?;
        let pq_new_sender_index = pq_group_state
            .group
            .ext_commit_sender_index(pq_staged_commit)
            .map_err(|error| {
                error!(%error, "Error getting PQ sender index");
                ResyncClientError::InvalidMessage
            })?;
        if t_new_sender_index != pq_new_sender_index {
            error!("T and PQ sender indices do not match");
            return Err(ResyncClientError::InvalidMessage);
        }

        // Everything seems to be okay.
        // Now we have to update the group state and distribute.

        ApqGroupRef::from_groups(&mut t_group_state.group, &mut pq_group_state.group)
            .accept_apq_processed_message(
                t_group_state.provider.storage(),
                pq_group_state.provider.storage(),
                processed_assisted_message_plus.processed_assisted_message,
                Duration::days(USER_EXPIRATION_DAYS),
            )?;

        t_group_state.remove_profiles(t_removed_indices);

        t_group_state.rekey_sender_profile(t_sender_index, t_new_sender_index);
        // Profiles are never maintained in PQ group state

        Ok(processed_assisted_message_plus.serialized_apq_message)
    }
}
