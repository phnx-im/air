// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::HashMap, iter};

use super::{ClientVerificationInfo, Group, openmls_provider::AirOpenMlsProvider};
use aircommon::{
    credentials::{
        AsIntermediateCredential, AsIntermediateCredentialBody, ClientCredential,
        VerifiableClientCredential,
    },
    crypto::{ear::keys::EncryptedUserProfileKey, hash::Hash, indexed_aead::keys::UserProfileKey},
    identifiers::UserId,
    messages::client_ds::{AadMessage, AadPayload},
};
use anyhow::{Context, Result, anyhow, bail, ensure};
use mimi_room_policy::RoleIndex;
use sqlx::SqliteConnection;
use tls_codec::DeserializeBytes as TlsDeserializeBytes;
use tracing::debug;

use crate::{clients::api_clients::ApiClients, key_stores::as_credentials::AsCredentials};

use openmls::{
    group::QueuedAddProposal,
    prelude::{
        BasicCredentialError, LeafNodeIndex, ProcessedMessage, ProcessedMessageContent,
        ProtocolMessage, Sender, SignaturePublicKey, StagedCommit,
    },
};

use super::client_auth_info::{ClientAuthInfo, GroupMembership};

pub(crate) struct ProcessMessageResult {
    pub(crate) processed_message: ProcessedMessage,
    pub(crate) we_were_removed: bool,
    pub(crate) sender_client_credential: ClientCredential,
    pub(crate) profile_infos: Vec<(ClientCredential, UserProfileKey)>,
}

impl Group {
    /// Process inbound message
    ///
    /// Returns the processed message, whether the group was deleted, as well as
    /// the sender's client credential.
    pub(crate) async fn process_message(
        &mut self,
        connection: &mut SqliteConnection,
        api_clients: &ApiClients,
        message: impl Into<ProtocolMessage>,
    ) -> Result<ProcessMessageResult> {
        // Phase 1: Process the message.
        let processed_message = {
            let provider = AirOpenMlsProvider::new(&mut *connection);
            self.mls_group.process_message(&provider, message)?
        };

        let group_id = self.group_id().clone();

        // Will be set to true if we were removed (or the group was deleted).
        let mut we_were_removed = false;
        let mut encrypted_profile_infos: Vec<(ClientCredential, EncryptedUserProfileKey)> =
            Vec::new();
        let sender_index = match processed_message.content() {
            // For now, we only care about commits.
            ProcessedMessageContent::ExternalJoinProposalMessage(_) => {
                bail!("Unsupported message type")
            }
            ProcessedMessageContent::ApplicationMessage(_) => {
                debug!("process application message");
                let sender_client_credential =
                    if let Sender::Member(index) = processed_message.sender() {
                        ClientAuthInfo::load(&mut *connection, &group_id, *index)
                            .await?
                            .map(|info| info.into_client_credential())
                            .context("Could not find client credential of message sender")?
                    } else {
                        bail!("Invalid sender type.")
                    };
                return Ok(ProcessMessageResult {
                    processed_message,
                    we_were_removed,
                    sender_client_credential,
                    profile_infos: Vec::new(),
                });
            }
            ProcessedMessageContent::ProposalMessage(_proposal) => {
                // Proposals are just returned and can then be added to the
                // proposal store after the caller has inspected them.
                let Sender::Member(sender_index) = processed_message.sender() else {
                    bail!("Invalid sender type.")
                };

                // TODO: Room policy checks?

                *sender_index
            }
            ProcessedMessageContent::StagedCommitMessage(staged_commit) => {
                let sender_index = match processed_message.sender() {
                    Sender::Member(index) => index.to_owned(),
                    Sender::NewMemberCommit => {
                        self.mls_group.ext_commit_sender_index(staged_commit)?
                    }
                    Sender::External(_) | Sender::NewMemberProposal => {
                        bail!("Invalid sender type.")
                    }
                };

                let sender =
                    VerifiableClientCredential::try_from(processed_message.credential().clone())?;

                // StagedCommitMessage Phase 1: Process the proposals.

                // Before we process the AAD payload, we first process the
                // proposals by value. Currently only removes are allowed.
                for remove_proposal in staged_commit.remove_proposals() {
                    let removed_index = remove_proposal.remove_proposal().removed();

                    let removed_id = self
                        .client_by_index(connection, removed_index)
                        .await
                        .context("Unknown removed_id")?;

                    // Room policy checks
                    self.room_state_change_role(
                        sender.user_id(),
                        &removed_id,
                        RoleIndex::Outsider,
                    )?;

                    GroupMembership::stage_removal(&mut *connection, &group_id, removed_index)
                        .await?;
                    if removed_index == self.mls_group().own_leaf_index() {
                        we_were_removed = true;
                    }
                }

                // Phase 2: Process the AAD payload.
                // Let's figure out which operation this is meant to be.
                let aad_payload = AadMessage::tls_deserialize_exact_bytes(processed_message.aad())?
                    .into_payload();
                match aad_payload {
                    AadPayload::GroupOperation(group_operation_payload) => {
                        let number_of_adds = staged_commit.add_proposals().count();
                        let number_of_upks = group_operation_payload
                            .new_encrypted_user_profile_keys
                            .len();
                        ensure!(
                            number_of_adds == number_of_upks,
                            "Number of add proposals and user profile keys don't match"
                        );

                        // Process adds if there are any.
                        if !group_operation_payload
                            .new_encrypted_user_profile_keys
                            .is_empty()
                        {
                            let verifiable_credentials = staged_commit
                                .add_proposals()
                                .map(|ap| {
                                    let credential = ap
                                        .add_proposal()
                                        .key_package()
                                        .leaf_node()
                                        .credential()
                                        .clone();
                                    VerifiableClientCredential::try_from(credential)
                                })
                                .collect::<Result<Vec<_>, _>>()?;
                            let as_credentials = AsCredentials::fetch_for_verification(
                                &mut *connection,
                                api_clients,
                                verifiable_credentials.iter(),
                            )
                            .await?;
                            let client_auth_infos = self
                                .process_adds(
                                    sender.user_id(),
                                    staged_commit,
                                    &mut *connection,
                                    staged_commit.add_proposals(),
                                    &as_credentials,
                                )
                                .await?;
                            // Match up client credentials and new UserProfileKeys
                            let new_profile_infos: Vec<_> = client_auth_infos
                                .into_iter()
                                .map(|cai| cai.into_client_credential())
                                .zip(
                                    group_operation_payload
                                        .new_encrypted_user_profile_keys
                                        .into_iter(),
                                )
                                .collect();
                            encrypted_profile_infos.extend(new_profile_infos);
                        }

                        // Process updates if there are any.
                        // Check if the client has updated its leaf credential.
                        let (new_sender_credential, new_sender_leaf_key) =
                            update_path_leaf_node_info(staged_commit)?;

                        let as_credentials = AsCredentials::fetch_for_verification(
                            &mut *connection,
                            api_clients,
                            iter::once(&new_sender_credential),
                        )
                        .await?;

                        let old_credential = sender;

                        if new_sender_credential != old_credential {
                            self.process_update(
                                &mut *connection,
                                old_credential,
                                new_sender_credential,
                                sender_index,
                                new_sender_leaf_key,
                                &as_credentials,
                            )
                            .await?;
                        }

                        // Process a resync if this is one
                        if matches!(processed_message.sender(), Sender::NewMemberCommit) {
                            self.process_resync(
                                &processed_message,
                                staged_commit,
                                &mut *connection,
                                sender_index,
                            )
                            .await?;
                        }
                    }
                    AadPayload::JoinConnectionGroup(join_connection_group_payload) => {
                        // JoinConnectionGroup Phase 1: Decrypt and verify the
                        // client credential of the joiner
                        let (sender_credential, sender_leaf_key) =
                            update_path_leaf_node_info(staged_commit)?;

                        let as_credentials = AsCredentials::fetch_for_verification(
                            &mut *connection,
                            api_clients,
                            iter::once(&sender_credential),
                        )
                        .await?;

                        let client_auth_info = ClientAuthInfo::verify_credential(
                            &group_id,
                            sender_index,
                            sender_credential,
                            sender_leaf_key,
                            None, // Since the join is an external commit, we don't have an old credential.
                            &as_credentials,
                        )?;

                        // TODO: (More) validation:
                        // * Check that the user id is unique.
                        // * Check that the proposals fit the operation.
                        // * Check that the sender type fits the operation.
                        // * Check that this group is indeed a connection group.

                        // JoinConnectionGroup Phase 2: Persist the client auth info.
                        client_auth_info.stage_add(&mut *connection).await?;
                        encrypted_profile_infos.push((
                            client_auth_info.into_client_credential(),
                            join_connection_group_payload.encrypted_user_profile_key,
                        ));
                    }
                    AadPayload::Resync => {
                        // Check if it's an external commit. This implies that
                        // there is only one remove proposal.
                        ensure!(
                            matches!(processed_message.sender(), Sender::NewMemberCommit),
                            "Resync operation must be an external commit"
                        );

                        let (sender_credential, sender_leaf_key) =
                            update_path_leaf_node_info(staged_commit)?;

                        let removed_index = staged_commit
                            .remove_proposals()
                            .next()
                            .context("Resync operation did not contain a remove proposal")?
                            .remove_proposal()
                            .removed();

                        let old_credential = self
                            .mls_group
                            .member(removed_index)
                            .ok_or(anyhow!("Could not find removed member in group"))?;

                        let as_credentials = AsCredentials::fetch_for_verification(
                            &mut *connection,
                            api_clients,
                            iter::once(&sender_credential),
                        )
                        .await?;

                        let mut client_auth_info = ClientAuthInfo::verify_credential(
                            &group_id,
                            removed_index,
                            sender_credential,
                            sender_leaf_key,
                            Some(old_credential.clone().try_into()?),
                            &as_credentials,
                        )?;

                        // Set the client's new leaf index.
                        client_auth_info
                            .group_membership_mut()
                            .set_leaf_index(sender_index);
                        client_auth_info.stage_update(&mut *connection).await?;
                    }
                    AadPayload::DeleteGroup => {
                        we_were_removed = true;
                        // There is nothing else to do at this point.
                    }
                };
                sender_index
            }
        };
        // Get the sender's credential
        // If the sender is added to the group with this commit, we have to load
        // it from the DB with status "staged".

        // Phase 2: Load the sender's client credential.
        let sender_client_credential =
            if matches!(processed_message.sender(), Sender::NewMemberCommit) {
                ClientAuthInfo::load_staged(&mut *connection, &group_id, sender_index).await?
            } else {
                ClientAuthInfo::load(&mut *connection, &group_id, sender_index).await?
            }
            .context("Could not find client credential of message sender")?
            .client_credential()
            .clone()
            .into();

        // Decrypt any user profile keys
        let profile_infos = encrypted_profile_infos
            .into_iter()
            .map(|(client_credential, encrypted_user_profile_key)| {
                let user_profile_key = UserProfileKey::decrypt(
                    self.identity_link_wrapper_key(),
                    &encrypted_user_profile_key,
                    client_credential.identity(),
                )?;
                Ok((client_credential, user_profile_key))
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(ProcessMessageResult {
            processed_message,
            we_were_removed,
            sender_client_credential,
            profile_infos,
        })
    }

    async fn process_adds<'a>(
        &mut self,
        sender_user: &UserId,
        staged_commit: &StagedCommit,
        connection: &mut SqliteConnection,
        added_clients: impl Iterator<Item = QueuedAddProposal<'a>>,
        as_credentials: &HashMap<Hash<AsIntermediateCredentialBody>, AsIntermediateCredential>,
    ) -> Result<Vec<ClientAuthInfo>> {
        // AddUsers Phase 1: Compute the free indices
        let added_clients_with_indices =
            GroupMembership::free_indices(&mut *connection, &self.group_id)
                .await?
                .zip(added_clients)
                .collect::<Vec<_>>();

        let added_credentials = added_clients_with_indices
            .into_iter()
            .map(|(i, proposal)| {
                let leaf_node = proposal.add_proposal().key_package().leaf_node();
                Ok(ClientVerificationInfo {
                    leaf_index: i,
                    credential: VerifiableClientCredential::try_from(
                        leaf_node.credential().clone(),
                    )?,
                    leaf_key: leaf_node.signature_key().clone(),
                })
            })
            .collect::<Result<Vec<_>, BasicCredentialError>>()?;

        // AddUsers Phase 2: Decrypt and verify the client credentials.
        let client_auth_infos = ClientAuthInfo::verify_new_credentials(
            &self.group_id,
            added_credentials,
            as_credentials,
        )?;

        // Room policy checks
        for client in &client_auth_infos {
            self.room_state_change_role(
                sender_user,
                client.group_membership().user_id(),
                RoleIndex::Regular,
            )?;
        }

        // TODO: Validation:
        // * Check that this commit only contains (inline) add proposals
        // * User ids MUST be unique within the group (check both new
        //   and existing credentials for duplicates).
        // * Client IDs MUST be unique within the group (only need to
        //   check new credentials, as client IDs are scoped to user
        //   names).

        // AddUsers Phase 3: Verify and store the client auth infos.
        if staged_commit.add_proposals().count() != client_auth_infos.len() {
            bail!("Number of add proposals and client credentials don't match.")
        }
        // We assume that leaf credentials are in the same order
        // as client credentials.
        for client_auth_info in client_auth_infos.iter() {
            // Persist the client auth info.
            client_auth_info.stage_add(&mut *connection).await?;
        }

        Ok(client_auth_infos)
    }

    async fn process_update(
        &self,
        connection: &mut SqliteConnection,
        old_sender_credential: VerifiableClientCredential,
        new_sender_credential: VerifiableClientCredential,
        sender_index: LeafNodeIndex,
        new_sender_leaf_key: SignaturePublicKey,
        as_credentials: &HashMap<Hash<AsIntermediateCredentialBody>, AsIntermediateCredential>,
    ) -> Result<()> {
        let client_auth_info = ClientAuthInfo::verify_credential(
            &self.group_id,
            sender_index,
            new_sender_credential,
            new_sender_leaf_key,
            Some(old_sender_credential),
            as_credentials,
        )?;
        // Persist the updated client auth info.
        client_auth_info.stage_update(&mut *connection).await?;

        Ok(())
    }

    async fn process_resync(
        &self,
        processed_message: &ProcessedMessage,
        staged_commit: &StagedCommit,
        connection: &mut sqlx::SqliteConnection,
        sender_index: LeafNodeIndex,
    ) -> Result<()> {
        let removed_index = staged_commit
            .remove_proposals()
            .next()
            .ok_or(anyhow!(
                "Resync operation did not contain a remove proposal"
            ))?
            .remove_proposal()
            .removed();

        let Some(removed_member) = self.mls_group().member_at(removed_index) else {
            bail!("Could not find removed member in group")
        };

        // Check that the leaf credential hasn't changed during the resync.
        if &removed_member.credential != processed_message.credential() {
            bail!("Invalid resync operation: Leaf credential does not match.")
        }

        let mut client_auth_info = ClientAuthInfo::load(connection, &self.group_id, removed_index)
            .await?
            .ok_or_else(|| anyhow!("Could not find client credential of resync sender"))?;

        // Set the client's new leaf index.
        client_auth_info
            .group_membership_mut()
            .set_leaf_index(sender_index);
        client_auth_info.stage_update(connection).await?;
        Ok(())
    }
}

fn update_path_leaf_node_info(
    staged_commit: &StagedCommit,
) -> Result<(VerifiableClientCredential, SignaturePublicKey)> {
    let leaf_node = staged_commit
        .update_path_leaf_node()
        .context("Could not find sender leaf node")?;
    let credential = leaf_node.credential().clone().try_into()?;
    let signature_key = leaf_node.signature_key().clone();
    Ok((credential, signature_key))
}
