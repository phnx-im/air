// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashSet;

use mimi_room_policy::RoleIndex;
use mls_assist::{
    group::ProcessedAssistedMessage,
    messages::{AssistedWelcome, SerializedMlsMessage},
    openmls::{
        group::StagedCommit,
        prelude::{
            Extension, KeyPackage, LeafNodeIndex, OpenMlsProvider, ProcessedMessageContent, Sender,
        },
    },
    openmls_rust_crypto::OpenMlsRustCrypto,
    provider_traits::MlsAssistProvider,
};

use aircommon::{
    credentials::VerifiableClientCredential,
    crypto::{
        ear::keys::{EncryptedUserProfileKey, GroupStateEarKey},
        hpke::{HpkeEncryptable, JoinerInfoEncryptionKey},
    },
    identifiers::QsReference,
    messages::{
        client_ds::{
            AadMessage, AadPayload, AddUsersInfo, DsJoinerInformation, GroupOperationParams,
            GroupOperationParamsAad, WelcomeBundle,
        },
        welcome_attribution_info::EncryptedWelcomeAttributionInfo,
    },
    mls_group_config::QS_CLIENT_REFERENCE_EXTENSION_TYPE,
    time::{Duration, TimeStamp},
};
use tls_codec::DeserializeBytes;
use tracing::{error, warn};

use crate::{
    errors::GroupOperationError,
    messages::intra_backend::{DsFanOutMessage, DsFanOutPayload},
};

use super::{group_state::MemberProfile, process::USER_EXPIRATION_DAYS};

use super::group_state::DsGroupState;

#[derive(Clone, Copy)]
enum SenderIndex {
    Member(LeafNodeIndex),
    External(LeafNodeIndex),
}

impl SenderIndex {
    fn leaf_index(&self) -> LeafNodeIndex {
        match self {
            SenderIndex::Member(leaf_index) => *leaf_index,
            SenderIndex::External(leaf_index) => *leaf_index,
        }
    }
}

impl DsGroupState {
    // TODO: Structured logging
    // TODO: Make into a sans-io-style state machine
    pub(crate) async fn group_operation(
        &mut self,
        params: GroupOperationParams,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<(SerializedMlsMessage, Vec<DsFanOutMessage>), GroupOperationError> {
        // Process message (but don't apply it yet). This performs mls-assist-level validations.
        let processed_assisted_message_plus = self
            .group
            .process_assisted_message(self.provider.crypto(), params.commit)
            .map_err(|e| {
                warn!(%e, "Error processing assisted message");
                GroupOperationError::ProcessingError
            })?;

        // Perform DS-level validation
        // Make sure that we have the right message type.
        let ProcessedAssistedMessage::Commit(processed_message, _group_info) =
            &processed_assisted_message_plus.processed_assisted_message
        else {
            // This should be a commit.
            warn!("Group operation is not a commit");
            return Err(GroupOperationError::InvalidMessage);
        };

        // Validate that the AAD includes enough encrypted credential chains
        let aad_message = AadMessage::tls_deserialize_exact_bytes(processed_message.aad())
            .map_err(|e| {
                warn!(%e, "Error deserializing AAD message");
                GroupOperationError::InvalidMessage
            })?;
        // TODO: Check version of Aad Message
        let AadPayload::GroupOperation(aad_payload) = aad_message.into_payload() else {
            warn!("AAD payload is not a group operation");
            return Err(GroupOperationError::InvalidMessage);
        };

        // Extract the message's content
        let ProcessedMessageContent::StagedCommitMessage(staged_commit) =
            processed_message.content()
        else {
            warn!("Processed message content is not a staged commit");
            return Err(GroupOperationError::InvalidMessage);
        };

        // Perform validation depending on the type of message
        let sender_index = match processed_message.sender() {
            Sender::Member(leaf_index) => SenderIndex::Member(*leaf_index),
            Sender::NewMemberCommit => {
                // If it's an external commit, it has to be a resync operation,
                // which means there MUST be a remove proposal for the sender's
                // original client. That client MUST be removed in the commit
                // and that client MUST have a user profile associated with it.
                let Some(remove_proposal) = staged_commit.remove_proposals().next() else {
                    warn!("External commit is not a resync operation");
                    return Err(GroupOperationError::InvalidMessage);
                };
                SenderIndex::External(remove_proposal.remove_proposal().removed())
            }
            // A group operation must be a commit.
            Sender::External(_) | Sender::NewMemberProposal => {
                warn!("A group operation must be a commit");
                return Err(GroupOperationError::InvalidMessage);
            }
        };

        let sender = VerifiableClientCredential::try_from(
            self.group
                .leaf(sender_index.leaf_index())
                .ok_or_else(|| {
                    error!("Leaf of sender not found");
                    GroupOperationError::InvalidMessage
                })?
                .credential()
                .clone(),
        )
        .map_err(|e| {
            error!(%e, "Credential in leaf of sender is invalid");
            GroupOperationError::InvalidMessage
        })?;

        // Check if the operation adds a user.
        let adds_users = staged_commit.add_proposals().count() != 0;

        // TODO: Validate that the senders of the proposals have sufficient
        //       privileges (if this isn't done by an MLS extension). Note that
        //       we have to check the sender of the proposals not those of the
        //       commit.

        // Validation related to adding users
        let added_users_state_option = if !adds_users {
            None
        } else {
            let Some(add_users_info) = params.add_users_info_option else {
                warn!("Group operation adds users but no add users info is provided");
                return Err(GroupOperationError::InvalidMessage);
            };

            let add_users_state = validate_added_users(staged_commit, aad_payload, add_users_info)?;

            for ((added_key_package, _), _) in &add_users_state.added_users {
                let added = VerifiableClientCredential::try_from(
                    added_key_package.leaf_node().credential().clone(),
                )
                .map_err(|e| {
                    error!(%e, "Credential of added user is invalid");
                    GroupOperationError::InvalidMessage
                })?;

                self.room_state_change_role(sender.user_id(), added.user_id(), RoleIndex::Regular)
                    .ok_or(GroupOperationError::InvalidMessage)?;
            }

            Some(add_users_state)
        };

        // Validation related to resync operations
        let external_sender_information = match sender_index {
            SenderIndex::External(original_index) => {
                // Make sure there is a remove proposal for the original client.
                if staged_commit.remove_proposals().count() == 0 {
                    warn!("External commit is not a resync operation");
                    return Err(GroupOperationError::InvalidMessage);
                }
                // Collect the encrypted client information and the client queue
                // config of the original client. We need this later to create
                // the new client profile.
                let sender_profile = self
                    .member_profiles
                    .get(&original_index)
                    .ok_or(GroupOperationError::InvalidMessage)?;
                let encrypted_user_profile_key = sender_profile.encrypted_user_profile_key.clone();
                // Get the queue config from the leaf node extensions.
                let client_queue_config = staged_commit
                    .update_path_leaf_node()
                    .ok_or(GroupOperationError::InvalidMessage)?
                    .extensions()
                    .iter()
                    .find_map(|e| match e {
                        Extension::Unknown(QS_CLIENT_REFERENCE_EXTENSION_TYPE, bytes) => {
                            let extension = QsReference::tls_deserialize_exact_bytes(&bytes.0)
                                .map_err(|e| {
                                    warn!(%e, "Error deserializing client reference");
                                    GroupOperationError::InvalidMessage
                                });
                            Some(extension)
                        }
                        _ => None,
                    })
                    .ok_or(GroupOperationError::InvalidMessage)??;
                Some((encrypted_user_profile_key, client_queue_config))
            }
            _ => None,
        };

        let removed_clients = staged_commit
            .remove_proposals()
            .map(|remove_proposal| remove_proposal.remove_proposal().removed())
            .collect::<Vec<_>>();

        for removed in &removed_clients {
            if *removed == sender_index.leaf_index() {
                return Err(GroupOperationError::InvalidMessage);
            }

            let removed = VerifiableClientCredential::try_from(
                self.group
                    .leaf(*removed)
                    .ok_or_else(|| {
                        error!("Leaf of removed user not found");
                        GroupOperationError::InvalidMessage
                    })?
                    .credential()
                    .clone(),
            )
            .map_err(|e| {
                error!(%e, "Credential of removed user is invalid");
                GroupOperationError::InvalidMessage
            })?;

            self.room_state_change_role(sender.user_id(), removed.user_id(), RoleIndex::Outsider)
                .ok_or(GroupOperationError::InvalidMessage)?;
        }

        // Everything seems to be okay.
        // Now we have to update the group state and distribute.

        // We first accept the message into the group state ...
        self.group.accept_processed_message(
            self.provider.storage(),
            processed_assisted_message_plus.processed_assisted_message,
            Duration::days(USER_EXPIRATION_DAYS),
        )?;

        // Process removes
        self.remove_profiles(removed_clients);

        // ... s.t. it's easier to update the user and client profiles.

        let mut fan_out_messages: Vec<DsFanOutMessage> = vec![];

        // If users were added, update the list of user profiles
        if let Some(AddUsersState {
            added_users,
            welcome,
        }) = added_users_state_option
        {
            self.update_membership_profiles(&added_users)?;
            fan_out_messages.extend(self.generate_fan_out_messages(
                added_users,
                group_state_ear_key,
                &welcome,
            )?);
        }

        // Process resync operations
        if let Some((encrypted_user_profile_key, client_queue_config)) = external_sender_information
        {
            // The original client profile was already removed. We just have to
            // add the new one.
            let client_profile = MemberProfile {
                leaf_index: sender_index.leaf_index(),
                client_queue_config,
                activity_time: TimeStamp::now(),
                activity_epoch: self.group().epoch(),
                encrypted_user_profile_key,
            };
            self.member_profiles
                .insert(sender_index.leaf_index(), client_profile);
        }

        // Finally, we create the message for distribution.
        Ok((
            processed_assisted_message_plus.serialized_mls_message,
            fan_out_messages,
        ))
    }

    /// Updates client and user profiles based on the added users.
    fn update_membership_profiles(
        &mut self,
        added_users: &[(AddedUserInfo, EncryptedWelcomeAttributionInfo)],
    ) -> Result<(), GroupOperationError> {
        let mut client_profiles = vec![];
        for ((key_package, encrypted_user_profile_key), _) in added_users.iter() {
            let member = self
                .group()
                .members()
                .find(|m| m.signature_key == key_package.leaf_node().signature_key().as_slice())
                .ok_or(GroupOperationError::InvalidMessage)?;
            let leaf_index = member.index;
            let client_queue_config = QsReference::tls_deserialize_exact_bytes(
                key_package
                    .leaf_node()
                    .extensions()
                    .iter()
                    .find_map(|e| match e {
                        Extension::Unknown(QS_CLIENT_REFERENCE_EXTENSION_TYPE, bytes) => {
                            Some(&bytes.0)
                        }
                        _ => None,
                    })
                    .ok_or(GroupOperationError::MissingQueueConfig)?
                    .as_slice(),
            )
            .map_err(|_| GroupOperationError::MissingQueueConfig)?;
            let client_profile = MemberProfile {
                leaf_index,
                encrypted_user_profile_key: encrypted_user_profile_key.clone(),
                client_queue_config: client_queue_config.clone(),
                activity_time: TimeStamp::now(),
                activity_epoch: self.group().epoch(),
            };
            client_profiles.push(client_profile);
        }

        for client_profile in client_profiles.into_iter() {
            self.member_profiles
                .insert(client_profile.leaf_index, client_profile);
        }

        Ok(())
    }

    fn generate_fan_out_messages(
        &self,
        added_users: Vec<(AddedUserInfo, EncryptedWelcomeAttributionInfo)>,
        group_state_ear_key: &GroupStateEarKey,
        welcome: &AssistedWelcome,
    ) -> Result<Vec<DsFanOutMessage>, GroupOperationError> {
        let mut fan_out_messages = vec![];
        for ((key_package, _), attribution_info) in added_users.into_iter() {
            let client_queue_config = QsReference::tls_deserialize_exact_bytes(
                key_package
                    .leaf_node()
                    .extensions()
                    .iter()
                    .find_map(|e| match e {
                        Extension::Unknown(QS_CLIENT_REFERENCE_EXTENSION_TYPE, bytes) => {
                            Some(&bytes.0)
                        }
                        _ => None,
                    })
                    .ok_or(GroupOperationError::MissingQueueConfig)?
                    .as_slice(),
            )
            .map_err(|_| GroupOperationError::MissingQueueConfig)?;
            let info = &[];
            let aad = &[];
            let encryption_key: JoinerInfoEncryptionKey =
                key_package.hpke_init_key().clone().into();
            let encrypted_joiner_info = DsJoinerInformation {
                group_state_ear_key: group_state_ear_key.clone(),
            }
            .encrypt(&encryption_key, info, aad);
            let welcome_bundle = WelcomeBundle {
                welcome: welcome.clone(),
                encrypted_attribution_info: attribution_info.clone(),
                encrypted_joiner_info,
            };
            let fan_out_message = DsFanOutMessage {
                payload: DsFanOutPayload::QueueMessage(
                    welcome_bundle
                        .try_into()
                        .map_err(|_| GroupOperationError::LibraryError)?,
                ),
                client_reference: client_queue_config,
                // No notifications for welcome messages
                suppress_notifications: true.into(),
            };
            fan_out_messages.push(fan_out_message);
        }

        Ok(fan_out_messages)
    }

    /// Removes user and client profiles based on the list of removed clients.
    fn remove_profiles(&mut self, removed_clients: Vec<LeafNodeIndex>) {
        for client_index in removed_clients {
            let removed_client_profile_option = self.member_profiles.remove(&client_index);
            debug_assert!(removed_client_profile_option.is_some());
        }
    }
}

type AddedUserInfo = (KeyPackage, EncryptedUserProfileKey);

struct AddUsersState {
    added_users: Vec<(AddedUserInfo, EncryptedWelcomeAttributionInfo)>,
    welcome: AssistedWelcome,
}

fn validate_added_users(
    staged_commit: &StagedCommit,
    aad_payload: GroupOperationParamsAad,
    add_users_info: AddUsersInfo,
) -> Result<AddUsersState, GroupOperationError> {
    let number_of_added_users = staged_commit.add_proposals().count();
    // Check that the lengths of the various vectors match.
    if add_users_info.encrypted_welcome_attribution_infos.len() != number_of_added_users {
        return Err(GroupOperationError::InvalidMessage);
    }

    // Check if for each added member, there is a corresponding entry
    // in the Welcome.
    let mut remaining_welcomes = add_users_info.welcome.joiners().collect::<HashSet<_>>();

    if staged_commit
        .add_proposals()
        .map(|ap| {
            ap.add_proposal()
                .key_package()
                .hash_ref(OpenMlsRustCrypto::default().crypto())
        })
        .any(|add_proposal_ref| {
            // Hashing shouldn't fail, so we ignore it here.
            let Ok(hash_ref) = add_proposal_ref else {
                return true;
            };
            !remaining_welcomes.remove(&hash_ref)
        })
    {
        return Err(GroupOperationError::IncompleteWelcome);
    }

    // Check if all welcomes had a corresponding add proposal
    if !remaining_welcomes.is_empty() {
        return Err(GroupOperationError::IncompleteWelcome);
    }

    let added_users = staged_commit
        .add_proposals()
        .map(|ap| ap.add_proposal().key_package().clone())
        .zip(aad_payload.new_encrypted_user_profile_keys)
        .zip(add_users_info.encrypted_welcome_attribution_infos)
        .collect::<Vec<_>>();

    Ok(AddUsersState {
        added_users,
        welcome: add_users_info.welcome,
    })
}
