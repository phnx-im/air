// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::HashMap, iter};

use aircommon::{
    credentials::{
        AsIntermediateCredential, AsIntermediateCredentialBody, ClientCredential,
        VerifiableClientCredential,
    },
    crypto::{aead::keys::EncryptedUserProfileKey, hash::Hash, indexed_aead::keys::UserProfileKey},
    identifiers::UserId,
    messages::client_ds::{
        AadMessage, AadPayload, GroupOperationParamsAad, JoinConnectionGroupParamsAad,
    },
    utils::removed_client,
};
use anyhow::{Context, Result, anyhow, bail, ensure};
use apqmls::{
    ApqMlsGroupMut,
    messages::ApqProtocolMessage,
    processing::{ApqProcessMessageError, ApqProcessedMessage},
};
use mimi_room_policy::RoleIndex;
use openmls::{
    group::{ProcessMessageError, ValidationError},
    prelude::{
        Credential, GroupId, LeafNodeIndex, ProcessedMessage, ProcessedMessageContent, Proposal,
        ProtocolMessage, Sender, SignaturePublicKey, StagedCommit,
    },
};
use openmls_traits::OpenMlsProvider;
use tls_codec::DeserializeBytes as TlsDeserializeBytes;
use tracing::{debug, instrument};

use crate::{
    clients::api_clients::ApiClients, db::access::WriteDbTransaction,
    groups::client_auth_info::VerifiableClientCredentialExt,
    job::pending_chat_operation::PendingChatOperation, key_stores::as_credentials::AsCredentials,
};

use super::{Group, openmls_provider::AirOpenMlsProvider};

pub(crate) struct ProcessMessageResult {
    pub(crate) processed_message: ProcessedMessage,
    pub(crate) we_were_removed: bool,
    pub(crate) profile_infos: Vec<(ClientCredential, UserProfileKey)>,
}

struct PostProcessState {
    sender_index: LeafNodeIndex,
    we_were_removed: bool,
    encrypted_profile_infos: Vec<(ClientCredential, EncryptedUserProfileKey)>,
}

struct PostProcessAadResult {
    we_were_removed: bool,
    encrypted_profile_infos: Vec<(ClientCredential, EncryptedUserProfileKey)>,
}

impl Group {
    /// Process inbound message
    ///
    /// Returns the processed message, whether the group was deleted, as well as
    /// the sender's client credential.
    #[instrument(skip_all, fields(group_id = ?self.group_id()))]
    pub(crate) async fn process_message(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
        api_clients: &ApiClients,
        message: impl Into<ProtocolMessage>,
    ) -> Result<Option<ProcessMessageResult>> {
        // Phase 1: Process the message.
        let processed_message = {
            let provider = AirOpenMlsProvider::new(txn.as_mut());
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

        self.post_process_message(txn, api_clients, processed_message, None)
            .await
            .map(Some)
    }

    async fn post_process_message(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
        api_clients: &ApiClients,
        processed_message: ProcessedMessage,
        pq_processed_message: Option<&ProcessedMessage>,
    ) -> Result<ProcessMessageResult> {
        let post_process_state = match processed_message.content() {
            // For now, we only care about commits.
            ProcessedMessageContent::ExternalJoinProposalMessage(_) => {
                bail!("Unsupported message type")
            }
            ProcessedMessageContent::ApplicationMessage(_) => {
                debug!("process application message");
                return Ok(ProcessMessageResult {
                    processed_message,
                    we_were_removed: false,
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

                PostProcessState {
                    sender_index: *sender_index,
                    we_were_removed: false,
                    encrypted_profile_infos: Vec::new(),
                }
            }
            ProcessedMessageContent::StagedCommitMessage(_) => {
                self.post_process_staged_commit(
                    txn,
                    api_clients,
                    &processed_message,
                    pq_processed_message,
                )
                .await?
            }
        };

        // Check that the signature keys of the sender match
        self.verify_pq_signature_key_at(post_process_state.sender_index)?;

        // Decrypt any user profile keys
        let profile_infos = post_process_state
            .encrypted_profile_infos
            .into_iter()
            .map(|(client_credential, encrypted_user_profile_key)| {
                let user_profile_key = UserProfileKey::decrypt(
                    self.identity_link_wrapper_key(),
                    &encrypted_user_profile_key,
                    client_credential.user_id(),
                )?;
                Ok((client_credential, user_profile_key))
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(ProcessMessageResult {
            processed_message,
            we_were_removed: post_process_state.we_were_removed,
            profile_infos,
        })
    }

    async fn post_process_staged_commit(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
        api_clients: &ApiClients,
        processed_message: &ProcessedMessage,
        pq_processed_message: Option<&ProcessedMessage>,
    ) -> Result<PostProcessState> {
        let group_id = self.group_id().clone();

        let staged_commit = expect_staged_commit(processed_message)?;
        let pq_staged_commit = pq_processed_message.map(expect_staged_commit).transpose()?;

        let sender_index = match processed_message.sender() {
            Sender::Member(index) => index.to_owned(),
            Sender::NewMemberCommit => self.mls_group.ext_commit_sender_index(staged_commit)?,
            Sender::External(_) | Sender::NewMemberProposal => {
                bail!("Invalid sender type.")
            }
        };

        self.discard_pending_commit_and_operations(txn, &group_id, staged_commit)
            .await?;

        let sender_credential =
            VerifiableClientCredential::from_basic_credential(processed_message.credential())?;

        // StagedCommitMessage Phase 1: Process the proposals.
        let removed_by_proposal =
            self.process_remove_proposals(staged_commit, &sender_credential)?;

        // Phase 2: Process the AAD payload.
        let aad_result = self
            .process_aad_payload(
                txn,
                api_clients,
                processed_message,
                pq_staged_commit,
                sender_credential,
            )
            .await?;

        // We were removed (or the group was deleted) if either the proposals or
        // the AAD payload indicated so.
        let we_were_removed = removed_by_proposal || aad_result.we_were_removed;

        Ok(PostProcessState {
            sender_index,
            we_were_removed,
            encrypted_profile_infos: aad_result.encrypted_profile_infos,
        })
    }

    /// Process the AAD payload of a staged commit, dispatching to the handler
    /// for the concrete operation.
    async fn process_aad_payload(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
        api_clients: &ApiClients,
        processed_message: &ProcessedMessage,
        pq_staged_commit: Option<&StagedCommit>,
        sender_credential: VerifiableClientCredential,
    ) -> Result<PostProcessAadResult> {
        // Let's figure out which operation this is meant to be.
        let aad_payload =
            AadMessage::tls_deserialize_exact_bytes(processed_message.aad())?.into_payload();
        let result = match aad_payload {
            AadPayload::GroupOperation(group_operation_payload) => {
                let encrypted_profile_infos = self
                    .process_group_operation_aad(
                        txn,
                        api_clients,
                        processed_message,
                        pq_staged_commit,
                        sender_credential,
                        group_operation_payload,
                    )
                    .await?;
                PostProcessAadResult {
                    we_were_removed: false,
                    encrypted_profile_infos,
                }
            }
            AadPayload::JoinConnectionGroup(join_connection_group_payload) => {
                let profile_info = self
                    .process_join_connection_group_aad(
                        txn,
                        api_clients,
                        processed_message,
                        pq_staged_commit,
                        join_connection_group_payload,
                    )
                    .await?;
                PostProcessAadResult {
                    we_were_removed: false,
                    encrypted_profile_infos: vec![profile_info],
                }
            }
            AadPayload::Resync => {
                self.process_resync_aad(txn, api_clients, processed_message, pq_staged_commit)
                    .await?;
                PostProcessAadResult {
                    we_were_removed: false,
                    encrypted_profile_infos: Vec::new(),
                }
            }
            // The group was deleted; there is nothing else to do at this point.
            AadPayload::DeleteGroup => PostProcessAadResult {
                we_were_removed: true,
                encrypted_profile_infos: Vec::new(),
            },
        };

        Ok(result)
    }

    /// Process a group operation AAD payload: verify and persist any added
    /// credentials and a potentially updated sender leaf credential. Returns
    /// the encrypted user profile keys of any added clients.
    async fn process_group_operation_aad(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
        api_clients: &ApiClients,
        processed_message: &ProcessedMessage,
        pq_staged_commit: Option<&StagedCommit>,
        sender_credential: VerifiableClientCredential,
        group_operation_payload: GroupOperationParamsAad,
    ) -> Result<Vec<(ClientCredential, EncryptedUserProfileKey)>> {
        let staged_commit = expect_staged_commit(processed_message)?;

        let mut encrypted_profile_infos: Vec<(ClientCredential, EncryptedUserProfileKey)> =
            Vec::new();

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
            // Verify that T/PQ added user signature keys match
            verify_pq_added_signature_keys(staged_commit, pq_staged_commit)?;

            // Collect the verifiable credentials
            let mut verifiable_credentials = Vec::with_capacity(number_of_adds);
            for ap in staged_commit.add_proposals() {
                let credential = ap.add_proposal().key_package().leaf_node().credential();
                let credential = VerifiableClientCredential::from_basic_credential(credential)?;
                verifiable_credentials.push(credential);
            }

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
                    &as_credentials,
                )
                .await?;
            // Match up client credentials and new UserProfileKeys
            let new_profile_infos: Vec<_> = credentials
                .into_iter()
                .zip(group_operation_payload.new_encrypted_user_profile_keys)
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
            credential.store(txn).await?;
        }

        // Process a resync if this is one
        if matches!(processed_message.sender(), Sender::NewMemberCommit) {
            self.process_resync(processed_message.credential(), staged_commit)?;
        }

        Ok(encrypted_profile_infos)
    }

    /// Process a join-connection-group AAD payload: verify and persist the
    /// joiner's client credential. Returns the joiner's encrypted user profile
    /// key.
    async fn process_join_connection_group_aad(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
        api_clients: &ApiClients,
        processed_message: &ProcessedMessage,
        pq_staged_commit: Option<&StagedCommit>,
        join_connection_group_payload: JoinConnectionGroupParamsAad,
    ) -> Result<(ClientCredential, EncryptedUserProfileKey)> {
        let staged_commit = expect_staged_commit(processed_message)?;

        validate_join_connection_group_commit(
            processed_message.sender(),
            staged_commit.add_proposals().next().is_some()
                || staged_commit.update_proposals().next().is_some()
                || staged_commit.remove_proposals().next().is_some(),
            self.mls_group.members().count(),
        )?;

        // JoinConnectionGroup Phase 1: Decrypt and verify the
        // client credential of the joiner
        let (sender_credential, sender_leaf_key) = update_path_leaf_node_info(staged_commit)?;

        // Verify that T/PQ signature keys match
        verify_pq_update_path_signature_key(sender_leaf_key, pq_staged_commit)?;

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

        // JoinConnectionGroup Phase 2: Persist the client credential
        sender_credential.store(txn).await?;
        Ok((
            sender_credential.into(),
            join_connection_group_payload.encrypted_user_profile_key,
        ))
    }

    /// Process a resync AAD payload: verify and persist the resyncing member's
    /// (unchanged) client credential.
    async fn process_resync_aad(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
        api_clients: &ApiClients,
        processed_message: &ProcessedMessage,
        pq_staged_commit: Option<&StagedCommit>,
    ) -> Result<()> {
        let staged_commit = expect_staged_commit(processed_message)?;

        // Check if it's an external commit. This implies that
        // there is only one remove proposal.
        ensure!(
            matches!(processed_message.sender(), Sender::NewMemberCommit),
            "Resync operation must be an external commit"
        );

        let (sender_credential, sender_leaf_key) = update_path_leaf_node_info(staged_commit)?;

        // Verify that T/PQ signature keys match
        verify_pq_update_path_signature_key(sender_leaf_key, pq_staged_commit)?;

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

        let old_credential = VerifiableClientCredential::from_basic_credential(old_credential)?;
        let sender_credential = sender_credential.verify_and_validate(
            sender_leaf_key,
            Some(&old_credential),
            &as_credentials,
        )?;
        sender_credential.store(txn).await?;
        Ok(())
    }

    /// Discard any pending commits we have locally and delete any pending
    /// non-leave chat operations we may have for this group. If it's a leave
    /// operation, only delete it if it's part of this commit.
    async fn discard_pending_commit_and_operations(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
        group_id: &GroupId,
        staged_commit: &StagedCommit,
    ) -> Result<()> {
        self.discard_pending_commit(&mut *txn).await?;
        if let Some(pending_chat_operation) =
            PendingChatOperation::load_by_group_id(&mut *txn, group_id).await?
        {
            let commit_contains_our_self_remove = staged_commit.queued_proposals().any(|p| {
                let Sender::Member(proposal_sender_index) = p.sender() else {
                    return false;
                };
                matches!(p.proposal(), Proposal::SelfRemove)
                    && proposal_sender_index == &self.mls_group().own_leaf_index()
            });
            if !pending_chat_operation.is_leave() || commit_contains_our_self_remove {
                PendingChatOperation::delete(&mut *txn, group_id).await?;
            }
        }
        Ok(())
    }

    /// Process the remove proposals in a staged commit by value. Currently only
    /// removes are allowed. Returns `true` if we were removed.
    fn process_remove_proposals(
        &self,
        staged_commit: &StagedCommit,
        sender_credential: &VerifiableClientCredential,
    ) -> Result<bool> {
        let mut we_were_removed = false;
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

            // Check that the signature keys of the removed user match
            self.verify_pq_signature_key_at(removed_index)?;

            let removed_credential = self
                .unverified_credential_at(removed_index)?
                .context("Removed user credential not found")?;
            let removed_id = removed_credential.user_id();

            // Room policy checks
            self.verify_role_change(sender_credential.user_id(), removed_id, RoleIndex::Outsider)?;

            if removed_index == self.mls_group().own_leaf_index() {
                we_were_removed = true;
            }
        }
        Ok(we_were_removed)
    }

    /// Verify that the T and PQ leaf nodes at `index` have matching signature
    /// keys. A no-op if there is no PQ group.
    fn verify_pq_signature_key_at(&self, index: LeafNodeIndex) -> Result<()> {
        if let Some(pq_group) = self.pq.as_ref() {
            let pq_leaf = pq_group
                .mls_group
                .public_group()
                .leaf(index)
                .context("PQ sender leaf not found")?;
            let t_leaf = self
                .mls_group
                .public_group()
                .leaf(index)
                .context("T sender leaf not found")?;
            ensure!(
                pq_leaf.signature_key() == t_leaf.signature_key(),
                "T and PQ sender signature keys do not match"
            );
        }
        Ok(())
    }

    async fn process_adds(
        &mut self,
        sender_user: &UserId,
        staged_commit: &StagedCommit,
        txn: &mut WriteDbTransaction<'_>,
        as_credentials: &HashMap<Hash<AsIntermediateCredentialBody>, AsIntermediateCredential>,
    ) -> Result<Vec<ClientCredential>> {
        let mut credentials = Vec::new();

        for proposal in staged_commit.add_proposals() {
            let leaf_node = proposal.add_proposal().key_package().leaf_node();

            // Verify the credential
            let credential =
                VerifiableClientCredential::from_basic_credential(leaf_node.credential())?;
            let credential =
                credential.verify_and_validate(leaf_node.signature_key(), None, as_credentials)?;

            self.verify_role_change(sender_user, credential.user_id(), RoleIndex::Regular)?;

            credential.store(&mut *txn).await?;
            credentials.push(credential.into());
        }

        // TODO: Validation:
        // * Check that this commit only contains (inline) add proposals
        // * User ids MUST be unique within the group (check both new
        //   and existing credentials for duplicates).
        // * Client IDs MUST be unique within the group (only need to
        //   check new credentials, as client IDs are scoped to user
        //   names).

        Ok(credentials)
    }

    fn process_resync(&self, credential: &Credential, staged_commit: &StagedCommit) -> Result<()> {
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
        if &removed_member.credential != credential {
            bail!("Invalid resync operation: Leaf credential does not match.")
        }

        // No need to verify or update the credential, since the sender is already member of the
        // group and the credential did not change.

        Ok(())
    }

    #[instrument(skip_all, fields(group_id = ?self.group_id()))]
    pub(crate) async fn process_apq_message(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
        api_clients: &ApiClients,
        message: impl Into<ApqProtocolMessage>,
    ) -> Result<Option<ProcessMessageResult>> {
        let message: ApqProtocolMessage = message.into();
        let message_t_epoch = message.t_epoch();
        let current_t_epoch = self.mls_group.epoch();
        let (t_mls_group, pq_mls_group) = self.apq_mls_groups_mut()?;

        let ApqProcessedMessage {
            t_message,
            pq_message,
        } = match ApqMlsGroupMut::from_groups(t_mls_group, pq_mls_group).process_message(
            &AirOpenMlsProvider::new(txn.as_mut()),
            message,
            |_, _| true, // PQ-credential is always empty
        ) {
            Ok(pm) => pm,
            Err(ApqProcessMessageError::Processing(ProcessMessageError::ValidationError(
                ValidationError::WrongEpoch,
            ))) => {
                // A past-epoch message is one we already moved past, so we
                // ignore it.
                if current_t_epoch > message_t_epoch {
                    bail!(
                        "Message epoch is in the past: message t-epoch {} < current t-epoch {}",
                        message_t_epoch,
                        current_t_epoch
                    );
                }
                // A future-epoch message means we are behind and the caller
                // must trigger a resync.
                return Ok(None);
            }
            Err(e) => {
                return Err(e).context("Failed to process APQ message");
            }
        };

        // The PQ message carries no Air-level semantics, so the only post-processing we need to do
        // is on the t-message.
        let res = Self::post_process_message(self, txn, api_clients, t_message, Some(&pq_message))
            .await?;

        // Merge the PQ staged commit or proposal (self-remove)
        match pq_message.into_content() {
            ProcessedMessageContent::StagedCommitMessage(pq_staged_commit) => {
                let provider = AirOpenMlsProvider::new(txn.as_mut());
                self.pq_mut()
                    .context("logic error: no PQ group")?
                    .mls_group
                    .merge_staged_commit(&provider, *pq_staged_commit)?;
            }
            ProcessedMessageContent::ProposalMessage(pq_queue_proposal) => {
                let provider = AirOpenMlsProvider::new(txn.as_mut());
                self.pq_mut()
                    .context("logic error: no PQ group")?
                    .mls_group
                    .store_pending_proposal(provider.storage(), *pq_queue_proposal)?;
            }
            _ => (),
        }

        Ok(Some(res))
    }
}

/// Extract the staged commit from a processed message. Errors if the message
/// is not a staged commit message.
fn expect_staged_commit(processed_message: &ProcessedMessage) -> Result<&StagedCommit> {
    let ProcessedMessageContent::StagedCommitMessage(staged_commit) = processed_message.content()
    else {
        bail!("Expected a StagedCommitMessage");
    };
    Ok(staged_commit)
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

/// Verify that the T update-path leaf signature key matches the PQ update-path
/// leaf signature key. A no-op if there is no PQ staged commit.
fn verify_pq_update_path_signature_key(
    t_leaf_key: &SignaturePublicKey,
    pq_staged_commit: Option<&StagedCommit>,
) -> Result<()> {
    if let Some(pq_staged_commit) = pq_staged_commit {
        let pq_leaf_node = pq_staged_commit
            .update_path_leaf_node()
            .context("Could not find sender leaf node")?;
        ensure!(
            t_leaf_key == pq_leaf_node.signature_key(),
            "T and PQ sender signature keys do not match"
        );
    }
    Ok(())
}

/// Verify that the T and PQ add proposals have matching signature keys, and
/// that there are at least as many PQ add proposals as T add proposals. A
/// no-op if there is no PQ staged commit.
fn verify_pq_added_signature_keys(
    staged_commit: &StagedCommit,
    pq_staged_commit: Option<&StagedCommit>,
) -> Result<()> {
    let Some(pq_staged_commit) = pq_staged_commit else {
        return Ok(());
    };
    let mut pq_add_proposals = pq_staged_commit.add_proposals();
    for ap in staged_commit.add_proposals() {
        let pq_add_proposal = pq_add_proposals
            .next()
            .context("Less PQ add proposals than T")?;
        let pq_signature_key = pq_add_proposal
            .add_proposal()
            .key_package()
            .leaf_node()
            .signature_key();
        let t_signature_key = ap.add_proposal().key_package().leaf_node().signature_key();
        ensure!(
            pq_signature_key == t_signature_key,
            "T and PQ added user signature keys do not match"
        );
    }
    Ok(())
}

fn validate_join_connection_group_commit(
    sender: &Sender,
    contains_membership_proposal: bool,
    member_count: usize,
) -> Result<()> {
    ensure!(
        matches!(sender, Sender::NewMemberCommit),
        "JoinConnectionGroup operation must be an external commit"
    );
    ensure!(
        !contains_membership_proposal,
        "JoinConnectionGroup operation must not contain add, update, or remove proposals"
    );
    ensure!(
        member_count == 1,
        "JoinConnectionGroup operation must target a connection group"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use openmls::prelude::LeafNodeIndex;

    use super::*;

    #[test]
    fn join_connection_group_validation_enforces_operation_shape() {
        assert!(validate_join_connection_group_commit(&Sender::NewMemberCommit, false, 1).is_ok());

        let cases = [
            (
                Sender::Member(LeafNodeIndex::new(0)),
                false,
                1,
                "JoinConnectionGroup operation must be an external commit",
            ),
            (
                Sender::NewMemberCommit,
                true,
                1,
                "JoinConnectionGroup operation must not contain add, update, or remove proposals",
            ),
            (
                Sender::NewMemberCommit,
                false,
                2,
                "JoinConnectionGroup operation must target a connection group",
            ),
        ];

        for (sender, contains_membership_proposal, member_count, expected_error) in cases {
            let error = validate_join_connection_group_commit(
                &sender,
                contains_membership_proposal,
                member_count,
            )
            .expect_err("invalid JoinConnectionGroup operation should fail");
            assert!(
                error.to_string().contains(expected_error),
                "unexpected error: {error:#}"
            );
        }
    }
}
