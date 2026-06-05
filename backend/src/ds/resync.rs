// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{credentials::VerifiableClientCredential, time::Duration, utils::removed_clients};
use mimi_room_policy::RoleIndex;
use mls_assist::{
    group::ProcessedAssistedMessage, messages::SerializedMlsMessage, openmls::prelude::Sender,
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

        let sender = VerifiableClientCredential::from_basic_credential(
            self.group
                .leaf(sender_index)
                .ok_or_else(|| {
                    error!(%sender_index, "Leaf node for sender not found");
                    ResyncClientError::InvalidMessage
                })?
                .credential(),
        )
        .map_err(|error| {
            error!(%error, "Credential of sender is invalid");
            ResyncClientError::InvalidMessage
        })?;

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
        for &removed_index in &removed_indices {
            let removed = VerifiableClientCredential::from_basic_credential(
                self.group()
                    .leaf(removed_index)
                    .ok_or_else(|| {
                        error!(%removed_index, "Leaf node for removed client not found");
                        ResyncClientError::InvalidMessage
                    })?
                    .credential(),
            )
            .map_err(|error| {
                error!(%error, "Credential of removed user is invalid");
                ResyncClientError::InvalidMessage
            })?;
            self.room_state_change_role(sender.user_id(), removed.user_id(), RoleIndex::Outsider)
                .ok_or_else(|| {
                    error!(%removed_index, "Failed to change role of removed client");
                    ResyncClientError::InvalidMessage
                })?;
        }

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

        // An external commit re-adds the client at the leftmost blank leaf. If it differs from the
        // original leaf index, we need to re-key the member profile accordingly, so that fan-out
        // exclusion and QS references stay correct.
        if new_sender_index != sender_index
            && let Some(mut profile) = self.member_profiles.remove(&sender_index)
        {
            profile.leaf_index = new_sender_index;
            // The external committer joins at the leftmost blank leaf. At this point all profiles
            // of clients removed by this commit (including the sender's old entry) have been
            // removed, so the new index is guaranteed to be vacant.
            debug_assert!(!self.member_profiles.contains_key(&new_sender_index));
            self.member_profiles.insert(new_sender_index, profile);
        }

        Ok(processed_assisted_message_plus.serialized_mls_message)
    }
}
