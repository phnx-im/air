// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashSet;

use airprotos::client::virtual_client::{
    VIRTUAL_CLIENT_KP_UPLOAD_COMPONENT_ID, VirtualClientKeyPackageUpload,
};
use mimi_room_policy::RoleIndex;
use mls_assist::{
    group::{ApqProcessedAssistedMessagePlus, ProcessedAssistedMessage, apq::ApqGroupRef},
    messages::{AssistedMessageIn, AssistedWelcome, SerializedMlsMessage},
    openmls::{
        group::StagedCommit,
        prelude::{
            Extension, KeyPackage, LeafNodeIndex, OpenMlsProvider, ProcessedMessage,
            ProcessedMessageContent, Sender,
        },
    },
    openmls_rust_crypto::OpenMlsRustCrypto,
    provider_traits::MlsAssistProvider,
};

use apqmls::messages::ApqWelcome;

use aircommon::{
    credentials::VerifiableClientCredential,
    crypto::{
        aead::keys::{EncryptedUserProfileKey, GroupStateEarKey},
        hpke::{HpkeEncryptable, JoinerInfoEncryptionKey},
    },
    identifiers::QsReference,
    messages::{
        client_ds::{
            AadMessage, AadPayload, AddUsersInfo, ApqWelcomeBundle, DsJoinerInformation,
            GroupOperationParams, GroupOperationParamsAad, QsQueueMessagePayload, WelcomeBundle,
        },
        welcome_attribution_info::EncryptedWelcomeAttributionInfo,
    },
    mls_group_config::QS_CLIENT_REFERENCE_EXTENSION_TYPE,
    time::{Duration, TimeStamp},
    utils::removed_clients,
};
use tls_codec::DeserializeBytes;
use tracing::{error, warn};

use crate::{
    errors::GroupOperationError,
    messages::intra_backend::{DsFanOutMessage, DsFanOutPayload, VirtualClientAction},
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

struct TCommitValidation {
    sender_index: SenderIndex,
    added_users_state: Option<AddUsersState>,
    external_sender_information: Option<(EncryptedUserProfileKey, QsReference)>,
    removed_clients: Vec<LeafNodeIndex>,
}

pub(crate) struct ProcessedGroupOperation {
    pub serialized_message: SerializedMlsMessage,
    pub added_users_state: Option<AddUsersState>,
    pub virtual_client_action: Option<VirtualClientAction>,
}

pub(crate) struct ProcessedApqGroupOperation {
    pub serialized_message: SerializedMlsMessage,
    pub t_add_users_state: Option<AddUsersState>,
    pub pq_welcome: Option<AssistedWelcome>,
    pub virtual_client_action: Option<VirtualClientAction>,
}

impl DsGroupState {
    /// Perform DS-level validation
    fn validate_t_commit(
        &mut self,
        processed_message: &ProcessedMessage,
        add_users_info: Option<AddUsersInfo>,
        pq_group_state: Option<&DsGroupState>,
        pq_staged_commit: Option<&StagedCommit>,
    ) -> Result<TCommitValidation, GroupOperationError> {
        // Validate that the AAD includes enough encrypted credential chains
        let aad_message = AadMessage::tls_deserialize_exact_bytes(processed_message.tail_aad())
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

        let sender = VerifiableClientCredential::from_basic_credential(
            self.group
                .leaf(sender_index.leaf_index())
                .ok_or_else(|| {
                    error!("Leaf of sender not found");
                    GroupOperationError::InvalidMessage
                })?
                .credential(),
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
        let added_users_state = if !adds_users {
            None
        } else {
            let Some(add_users_info) = add_users_info else {
                warn!("Group operation adds users but no add users info is provided");
                return Err(GroupOperationError::InvalidMessage);
            };

            let add_users_state = validate_added_users(staged_commit, aad_payload, add_users_info)?;

            let mut pq_add_proposals = pq_staged_commit.map(|commit| commit.add_proposals());

            for ((added_key_package, _), _) in &add_users_state.added_users {
                let added_credential = VerifiableClientCredential::from_basic_credential(
                    added_key_package.leaf_node().credential(),
                )
                .map_err(|e| {
                    error!(%e, "Credential of added user is invalid");
                    GroupOperationError::InvalidMessage
                })?;

                if let Some(pq_adds_sig_keys) = pq_add_proposals.as_mut() {
                    let pq_add_proposal = pq_adds_sig_keys.next().ok_or_else(|| {
                        error!("PQ has fewer add proposals than T");
                        GroupOperationError::InvalidMessage
                    })?;
                    let pq_signature_key = pq_add_proposal
                        .add_proposal()
                        .key_package()
                        .leaf_node()
                        .signature_key();
                    if added_key_package.leaf_node().signature_key() != pq_signature_key {
                        error!("T and PQ added user signature keys do not match");
                        return Err(GroupOperationError::InvalidMessage);
                    }
                }

                self.room_state_change_role(
                    sender.user_id(),
                    added_credential.user_id(),
                    RoleIndex::Regular,
                )
                .ok_or(GroupOperationError::InvalidMessage)?;
            }

            if let Some(pq_adds_sig_keys) = pq_add_proposals.as_mut()
                && pq_adds_sig_keys.next().is_some()
            {
                error!("PQ has more add proposals than T");
                return Err(GroupOperationError::InvalidMessage);
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

        let removed_clients = removed_clients(staged_commit);

        for &removed_index in &removed_clients {
            if removed_index == sender_index.leaf_index() {
                return Err(GroupOperationError::InvalidMessage);
            }

            let removed_leaf = self.group.leaf(removed_index).ok_or_else(|| {
                error!("Leaf of removed user not found");
                GroupOperationError::InvalidMessage
            })?;

            if let Some(pq_group_state) = pq_group_state {
                let pq_removed_signature_key = pq_group_state
                    .group
                    .leaf(removed_index)
                    .ok_or_else(|| {
                        error!("Leaf of removed user not found in PQ group");
                        GroupOperationError::InvalidMessage
                    })?
                    .signature_key();
                if removed_leaf.signature_key() != pq_removed_signature_key {
                    error!("T and PQ removed user signature keys do not match");
                    return Err(GroupOperationError::InvalidMessage);
                }
            }

            let removed_credential =
                VerifiableClientCredential::from_basic_credential(removed_leaf.credential())
                    .map_err(|e| {
                        error!(%e, "Credential of removed user is invalid");
                        GroupOperationError::InvalidMessage
                    })?;

            self.room_state_change_role(
                sender.user_id(),
                removed_credential.user_id(),
                RoleIndex::Outsider,
            )
            .ok_or(GroupOperationError::InvalidMessage)?;
        }

        Ok(TCommitValidation {
            sender_index,
            added_users_state,
            external_sender_information,
            removed_clients,
        })
    }

    // TODO: Make into a sans-io-style state machine
    pub(crate) async fn process_group_operation(
        &mut self,
        params: GroupOperationParams,
    ) -> Result<ProcessedGroupOperation, GroupOperationError> {
        // Process message (but don't apply it yet). This performs mls-assist-level validations.
        let processed_assisted_message_plus = self
            .group
            .process_assisted_message(self.provider.crypto(), params.commit)?;

        // Make sure that we have the right message type.
        let ProcessedAssistedMessage::Commit(processed_message, _group_info) =
            &processed_assisted_message_plus.processed_assisted_message
        else {
            // This should be a commit.
            warn!("Group operation is not a commit");
            return Err(GroupOperationError::InvalidMessage);
        };

        // Extract the virtual client action if present
        let virtual_client_action = extract_virtual_client_action(processed_message)?;

        let TCommitValidation {
            sender_index,
            added_users_state,
            external_sender_information,
            removed_clients,
        } = self.validate_t_commit(processed_message, params.add_users_info_option, None, None)?;

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

        // Update membership profiles for added users
        if let Some(add_users_state) = &added_users_state {
            self.update_membership_profiles(&add_users_state.added_users)?;
        }

        // Process resync operations
        if let Some((encrypted_user_profile_key, client_queue_config)) = external_sender_information
        {
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

        Ok(ProcessedGroupOperation {
            serialized_message: processed_assisted_message_plus.serialized_mls_message,
            added_users_state,
            virtual_client_action,
        })
    }

    /// Returns (serialized message, T added users state, PQ welcome info)
    pub(crate) fn process_apq_group_operation(
        t_group_state: &mut DsGroupState,
        pq_group_state: &mut DsGroupState,
        t_message: AssistedMessageIn,
        pq_message: AssistedMessageIn,
        t_add_users_info: Option<AddUsersInfo>,
        pq_add_users_info: Option<AddUsersInfo>,
    ) -> Result<ProcessedApqGroupOperation, GroupOperationError> {
        let crypto = t_group_state.provider.crypto();
        let ApqProcessedAssistedMessagePlus {
            processed_assisted_message,
            serialized_apq_message,
        } = ApqGroupRef::from_groups(&mut t_group_state.group, &mut pq_group_state.group)
            .process_apq_assisted_message(crypto, t_message, pq_message, |_, _| true)?;

        // PQ-side validation: sender type, self-remove check, welcome
        let (pq_staged_commit, pq_welcome) = {
            let ProcessedMessageContent::StagedCommitMessage(pq_staged_commit) =
                processed_assisted_message
                    .processed_message
                    .pq_message
                    .content()
            else {
                warn!("PQ message content is not a staged commit");
                return Err(GroupOperationError::InvalidMessage);
            };
            let pq_sender_index = match processed_assisted_message
                .processed_message
                .pq_message
                .sender()
            {
                Sender::Member(leaf_index) => SenderIndex::Member(*leaf_index),
                Sender::NewMemberCommit => {
                    let Some(remove_proposal) = pq_staged_commit.remove_proposals().next() else {
                        warn!("PQ external commit is not a resync operation");
                        return Err(GroupOperationError::InvalidMessage);
                    };
                    SenderIndex::External(remove_proposal.remove_proposal().removed())
                }
                Sender::External(_) | Sender::NewMemberProposal => {
                    warn!("PQ group operation must be a member or external commit");
                    return Err(GroupOperationError::InvalidMessage);
                }
            };
            for removed in removed_clients(pq_staged_commit) {
                if removed == pq_sender_index.leaf_index() {
                    return Err(GroupOperationError::InvalidMessage);
                }
            }
            let pq_welcome = if pq_staged_commit.add_proposals().count() != 0 {
                let Some(pq_add_users_info) = pq_add_users_info else {
                    warn!("PQ group operation adds users but no add users info is provided");
                    return Err(GroupOperationError::InvalidMessage);
                };
                validate_welcome_only(pq_staged_commit, &pq_add_users_info.welcome)?;
                Some(pq_add_users_info.welcome)
            } else {
                None
            };
            (pq_staged_commit, pq_welcome)
        };

        let TCommitValidation {
            sender_index: t_sender_index,
            added_users_state: t_add_users_state,
            external_sender_information,
            removed_clients: t_removed_clients,
        } = t_group_state.validate_t_commit(
            &processed_assisted_message.processed_message.t_message,
            t_add_users_info,
            Some(pq_group_state),
            Some(pq_staged_commit),
        )?;

        // Extract the virtual client action if present
        let virtual_client_action =
            extract_virtual_client_action(&processed_assisted_message.processed_message.t_message)?;

        // Everything seems to be okay.
        // Now we have to update the group state and distribute.

        // We first accept the message into the group state ...
        ApqGroupRef::from_groups(&mut t_group_state.group, &mut pq_group_state.group)
            .accept_apq_processed_message(
                t_group_state.provider.storage(),
                pq_group_state.provider.storage(),
                processed_assisted_message,
                Duration::days(USER_EXPIRATION_DAYS),
            )?;

        // Process removes
        t_group_state.remove_profiles(t_removed_clients);

        // Update membership profiles for added users
        if let Some(ref add_users_state) = t_add_users_state {
            t_group_state.update_membership_profiles(&add_users_state.added_users)?;
        }

        // Process resync operations
        if let Some((encrypted_user_profile_key, client_queue_config)) = external_sender_information
        {
            let client_profile = MemberProfile {
                leaf_index: t_sender_index.leaf_index(),
                client_queue_config,
                activity_time: TimeStamp::now(),
                activity_epoch: t_group_state.group().epoch(),
                encrypted_user_profile_key,
            };
            t_group_state
                .member_profiles
                .insert(t_sender_index.leaf_index(), client_profile);
        }

        Ok(ProcessedApqGroupOperation {
            serialized_message: serialized_apq_message,
            t_add_users_state,
            pq_welcome,
            virtual_client_action,
        })
    }

    pub(crate) async fn group_operation(
        &mut self,
        params: GroupOperationParams,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<
        (
            SerializedMlsMessage,
            Vec<DsFanOutMessage>,
            Option<VirtualClientAction>,
        ),
        GroupOperationError,
    > {
        let ProcessedGroupOperation {
            serialized_message,
            added_users_state,
            virtual_client_action,
        } = self.process_group_operation(params).await?;

        let fan_out_messages = added_users_state
            .map(
                |AddUsersState {
                     added_users,
                     welcome,
                 }| {
                    self.generate_fan_out_messages(added_users, group_state_ear_key, &welcome)
                },
            )
            .transpose()?
            .unwrap_or_default();

        Ok((serialized_message, fan_out_messages, virtual_client_action))
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
                suppress_notifications: false.into(),
                virtual_client_action: None,
            };
            fan_out_messages.push(fan_out_message);
        }

        Ok(fan_out_messages)
    }

    pub(crate) fn generate_apq_fan_out_messages(
        &self,
        added_users: Vec<(AddedUserInfo, EncryptedWelcomeAttributionInfo)>,
        t_welcome: &AssistedWelcome,
        pq_welcome: &AssistedWelcome,
        ear_key: &GroupStateEarKey,
    ) -> Result<Vec<DsFanOutMessage>, GroupOperationError> {
        let mut fan_out_messages = vec![];
        for ((t_key_package, _), attribution_info) in added_users.into_iter() {
            let client_queue_config = QsReference::tls_deserialize_exact_bytes(
                t_key_package
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
                t_key_package.hpke_init_key().clone().into();
            let encrypted_joiner_info = DsJoinerInformation {
                group_state_ear_key: ear_key.clone(),
            }
            .encrypt(&encryption_key, info, aad);
            let welcome = ApqWelcome::new(t_welcome.welcome.clone(), pq_welcome.welcome.clone());
            let welcome_bundle = ApqWelcomeBundle {
                welcome,
                encrypted_attribution_info: attribution_info,
                encrypted_joiner_info,
            };
            let fan_out_message = DsFanOutMessage {
                payload: DsFanOutPayload::QueueMessage(
                    welcome_bundle
                        .try_into()
                        .map_err(|_| GroupOperationError::LibraryError)?,
                ),
                client_reference: client_queue_config,
                suppress_notifications: false.into(),
                virtual_client_action: None,
            };
            fan_out_messages.push(fan_out_message);
        }
        Ok(fan_out_messages)
    }

    /// Removes user and client profiles based on the list of removed clients.
    pub(crate) fn remove_profiles(&mut self, removed_clients: Vec<LeafNodeIndex>) {
        for client_index in removed_clients {
            let removed_client_profile_option = self.member_profiles.remove(&client_index);
            debug_assert!(removed_client_profile_option.is_some());
        }
    }

    pub(super) fn create_commit_response(
        &self,
        sender_index: LeafNodeIndex,
        timestamp: TimeStamp,
    ) -> Result<DsFanOutMessage, GroupOperationError> {
        // Fan the response to this commit out into the sender's queue.
        let commit_response = QsQueueMessagePayload::ds_commit_response(
            self.group.group_info().group_context().group_id().clone(),
            (self.group.epoch().as_u64() - 1).into(),
            timestamp,
        )
        .map_err(|e| {
            warn!(error = %e, "Error serializing commit response");
            GroupOperationError::LibraryError
        })?;
        let payload = DsFanOutPayload::QueueMessage(commit_response);
        let sender_client_reference = self
            .member_profiles
            .get(&sender_index)
            .ok_or(GroupOperationError::InvalidMessage)?
            .client_queue_config
            .clone();
        let response = DsFanOutMessage {
            payload,
            client_reference: sender_client_reference,
            suppress_notifications: true.into(),
            virtual_client_action: None,
        };
        Ok(response)
    }
}

/// Extract the virtual client action if present from the Safe AAD part of the message.
fn extract_virtual_client_action(
    processed_message: &ProcessedMessage,
) -> Result<Option<VirtualClientAction>, GroupOperationError> {
    processed_message
        .safe_aad_item(VIRTUAL_CLIENT_KP_UPLOAD_COMPONENT_ID)
        .map(
            |bytes| -> Result<VirtualClientAction, GroupOperationError> {
                let VirtualClientKeyPackageUpload { epoch_id, random } =
                    DeserializeBytes::tls_deserialize_exact_bytes(bytes).map_err(|_| {
                        error!("Failed to deserialize virtual client action");
                        GroupOperationError::InvalidMessage
                    })?;
                Ok(VirtualClientAction::PromoteStagedKeyPackages { epoch_id, random })
            },
        )
        .transpose()
}

pub(crate) type AddedUserInfo = (KeyPackage, EncryptedUserProfileKey);

pub(crate) struct AddUsersState {
    pub(crate) added_users: Vec<(AddedUserInfo, EncryptedWelcomeAttributionInfo)>,
    pub(crate) welcome: AssistedWelcome,
}

/// Checks that each add proposal has a corresponding Welcome entry and vice versa.
fn validate_welcome_only(
    staged_commit: &StagedCommit,
    welcome: &AssistedWelcome,
) -> Result<(), GroupOperationError> {
    let mut remaining_welcomes = welcome.joiners().collect::<HashSet<_>>();

    if staged_commit
        .add_proposals()
        .map(|ap| {
            ap.add_proposal()
                .key_package()
                .hash_ref(OpenMlsRustCrypto::default().crypto())
        })
        .any(|add_proposal_ref| {
            let Ok(hash_ref) = add_proposal_ref else {
                return true;
            };
            !remaining_welcomes.remove(&hash_ref)
        })
    {
        return Err(GroupOperationError::IncompleteWelcome);
    }

    if !remaining_welcomes.is_empty() {
        return Err(GroupOperationError::IncompleteWelcome);
    }

    Ok(())
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

    validate_welcome_only(staged_commit, &add_users_info.welcome)?;

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
