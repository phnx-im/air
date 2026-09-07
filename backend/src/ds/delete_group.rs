// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::utils::removed_clients;
use mls_assist::{
    group::{ApqProcessedAssistedMessagePlus, ProcessedAssistedMessage, apq::ApqGroupRef},
    messages::{AssistedMessageIn, SerializedMlsMessage},
    openmls::prelude::{ProcessedMessage, ProcessedMessageContent, Sender},
    provider_traits::MlsAssistProvider,
};
use tracing::warn;

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
        // Process message (but don't apply it yet). This performs mls-assist-level validations.
        let processed_assisted_message_plus = self
            .group()
            .process_assisted_message(self.provider.crypto(), commit)
            .map_err(|_| GroupDeletionError::ProcessingError)?;

        // Perform DS-level validation
        // Make sure that we have the right message type.
        let ProcessedAssistedMessage::Commit(processed_message, _group_info) =
            &processed_assisted_message_plus.processed_assisted_message
        else {
            // This should be a commit.
            warn!("Received non-commit message for delete_group operation");
            return Err(GroupDeletionError::InvalidMessage);
        };

        self.validate_delete_commit(processed_message, MembershipCheck::MemberProfiles)?;

        // Everything seems to be okay.
        // No need to do anything else here, since the group is getting deleted
        // anyway.

        Ok(processed_assisted_message_plus.serialized_mls_message)
    }

    /// Deletes an APQ group, i.e. both of its legs.
    ///
    /// The two commits go through the APQ paired path, so they have to agree on
    /// the session before either of them is looked at on its own. The removals
    /// are then validated per leg: the T leg against the DS member profiles and
    /// the PQ leg against the ratchet tree, since the DS keeps no member
    /// profiles for PQ groups.
    pub(crate) fn delete_apq_group(
        t_group_state: &mut Self,
        pq_group_state: &mut Self,
        t_message: AssistedMessageIn,
        pq_message: AssistedMessageIn,
    ) -> Result<SerializedMlsMessage, GroupDeletionError> {
        // Process both legs as a unit (but don't apply them). This performs the
        // mls-assist- and APQ-level validations.
        let ApqProcessedAssistedMessagePlus {
            processed_assisted_message,
            serialized_apq_message,
        } = ApqGroupRef::from_groups(&mut t_group_state.group, &mut pq_group_state.group)
            .process_apq_assisted_message(
                t_group_state.provider.crypto(),
                t_message,
                pq_message,
                |_, _| true,
            )
            .map_err(|error| {
                warn!(%error, "Failed to process APQ delete group commit");
                GroupDeletionError::ProcessingError
            })?;

        // Perform DS-level validation on each leg against its own source of
        // truth for the group's membership.
        let apq_processed_message = &processed_assisted_message.processed_message;
        t_group_state.validate_delete_commit(
            &apq_processed_message.t_message,
            MembershipCheck::MemberProfiles,
        )?;
        pq_group_state.validate_delete_commit(
            &apq_processed_message.pq_message,
            MembershipCheck::RatchetTree,
        )?;

        // Everything seems to be okay.
        // No need to do anything else here, since the group is getting deleted
        // anyway.

        Ok(serialized_apq_message)
    }

    /// Checks that the commit is a member commit that removes every member of
    /// the group except its sender.
    fn validate_delete_commit(
        &self,
        processed_message: &ProcessedMessage,
        membership_check: MembershipCheck,
    ) -> Result<(), GroupDeletionError> {
        let Sender::Member(sender_index) = processed_message.sender() else {
            // Delete group should be a regular commit
            warn!("Invalid sender");
            return Err(GroupDeletionError::InvalidMessage);
        };

        let ProcessedMessageContent::StagedCommitMessage(staged_commit) =
            processed_message.content()
        else {
            warn!("Invalid message content");
            return Err(GroupDeletionError::InvalidMessage);
        };

        // Check that the commit only contains removes.
        if staged_commit.add_proposals().count() > 0 || staged_commit.update_proposals().count() > 0
        {
            warn!("Found add or update proposals in delete group commit");
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
            warn!(
                ?removed_clients,
                ?existing_clients,
                "Incomplete remove proposals in delete group commit"
            );
            return Err(GroupDeletionError::InvalidMessage);
        }

        Ok(())
    }
}
