// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::group_state::DsGroupState;
use super::process::USER_EXPIRATION_DAYS;
use crate::errors::ClientSelfRemovalError;
use aircommon::{credentials::VerifiableClientCredential, time::Duration};
use mimi_room_policy::RoleIndex;
use mls_assist::{
    group::ProcessedAssistedMessage,
    messages::{AssistedMessageIn, SerializedMlsMessage},
    openmls::prelude::{ProcessedMessageContent, Proposal, Sender},
    provider_traits::MlsAssistProvider,
};

impl DsGroupState {
    pub(crate) fn self_remove_client(
        &mut self,
        remove_proposal: AssistedMessageIn,
    ) -> Result<SerializedMlsMessage, ClientSelfRemovalError> {
        // Process message (but don't apply it yet). This performs
        // mls-assist-level validations and puts the proposal into mls-assist's
        // proposal store.
        // Process message (but don't apply it yet). This performs mls-assist-level validations.
        let processed_assisted_message_plus = self
            .group()
            .process_assisted_message(self.provider.crypto(), remove_proposal)
            .map_err(|_| ClientSelfRemovalError::ProcessingError)?;

        // Perform DS-level validation
        // Make sure that we have the right message type.
        let ProcessedAssistedMessage::NonCommit(processed_message) =
            &processed_assisted_message_plus.processed_assisted_message
        else {
            // This should be a commit.
            return Err(ClientSelfRemovalError::InvalidMessage);
        };

        // Check if sender index and user profile match.
        let Sender::Member(sender_index) = *processed_message.sender() else {
            // The remove proposal should come from a member.
            return Err(ClientSelfRemovalError::InvalidMessage);
        };

        let ProcessedMessageContent::ProposalMessage(queued_proposal) = processed_message.content()
        else {
            return Err(ClientSelfRemovalError::InvalidMessage);
        };

        let Proposal::Remove(remove_proposal) = queued_proposal.proposal() else {
            return Err(ClientSelfRemovalError::InvalidMessage);
        };

        if remove_proposal.removed() != sender_index {
            return Err(ClientSelfRemovalError::InvalidMessage);
        }

        // Everything seems to be okay.
        // Now we have to update the group state and distribute.
        let sender = VerifiableClientCredential::try_from(
            self.group.leaf(sender_index).unwrap().credential().clone(),
        )
        .unwrap();

        self.room_state_change_role(sender.user_id(), sender.user_id(), RoleIndex::Outsider)
            .ok_or(ClientSelfRemovalError::InvalidMessage)?;

        // We first accept the message into the group state ...
        self.group.accept_processed_message(
            self.provider.storage(),
            processed_assisted_message_plus.processed_assisted_message,
            Duration::days(USER_EXPIRATION_DAYS),
        )?;

        // We remove the user and client profile only when the proposal is committed.

        // Finally, we create the message for distribution.
        Ok(processed_assisted_message_plus.serialized_mls_message)
    }
}
