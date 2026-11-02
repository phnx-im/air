// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

mod aad;
mod self_group_payload;
mod validation;

use aircommon::{
    credentials::{LeafCredential, UserCredential},
    crypto::{aead::keys::EncryptedUserProfileKey, indexed_aead::keys::UserProfileKey},
};
use airprotos::client::app_data::GroupAppData;
use anyhow::{Context, Result, bail, ensure};
use apqmls::{
    ApqMlsGroupMut,
    messages::ApqProtocolMessage,
    processing::{ApqProcessMessageError, ApqProcessedMessage, resolve_partial_commit},
};
use openmls::{
    components::vc_derivation_info::VirtualClientsError,
    framing::errors::{MessageDecryptionError, SecretTreeError},
    group::{ProcessMessageError, StageCommitError, ValidationError},
    prelude::{
        GroupId, LeafNodeIndex, MlsGroup, ProcessedMessage, ProcessedMessageContent, Proposal,
        ProtocolMessage, Sender, StagedCommit,
    },
};
use openmls_traits::OpenMlsProvider;
use tracing::{debug, error, instrument, warn};

use crate::{
    chats::connection_requests::ConnectionRequestEffects,
    clients::api_clients::ApiClients,
    db::access::WriteDbTransaction,
    groups::{
        Group,
        openmls_provider::AirOpenMlsProvider,
        process::{
            self_group_payload::apply_self_group_payload,
            validation::verify_pq_update_path_signature_key,
        },
    },
    job::pending_chat_operation::PendingChatOperation,
    outbound_service::resync::{Resync, ResyncReason},
};

pub(crate) enum ProcessMessageResult {
    Processed(Box<ProcessMessageProcessed>),
    /// This message was skipped because we have processed it (or its side-effect) already.
    Ignored,
    /// We got a message that we can't process from our current group state, e.g. because it's too
    /// far in the future or because the local PQ group state is missing. Only a resync can recover
    /// the group.
    ResyncRequired(ResyncReason),
}

pub(crate) struct ProcessMessageProcessed {
    pub(crate) processed_message: ProcessedMessage,
    pub(crate) we_were_removed: bool,
    pub(crate) profile_infos: Vec<(UserCredential, UserProfileKey)>,
    /// What a sibling's self-group commit did to connection requests.
    pub(crate) connection_request_effects: ConnectionRequestEffects,
}

struct PostProcessState {
    sender_index: LeafNodeIndex,
    we_were_removed: bool,
    encrypted_profile_infos: Vec<(UserCredential, EncryptedUserProfileKey)>,
    connection_request_effects: ConnectionRequestEffects,
}

struct PostProcessAadResult {
    we_were_removed: bool,
    encrypted_profile_infos: Vec<(UserCredential, EncryptedUserProfileKey)>,
}

impl Group {
    /// Process inbound message
    ///
    /// Returns the processed message, whether the group was deleted, as well as
    /// the sender's user credential
    #[instrument(skip_all, fields(group_id = ?self.group_id()))]
    pub(crate) async fn process_message(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
        api_clients: &ApiClients,
        message: impl Into<ProtocolMessage>,
    ) -> Result<ProcessMessageResult> {
        // Phase 1: Process the message.
        let processed_message = {
            let provider = AirOpenMlsProvider::new(txn.as_mut());
            let message = message.into();
            let message_epoch = message.epoch();
            match self.mls_group.process_message(&provider, message) {
                Ok(processed_message) => {
                    // Processes app data updates in the message, if any.
                    resolve_partial_commit(&self.mls_group, &provider, processed_message)?
                }
                Err(ProcessMessageError::<sqlx::Error>::ValidationError(
                    ValidationError::WrongEpoch,
                )) => {
                    // If the message epoch is in the past, we can just
                    // ignore it: our pending commit was already merged via
                    // the `DsCommitResponse`, so a replayed or stale delivery
                    // is one epoch behind. Skip quietly rather than logging
                    // an error.
                    if self.mls_group.epoch() > message_epoch {
                        debug!(
                            ?message_epoch,
                            current_epoch = ?self.mls_group.epoch(),
                            "Ignoring past-epoch message"
                        );
                        return Ok(ProcessMessageResult::Ignored);
                    }
                    // If the message epoch is in the future, we need to re-join
                    // the group.
                    return Ok(ProcessMessageResult::ResyncRequired(
                        ResyncReason::FutureEpoch,
                    ));
                }
                Err(ProcessMessageError::InvalidCommit(StageCommitError::VirtualClientsError(
                    error @ VirtualClientsError::MissingDerivationEpochState
                    | error @ VirtualClientsError::MissingOperationTree
                    | error @ VirtualClientsError::OperationGenerationConsumed
                    | error @ VirtualClientsError::OperationGenerationTooDistant,
                ))) => {
                    // The commit was not built against a virtual client derivation epoch we hold.
                    // Only a resync can get us back onto the same shared leaf.
                    error!(
                        %error,
                        "Cannot follow a virtual-client commit onto our shared leaf"
                    );
                    return Ok(ProcessMessageResult::ResyncRequired(
                        ResyncReason::VirtualClientDesync,
                    ));
                }
                Err(ProcessMessageError::ValidationError(ValidationError::UnableToDecrypt(
                    MessageDecryptionError::SecretTreeError(SecretTreeError::SecretReuseError),
                ))) if self.own_leaf_is_virtual_client() => {
                    debug!("Ignoring our own echoed application message");
                    return Ok(ProcessMessageResult::Ignored);
                }
                Err(e) => {
                    bail!("Could not process message: {e:?}");
                }
            }
        };

        self.post_process_message(txn, api_clients, processed_message, None)
            .await
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
                return Ok(ProcessMessageResult::Processed(Box::new(
                    ProcessMessageProcessed {
                        processed_message,
                        we_were_removed: false,
                        profile_infos: Vec::new(),
                        connection_request_effects: ConnectionRequestEffects::default(),
                    },
                )));
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
                    connection_request_effects: ConnectionRequestEffects::default(),
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
            ProcessedMessageContent::OwnPendingCommit => {
                // Our own commit was echoed back to us: openmls matched it
                // against our pending commit.
                PostProcessState {
                    sender_index: self.mls_group.own_leaf_index(),
                    we_were_removed: false,
                    encrypted_profile_infos: Vec::new(),
                    connection_request_effects: ConnectionRequestEffects::default(),
                }
            }
            ProcessedMessageContent::OwnPrivateMessage => {
                return Ok(ProcessMessageResult::Ignored);
            }
            ProcessedMessageContent::UnresolvedAppDataCommit(_) => {
                error!(
                    "Unexpected UnresolvedAppDataCommit in commit message, \
                    should have been resolved before"
                );
                return Ok(ProcessMessageResult::Ignored);
            }
        };

        // Check that the signature keys of the sender match
        //
        // Skip this check for external-commit senders (resync, join connection group). Their leaf
        // is not visible in the live tree until the commit is merged; `post_process_staged_commit`
        // checks the update-path leaf keys instead.
        if !matches!(processed_message.sender(), Sender::NewMemberCommit) {
            self.verify_pq_signature_key_at(post_process_state.sender_index)?;
        }

        // Decrypt any user profile keys
        let profile_infos = post_process_state
            .encrypted_profile_infos
            .into_iter()
            .map(|(user_credential, encrypted_user_profile_key)| {
                let user_profile_key = UserProfileKey::decrypt(
                    self.identity_link_wrapper_key(),
                    &encrypted_user_profile_key,
                    user_credential.user_id(),
                )?;
                Ok((user_credential, user_profile_key))
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(ProcessMessageResult::Processed(Box::new(
            ProcessMessageProcessed {
                processed_message,
                we_were_removed: post_process_state.we_were_removed,
                profile_infos,
                connection_request_effects: post_process_state.connection_request_effects,
            },
        )))
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

        // The self-group flag is set at group creation and must never change. Reject any commit
        // whose proposals (AppDataUpdate or GroupContextExtensions) would toggle it.
        ensure_self_group_flag_unchanged(&self.mls_group, staged_commit)?;
        if let (Some(pq_group), Some(pq_staged_commit)) = (self.pq.as_ref(), pq_staged_commit) {
            ensure_self_group_flag_unchanged(&pq_group.mls_group, pq_staged_commit)?;
        }

        // For an external commit (resync, join connection group), the sender's new leaf lives in
        // the commit's update path and is not visible in the live tree until the commit is merged.
        // Bind the T and PQ legs here, regardless of which AAD payload the commit carries.
        if let Sender::NewMemberCommit = processed_message.sender() {
            let t_leaf_key = staged_commit
                .update_path_leaf_node()
                .context("Could not find sender leaf node")?
                .signature_key();
            verify_pq_update_path_signature_key(t_leaf_key, pq_staged_commit)?;
        }

        let sender_index = match processed_message.sender() {
            Sender::Member(index) => index.to_owned(),
            Sender::NewMemberCommit => self.mls_group.ext_commit_sender_index(staged_commit)?,
            Sender::External(_) | Sender::NewMemberProposal => {
                bail!("Invalid sender type.")
            }
        };

        // Self-group message phase. This must run before the pending-op discard
        // below so the pending setting changes and token seed proposals are
        // reconciled against what this commit carried before the outbound
        // service can re-issue them.
        let mut connection_request_effects = ConnectionRequestEffects::default();
        if self.is_self_group() {
            let payload = self.extract_self_group_messages(txn, staged_commit).await;
            if !payload.is_empty() {
                // Own echo: the DS fanned our own commit back while our pending
                // commit was already gone.
                let own_echo = sender_index == self.mls_group().own_leaf_index();
                connection_request_effects =
                    apply_self_group_payload(txn, &payload, own_echo).await?;
            }
        }

        self.discard_pending_commit_and_operations(txn, &group_id, staged_commit)
            .await?;

        // Process a sibling's key package upload announcement (self-group
        // only), if any. Errors are logged and swallowed: the commit itself
        // is valid and must be merged, otherwise the group falls permanently
        // behind the DS.
        if let Err(error) = self
            .process_vc_key_package_upload_aad(txn, processed_message, sender_index)
            .await
        {
            error!(%error, "Failed to process sibling key package upload");
        }

        let sender_credential = LeafCredential::from_credential(processed_message.credential())?;

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
            connection_request_effects,
        })
    }

    /// Discard our local pending commit and reconcile any pending chat
    /// operation for this group against the incoming commit.
    ///
    /// A pending non-leave operation is deleted. For a settings update this
    /// only drops the send attempt whose staged commit just became stale: the
    /// pending [`SettingChanges`] survive and the outbound service re-issues a
    /// fresh commit from them. A leave operation is deleted only if this
    /// commit carries our own self-remove.
    pub(crate) async fn discard_pending_commit_and_operations(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
        group_id: &GroupId,
        staged_commit: &StagedCommit,
    ) -> Result<()> {
        self.discard_pending_commit(&mut *txn).await?;
        // Processing a commit in this group proves the group is in sync, so
        // this also clears a `failed` entry
        Resync::remove(&mut *txn, group_id).await?;
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
                pending_chat_operation.discard(txn).await?;
            }
        }
        Ok(())
    }

    #[instrument(skip_all, fields(group_id = ?self.group_id()))]
    pub(crate) async fn process_apq_message(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
        api_clients: &ApiClients,
        message: impl Into<ApqProtocolMessage>,
    ) -> Result<ProcessMessageResult> {
        if self.pq.is_none() {
            // The local PQ group state is missing, e.g. because a legacy
            // T-only resync dropped it. We can't process any APQ message in
            // this state, but a resync restores both legs, so report the
            // message as too distant instead of failing.
            warn!("No local PQ group state; a resync is required");
            return Ok(ProcessMessageResult::ResyncRequired(
                ResyncReason::MissingPqGroupState,
            ));
        }

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
                // ignore it: our pending commit was already merged via the
                // `DsCommitResponse`, so a replayed or stale delivery is one
                // epoch behind. Skip quietly rather than logging an error.
                if current_t_epoch > message_t_epoch {
                    debug!(
                        %message_t_epoch,
                        %current_t_epoch,
                        "Ignoring past-epoch APQ message"
                    );
                    return Ok(ProcessMessageResult::Ignored);
                }
                // A future-epoch message means we are behind and the caller
                // must trigger a resync.
                return Ok(ProcessMessageResult::ResyncRequired(
                    ResyncReason::FutureEpoch,
                ));
            }
            Err(ApqProcessMessageError::Processing(ProcessMessageError::InvalidCommit(
                StageCommitError::VirtualClientsError(
                    error @ VirtualClientsError::MissingDerivationEpochState
                    | error @ VirtualClientsError::MissingOperationTree
                    | error @ VirtualClientsError::OperationGenerationConsumed
                    | error @ VirtualClientsError::OperationGenerationTooDistant,
                ),
            ))) => {
                // See the T-only variant: we cannot derive the leaf a sibling
                // emulator client replaced, so only a resync recovers.
                error!(
                    %error,
                    "Cannot follow a virtual-client APQ commit onto our shared leaf"
                );
                return Ok(ProcessMessageResult::ResyncRequired(
                    ResyncReason::VirtualClientDesync,
                ));
            }
            Err(ApqProcessMessageError::Processing(ProcessMessageError::ValidationError(
                ValidationError::UnableToDecrypt(MessageDecryptionError::SecretTreeError(
                    SecretTreeError::SecretReuseError,
                )),
            ))) if self.own_leaf_is_virtual_client() => {
                debug!("Ignoring our own echoed APQ application message");
                return Ok(ProcessMessageResult::Ignored);
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

        Ok(res)
    }
}

/// Verify that merging `staged_commit` keeps the self-group flag in the group context unchanged.
/// The flag is fixed at group creation.
fn ensure_self_group_flag_unchanged(
    mls_group: &MlsGroup,
    staged_commit: &StagedCommit,
) -> Result<()> {
    ensure!(
        GroupAppData::is_self_group_context(staged_commit.group_context().extensions())
            == GroupAppData::is_self_group_context(mls_group.extensions()),
        "commit would toggle the self-group flag"
    );
    Ok(())
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
