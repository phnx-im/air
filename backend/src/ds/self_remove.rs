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
    openmls::prelude::{LeafNode, ProcessedMessageContent, Proposal, Sender},
    provider_traits::MlsAssistProvider,
};
use tracing::error;

impl DsGroupState {
    pub(crate) fn self_remove_client(
        &mut self,
        remove_proposal: AssistedMessageIn,
    ) -> Result<SerializedMlsMessage, ClientSelfRemovalError> {
        let (sender_leaf, serialized_mls_message) =
            self.self_remove_client_inner(remove_proposal)?;

        // Change the role of the sender to outsider.
        let sender = VerifiableClientCredential::from_basic_credential(sender_leaf.credential())
            .map_err(|error| {
                error!(%error, "Credential of sender is invalid");
                ClientSelfRemovalError::InvalidMessage
            })?;
        self.room_state_change_role(sender.user_id(), sender.user_id(), RoleIndex::Outsider)
            .ok_or(ClientSelfRemovalError::InvalidMessage)?;

        Ok(serialized_mls_message)
    }

    /// Same as [`Self::self_remove_client`], but does not change the room state.
    pub(crate) fn self_remove_client_without_room_state(
        &mut self,
        remove_proposal: AssistedMessageIn,
    ) -> Result<SerializedMlsMessage, ClientSelfRemovalError> {
        let (_sender_leaf, serialized_mls_message) =
            self.self_remove_client_inner(remove_proposal)?;
        Ok(serialized_mls_message)
    }

    fn self_remove_client_inner(
        &mut self,
        remove_proposal: AssistedMessageIn,
    ) -> Result<(&LeafNode, SerializedMlsMessage), ClientSelfRemovalError> {
        // Process message (but don't apply it yet). This performs
        // mls-assist-level validations and puts the proposal into mls-assist's
        // proposal store.
        let processed_assisted_message_plus = self
            .group()
            .process_assisted_message(self.provider.crypto(), remove_proposal)?;

        // Perform DS-level validation
        // Make sure that we have the right message type.
        let ProcessedAssistedMessage::NonCommit(processed_message) =
            &processed_assisted_message_plus.processed_assisted_message
        else {
            // This should be a proposal.
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

        let Proposal::SelfRemove = queued_proposal.proposal() else {
            return Err(ClientSelfRemovalError::InvalidMessage);
        };

        // We first accept the message into the group state ...
        self.group.accept_processed_message(
            self.provider.storage(),
            processed_assisted_message_plus.processed_assisted_message,
            Duration::days(USER_EXPIRATION_DAYS),
        )?;

        let serialized_mls_message = processed_assisted_message_plus.serialized_mls_message;

        // Store the proposal so we can send it to clients requesting external
        // commit info.
        self.proposals.push(serialized_mls_message.0.clone());

        // We remove the user and client profile only when the proposal is committed.

        // The leaf still exists at this point: the member is only removed once
        // the proposal is committed.
        let sender_leaf = self
            .group
            .leaf(sender_index)
            .ok_or(ClientSelfRemovalError::InvalidMessage)?;

        // Finally, we create the message for distribution.
        Ok((sender_leaf, serialized_mls_message))
    }
}
