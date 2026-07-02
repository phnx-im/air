// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::utils::removed_clients;
use mls_assist::{
    group::ProcessedAssistedMessage,
    messages::{AssistedMessageIn, SerializedMlsMessage},
    openmls::prelude::{ProcessedMessageContent, Sender},
    provider_traits::MlsAssistProvider,
};

use crate::errors::GroupDeletionError;

use super::group_state::DsGroupState;

/// Source of truth for the group's membership when validating that a
/// delete-group commit removes all other members.
enum MembershipCheck {
    /// The DS member profiles. Used for regular (T) groups.
    MemberProfiles,
    /// The MLS ratchet tree. Used for PQ groups, whose member profiles are
    /// not maintained.
    RatchetTree,
}

impl DsGroupState {
    pub(crate) fn delete_group(
        &mut self,
        commit: AssistedMessageIn,
    ) -> Result<SerializedMlsMessage, GroupDeletionError> {
        self.delete_group_inner(commit, MembershipCheck::MemberProfiles)
    }

    /// Same as [`Self::delete_group`], but validates the removals against the
    /// MLS ratchet tree instead of the member profiles.
    pub(crate) fn delete_pq_group(
        &mut self,
        commit: AssistedMessageIn,
    ) -> Result<SerializedMlsMessage, GroupDeletionError> {
        self.delete_group_inner(commit, MembershipCheck::RatchetTree)
    }

    fn delete_group_inner(
        &mut self,
        commit: AssistedMessageIn,
        membership_check: MembershipCheck,
    ) -> Result<SerializedMlsMessage, GroupDeletionError> {
        // Process message (but don't apply it yet). This performs mls-assist-level validations.
        let processed_assisted_message_plus = self
            .group()
            .process_assisted_message(self.provider.crypto(), commit)
            .map_err(|_| GroupDeletionError::ProcessingError)?;

        // Perform DS-level validation
        // Make sure that we have the right message type.
        let processed_message =
            if let ProcessedAssistedMessage::Commit(processed_message, _group_info) =
                &processed_assisted_message_plus.processed_assisted_message
            {
                processed_message
            } else {
                // This should be a commit.
                tracing::warn!("Received non-commit message for delete_group operation");
                return Err(GroupDeletionError::InvalidMessage);
            };

        let Sender::Member(sender_index) = processed_message.sender() else {
            // Delete group should be a regular commit
            tracing::warn!("Invalid sender");
            return Err(GroupDeletionError::InvalidMessage);
        };

        if let ProcessedMessageContent::StagedCommitMessage(staged_commit) =
            processed_message.content()
        {
            // Check that the commit only contains removes.
            if staged_commit.add_proposals().count() > 0
                || staged_commit.update_proposals().count() > 0
            {
                tracing::warn!("Found add or update proposals in delete group commit");
                return Err(GroupDeletionError::InvalidMessage);
            }
            // Process remove proposals, but only non-inline ones.

            // Note: The staged commit yields the remove proposals in no
            // particular order, so we compare sorted lists.
            let mut removed_clients: Vec<_> = removed_clients(staged_commit);
            removed_clients.sort_unstable();
            let existing_clients: Vec<_> = match membership_check {
                MembershipCheck::MemberProfiles => self
                    .member_profiles
                    .keys()
                    .filter(|index| index != &sender_index)
                    .copied()
                    .collect(),
                MembershipCheck::RatchetTree => self
                    .group()
                    .members()
                    .map(|member| member.index)
                    .filter(|index| index != sender_index)
                    .collect(),
            };
            // Check that we're indeed removing all the clients.
            if removed_clients != existing_clients {
                tracing::warn!(
                    ?removed_clients,
                    ?existing_clients,
                    "Incomplete remove proposals in delete group commit"
                );
                return Err(GroupDeletionError::InvalidMessage);
            }
        } else {
            tracing::warn!("Invalid message content");
            return Err(GroupDeletionError::InvalidMessage);
        }

        // Everything seems to be okay.
        // No need to do anything else here, since the group is getting deleted
        // anyway.

        Ok(processed_assisted_message_plus.serialized_mls_message)
    }
}
