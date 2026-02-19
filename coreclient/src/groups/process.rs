// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::HashMap, iter};

use aircommon::{
    credentials::{
        AsIntermediateCredential, AsIntermediateCredentialBody, ClientCredential,
        VerifiableClientCredential,
    },
    crypto::{ear::keys::EncryptedUserProfileKey, hash::Hash, indexed_aead::keys::UserProfileKey},
    identifiers::UserId,
    messages::client_ds::{AadMessage, AadPayload},
    utils::removed_client,
};
use anyhow::{Context, Result, anyhow, bail, ensure};
use mimi_room_policy::RoleIndex;
use openmls::{
    group::{ProcessMessageError, QueuedAddProposal, ValidationError},
    prelude::{
        ProcessedMessage, ProcessedMessageContent, Proposal, ProtocolMessage, Sender,
        SignaturePublicKey, StagedCommit,
    },
};
use sqlx::SqliteTransaction;
use tls_codec::DeserializeBytes as TlsDeserializeBytes;
use tracing::debug;

use crate::{
    clients::api_clients::ApiClients,
    groups::client_auth_info::{MlsGroupExt, VerifiableClientCredentialExt},
    job::pending_chat_operation::PendingChatOperation,
    key_stores::as_credentials::AsCredentials,
};

use super::{Group, openmls_provider::AirOpenMlsProvider};

pub(crate) struct ProcessMessageResult {
    pub(crate) processed_message: ProcessedMessage,
    pub(crate) we_were_removed: bool,
    pub(crate) profile_infos: Vec<(ClientCredential, UserProfileKey)>,
}

impl Group {
    /// Process inbound message
    ///
    /// Returns the processed message, whether the group was deleted, as well as
    /// the sender's client credential.
    pub(crate) async fn process_message(
        &mut self,
        txn: &mut SqliteTransaction<'_>,
        api_clients: &ApiClients,
        message: impl Into<ProtocolMessage>,
    ) -> Result<Option<ProcessMessageResult>> {
        // Phase 1: Process the message.
        let processed_message = {
            let provider = AirOpenMlsProvider::new(&mut *txn);
            let message = message.into();
            let message_epoch = message.epoch();
            match self.mls_group.process_message(&provider, message) {
                Ok(pm) => pm,
                Err(ProcessMessageError::<sqlx::Error>::ValidationError(
                    ValidationError::WrongEpoch,
                )) => {
                    // If the message epoch is in the past, we can just ignore
                    // it. Likely we already re-joined and this is a message we
                    // missed.
                    if self.mls_group.epoch() > message_epoch {
                        bail!("Message epoch is in the past");
                    }
                    // If the message epoch is in the future, we need to re-join
                    // the group.
                    return Ok(None);
                }
                Err(e) => {
                    bail!("Could not process message: {e:?}");
                }
            }
        };

        let group_id = self.group_id().clone();

        // Will be set to true if we were removed (or the group was deleted).
        let mut we_were_removed = false;
        let mut encrypted_profile_infos: Vec<(ClientCredential, EncryptedUserProfileKey)> =
            Vec::new();
        match processed_message.content() {
            // For now, we only care about commits.
            ProcessedMessageContent::ExternalJoinProposalMessage(_) => {
                bail!("Unsupported message type")
            }
            ProcessedMessageContent::ApplicationMessage(_) => {
                debug!("process application message");
                // let sender_client_credential =
                //     if let Sender::Member(index) = processed_message.sender() {
                //         ClientAuthInfo::load(&mut *txn, &group_id, *index)
                //             .await?
                //             .map(|info| info.into_client_credential())
                //             .context("Could not find client credential of message sender")?
                //     } else {
                //         bail!("Invalid sender type.")
                //     };
                return Ok(Some(ProcessMessageResult {
                    processed_message,
                    we_were_removed,
                    profile_infos: Vec::new(),
                }));
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

                // Discard any pending commits we have locally and delete any
                // pending non-leave chat operations we may have for this group.
                // If it's a leave operation, only delete it if it's part of
                // this commit.
                self.discard_pending_commit(txn).await?;
                if let Some(pending_chat_operation) =
                    PendingChatOperation::load_by_group_id(txn, &group_id).await?
                {
                    let commit_contains_our_self_remove =
                        staged_commit.queued_proposals().any(|p| {
                            matches!(p.proposal(), Proposal::SelfRemove)
                                && sender_index == self.mls_group().own_leaf_index()
                        });
                    if !pending_chat_operation.is_leave() || commit_contains_our_self_remove {
                        PendingChatOperation::delete(txn.as_mut(), &group_id).await?;
                    }
                }

                let sender_credential = VerifiableClientCredential::from_basic_credential(
                    processed_message.credential(),
                )?;

                // StagedCommitMessage Phase 1: Process the proposals.

                // Before we process the AAD payload, we first process the
                // proposals by value. Currently only removes are allowed.
                for queued_proposal in staged_commit.queued_proposals() {
                    if matches!(queued_proposal.sender(), Sender::NewMemberCommit) {
                        // This can only happen if the removed member is rejoining
                        // as part of the commit. No need to process the removal.
                        continue;
                    }
                    // Load the removed client's index.
                    let Some(removed_index) = removed_client(queued_proposal) else {
                        // This is not a remove proposal, so we skip it.
                        continue;
                    };

                    let removed_credential = self
                        .mls_group
                        .unverified_credential_at(removed_index)?
                        .context("Removed user credential not found")?;
                    let removed_id = removed_credential.user_id();

                    // Room policy checks
                    self.verify_role_change(
                        sender_credential.user_id(),
                        removed_id,
                        RoleIndex::Outsider,
                    )?;

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
                                &mut *txn,
                                api_clients,
                                verifiable_credentials.iter(),
                            )
                            .await?;
                            let credentials = self
                                .process_adds(
                                    sender_credential.user_id(),
                                    staged_commit,
                                    &mut *txn,
                                    staged_commit.add_proposals(),
                                    &as_credentials,
                                )
                                .await?;
                            // Match up client credentials and new UserProfileKeys
                            let new_profile_infos: Vec<_> = credentials
                                .into_iter()
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
                            &mut *txn,
                            api_clients,
                            iter::once(&new_sender_credential),
                        )
                        .await?;

                        let old_credential = sender_credential;
                        if new_sender_credential != old_credential {
                            let credential = new_sender_credential.verify_and_validate(
                                new_sender_leaf_key,
                                Some(&old_credential),
                                &as_credentials,
                            )?;
                            credential.store(txn.as_mut()).await?;
                        }

                        // Process a resync if this is one
                        if matches!(processed_message.sender(), Sender::NewMemberCommit) {
                            self.process_resync(&processed_message, staged_commit)?;
                        }
                    }
                    AadPayload::JoinConnectionGroup(join_connection_group_payload) => {
                        // JoinConnectionGroup Phase 1: Decrypt and verify the
                        // client credential of the joiner
                        let (sender_credential, sender_leaf_key) =
                            update_path_leaf_node_info(staged_commit)?;

                        let as_credentials = AsCredentials::fetch_for_verification(
                            &mut *txn,
                            api_clients,
                            iter::once(&sender_credential),
                        )
                        .await?;

                        let sender_credential = sender_credential.verify_and_validate(
                            sender_leaf_key,
                            None, // Since the join is an external commit, we don't have an old credential.
                            &as_credentials,
                        )?;

                        // TODO: (More) validation:
                        // * Check that the user id is unique.
                        // * Check that the proposals fit the operation.
                        // * Check that the sender type fits the operation.
                        // * Check that this group is indeed a connection group.

                        // JoinConnectionGroup Phase 2: Persist the client credential
                        sender_credential.store(txn.as_mut()).await?;
                        encrypted_profile_infos.push((
                            sender_credential.into(),
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
                            &mut *txn,
                            api_clients,
                            iter::once(&sender_credential),
                        )
                        .await?;

                        let old_credential =
                            VerifiableClientCredential::from_basic_credential(old_credential)?;
                        let sender_credential = sender_credential.verify_and_validate(
                            sender_leaf_key,
                            Some(&old_credential),
                            &as_credentials,
                        )?;
                        sender_credential.store(txn.as_mut()).await?;
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
        // let sender_client_credential =
        //     if matches!(processed_message.sender(), Sender::NewMemberCommit) {
        //         ClientAuthInfo::load_staged(&mut *txn, &group_id, sender_index).await?
        //     } else {
        //         ClientAuthInfo::load(&mut *txn, &group_id, sender_index).await?
        //     }
        //     .context("Could not find client credential of message sender")?
        //     .client_credential()
        //     .clone()
        //     .into();

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

        Ok(Some(ProcessMessageResult {
            processed_message,
            we_were_removed,
            profile_infos,
        }))
    }

    async fn process_adds<'a>(
        &mut self,
        sender_user: &UserId,
        staged_commit: &StagedCommit,
        txn: &mut SqliteTransaction<'_>,
        added_clients: impl Iterator<Item = QueuedAddProposal<'a>>,
        as_credentials: &HashMap<Hash<AsIntermediateCredentialBody>, AsIntermediateCredential>,
    ) -> Result<Vec<ClientCredential>> {
        let mut credentials = Vec::new();

        for proposal in added_clients {
            let leaf_node = proposal.add_proposal().key_package().leaf_node();

            // Verify the credential
            let credential =
                VerifiableClientCredential::from_basic_credential(leaf_node.credential())?;
            let credential =
                credential.verify_and_validate(leaf_node.signature_key(), None, as_credentials)?;

            self.verify_role_change(sender_user, credential.identity(), RoleIndex::Regular)?;

            credential.store(txn.as_mut()).await?;
            credentials.push(credential.into());
        }

        // TODO: Validation:
        // * Check that this commit only contains (inline) add proposals
        // * User ids MUST be unique within the group (check both new
        //   and existing credentials for duplicates).
        // * Client IDs MUST be unique within the group (only need to
        //   check new credentials, as client IDs are scoped to user
        //   names).

        // AddUsers Phase 3: Verify and store the client auth infos.
        if staged_commit.add_proposals().count() != credentials.len() {
            bail!("Number of add proposals and client credentials don't match.")
        }

        Ok(credentials)
    }

    fn process_resync(
        &self,
        processed_message: &ProcessedMessage,
        staged_commit: &StagedCommit,
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

        // No need to verify or update the credential, since the sender is already member of the
        // group and the credential did not change.

        Ok(())
    }
}

fn update_path_leaf_node_info(
    staged_commit: &StagedCommit,
) -> Result<(VerifiableClientCredential, &SignaturePublicKey)> {
    let leaf_node = staged_commit
        .update_path_leaf_node()
        .context("Could not find sender leaf node")?;
    let credential = VerifiableClientCredential::from_basic_credential(leaf_node.credential())?;
    let signature_key = leaf_node.signature_key();
    Ok((credential, signature_key))
}
