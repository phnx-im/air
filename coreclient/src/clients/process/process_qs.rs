// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::Instant;

use aircommon::{
    credentials::{ClientCredential, VerifiableClientCredential},
    crypto::{aead::AeadDecryptable, indexed_aead::keys::UserProfileKey},
    identifiers::{MimiId, QualifiedGroupId, UserId},
    messages::{
        QueueMessage,
        client_ds::{
            AadMessage, AadPayload, DsCommitResponse, ExtractedQsQueueMessage,
            ExtractedQsQueueMessagePayload, QsQueueTargetedMessage, UserProfileKeyUpdateParams,
            WelcomeBundle,
        },
    },
    time::TimeStamp,
    utils::removed_client,
};
use airprotos::{
    client::group::GroupData,
    queue_service::v1::{QueueEvent, queue_event},
};
use anyhow::{Context, Result, bail, ensure};
use chrono::Utc;
use mimi_content::{
    Disposition, MessageStatus, MessageStatusReport, MimiContent, NestedPartContent,
};
use mimi_room_policy::RoleIndex;
use openmls::{
    group::{GroupId, QueuedProposal},
    prelude::{
        ApplicationMessage, MlsMessageBodyIn, MlsMessageIn, ProcessedMessageContent,
        ProtocolMessage, Sender, StagedCommit,
    },
};
use tls_codec::DeserializeBytes;
use tracing::{debug, error, info, warn};

use crate::{
    ChatAttributes, ChatMessage, ChatStatus, ContentMessage, Message, MimiContentExt,
    SystemMessage,
    chats::{GroupDataExt, StatusRecord, messages::edit::MessageEdit},
    clients::{
        QsListenResponder,
        attachment::AttachmentRecord,
        block_contact::{BlockedContact, BlockedContactError},
        process::process_as::{ConnectionInfoSource, TargetedMessageSource},
        targeted_message::TargetedMessageContent,
        update_key::update_chat_title,
        user_settings::ReadReceiptsSetting,
    },
    contacts::{PartialContact, PartialContactType},
    db_access::{WriteConnection, WriteDbTransaction},
    groups::{
        Group, VerifiedGroup, client_auth_info::StorableClientCredential,
        process::ProcessMessageResult,
    },
    job::{JobContext, JobContextDb, pending_chat_operation::PendingChatOperation},
    key_stores::{indexed_keys::StorableIndexedKey, queue_ratchets::StorableQsQueueRatchet},
    outbound_service::resync::Resync,
    store::Store,
};

use super::{Chat, ChatId, CoreUser, FriendshipPackage, TimestampedMessage, anyhow};

pub enum ProcessQsMessageResult {
    None,
    NewChat(ChatId, Vec<ChatMessage>),
    ChatChanged(ChatId, Vec<ChatMessage>),
    Messages(Vec<ChatMessage>),
    NewConnection(ChatId),
}

#[derive(Debug, Default)]
pub struct ProcessedQsMessages {
    pub new_chats: Vec<ChatId>,
    pub changed_chats: Vec<ChatId>,
    pub new_messages: Vec<ChatMessage>,
    pub errors: Vec<anyhow::Error>,
    pub processed: usize,
    pub new_connections: Vec<ChatId>,
}

impl ProcessedQsMessages {
    pub fn is_empty(&self) -> bool {
        self.new_chats.is_empty()
            && self.changed_chats.is_empty()
            && self.new_messages.is_empty()
            && self.errors.is_empty()
    }
}

#[derive(Default)]
struct ApplicationMessagesHandlerResult {
    new_messages: Vec<TimestampedMessage>,
    updated_messages: Vec<ChatMessage>,
    chat_changed: bool,
}

impl CoreUser {
    /// Process a decrypted message received from the QS queue.
    ///
    /// Returns the [`ChatId`] of newly created chats and any
    /// [`ChatMessage`]s produced by processin the QS message.
    ///
    /// TODO: This function is (still) async, because depending on the message
    /// it processes, it might do one of the following:
    ///
    /// * fetch credentials from the AS to authenticate existing group members
    ///   (when joining a new group) or new group members (when processing an
    ///   Add or external join)
    /// * download AddInfos (KeyPackages, etc.) from the DS. This happens when a
    ///   user externally joins a connection group and the contact is upgraded
    ///   from partial contact to full contact.
    /// * get a QS verifying key from the QS. This also happens when a user
    ///   externally joins a connection group to verify the KeyPackageBatches
    ///   received from the QS as part of the AddInfo download.
    async fn process_qs_message<'a>(
        &'a self,
        txn: &'a mut WriteDbTransaction<'_>,
        qs_queue_message: ExtractedQsQueueMessage,
        read_receipts_enabled: bool,
    ) -> Result<ProcessQsMessageResult> {
        // TODO: We should verify whether the messages are valid messages, i.e.
        // if it doesn't mix requests, etc. I think the DS already does some of this
        // and we might be able to re-use code.

        let started = Instant::now();

        // Keep track of freshly joined groups s.t. we can later update our user auth keys.
        let ds_timestamp = qs_queue_message.timestamp;
        let res = match qs_queue_message.payload {
            ExtractedQsQueueMessagePayload::WelcomeBundle(welcome_bundle) => {
                Box::pin(self.handle_welcome_bundle(txn, welcome_bundle, ds_timestamp)).await
            }
            ExtractedQsQueueMessagePayload::MlsMessage(mls_message) => {
                self.handle_mls_message(txn, *mls_message, ds_timestamp, read_receipts_enabled)
                    .await
            }
            ExtractedQsQueueMessagePayload::UserProfileKeyUpdate(
                user_profile_key_update_params,
            ) => {
                self.handle_user_profile_key_update(txn, user_profile_key_update_params)
                    .await
            }
            ExtractedQsQueueMessagePayload::TargetedMessage(
                QsQueueTargetedMessage::ApplicationMessage(mls_message_bytes),
            ) => {
                let mls_message = MlsMessageIn::tls_deserialize_exact_bytes(&mls_message_bytes)
                    .context("Failed to deserialize targeted MLS message")?;
                self.handle_targeted_application_message(txn, mls_message, ds_timestamp)
                    .await
            }
            ExtractedQsQueueMessagePayload::DsCommitResponse(ds_commit_response) => {
                self.handle_commit_response(txn, ds_commit_response).await
            }
        };

        debug!(elapsed = ?started.elapsed(), "Processed QS message");
        res
    }

    async fn handle_commit_response(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        commit_response: DsCommitResponse,
    ) -> Result<ProcessQsMessageResult> {
        let DsCommitResponse {
            group_id,
            epoch,
            timestamp,
        } = commit_response;

        // Load the group by group_id
        let mut group = Group::load_verified(&mut *txn, &group_id)
            .await?
            .context("Can't find group for commit response")?;

        // Check how the message epoch compares to our group's local epoch.
        if group.mls_group().epoch() < epoch {
            error!(
                local_epoch=?group.mls_group().epoch(),
                confirmation_epoch=?epoch,
                "Received commit response for future epoch",
            );
            bail!("Received commit response for future epoch");
        } else if group.mls_group().epoch() > epoch {
            // It's just a confirmation for an old commit we already merged.
            return Ok(ProcessQsMessageResult::None);
        }

        // If yes, merge the commit and store the updated group
        let (mut group_messages, group_data_bytes) =
            group.merge_pending_commit(txn, None, timestamp).await?;
        group
            .group_mut()
            .store_update(&mut *txn, Some(timestamp))
            .await?;

        let mut chat = Chat::load_by_group_id(&mut *txn, &group_id)
            .await?
            .context("Can't find chat for commit response")?;

        // Update group data in chat attributes if present
        if let Some(group_data_bytes) = group_data_bytes {
            let group_data = GroupData::decode(&group_data_bytes)?;
            let (chat_title, _external_group_profile) =
                group_data.into_parts(group.identity_link_wrapper_key());
            // No need to fetch the group profile: this is our own commit response, so the
            // profile data is already available locally.
            if let Some(title) = chat_title {
                update_chat_title(
                    &mut *txn,
                    &mut chat,
                    self.user_id(),
                    title,
                    timestamp,
                    &mut group_messages,
                )
                .await?;
            }
        }
        CoreUser::store_new_messages(&mut *txn, chat.id(), group_messages).await?;

        // Delete the pending chat operation
        PendingChatOperation::delete(txn, &group_id).await?;

        Ok(ProcessQsMessageResult::None)
    }

    async fn handle_welcome_bundle(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        welcome_bundle: WelcomeBundle,
        ds_timestamp: TimeStamp,
    ) -> Result<ProcessQsMessageResult> {
        // WelcomeBundle Phase 1: Join the group. This might involve
        // loading AS credentials or fetching them from the AS.

        let (group, sender_user_id, member_profile_info) = Group::join_group(
            welcome_bundle,
            &self.inner.key_store.wai_ear_key,
            &mut *txn,
            &self.inner.api_clients,
            self.signing_key(),
        )
        .await?;
        let group_id = group.group_id().clone();

        // WelcomeBundle Phase 2: Fetch the user profiles of the group members
        // and decrypt them.

        // TODO: This can fail in some cases. If it does, we should fetch and
        // process messages and then try again.
        let mut own_profile_key_in_group = None;
        for profile_info in member_profile_info {
            // TODO: Don't fetch while holding a transaction!
            if profile_info.client_credential.user_id() == self.user_id() {
                // We already have our own profile info.
                own_profile_key_in_group = Some(profile_info.user_profile_key);
                continue;
            }
            Self::schedule_fetch_user_profile(&mut *txn, profile_info).await?;
        }

        let Some(own_profile_key_in_group) = own_profile_key_in_group else {
            bail!("No profile info for our user found");
        };

        // WelcomeBundle Phase 3: Store the user profiles of the group
        // members if they don't exist yet and store the group and the
        // new chat.

        // Set the chat attributes according to the group's
        // group data.
        let group_data_bytes = group.group_data().context("No group data")?;
        let group_data = GroupData::decode(&group_data_bytes)?;
        let (title, external_group_profile) =
            group_data.into_parts(group.identity_link_wrapper_key());
        let title = title.context("No group title")?;
        let attributes = ChatAttributes {
            title,
            picture: None, // Group picture is not yet available
        };
        if let Some(external_group_profile) = external_group_profile {
            Self::schedule_fetch_group_profile(
                &mut *txn,
                group_id.clone(),
                sender_user_id.clone(),
                ds_timestamp,
                external_group_profile,
            )
            .await?;
        }

        let chat = Chat::new_group_chat(group_id.clone(), attributes);
        let own_profile_key = UserProfileKey::load_own(&mut *txn).await?;
        // If we've been in that chat before, we delete the old chat
        // first and then create a new one. We do leave the messages
        // intact, though.
        chat.store(&mut *txn).await?;

        // Add system message who added us to the group.
        let system_message = ChatMessage::new_system_message(
            chat.id(),
            ds_timestamp,
            SystemMessage::Add(sender_user_id, self.user_id().clone()),
        );
        system_message.store(&mut *txn).await?;

        // WelcomeBundle Phase 4: Check whether our user profile key is up to
        // date and if not, update it.
        if own_profile_key_in_group != own_profile_key {
            let qualified_group_id = QualifiedGroupId::try_from(group.group_id().clone())?;
            let api_client = self
                .inner
                .api_clients
                .get(qualified_group_id.owning_domain())?;
            let encrypted_profile_key =
                own_profile_key.encrypt(group.identity_link_wrapper_key(), self.user_id())?;
            let params = UserProfileKeyUpdateParams {
                group_id: group.group_id().clone(),
                sender_index: group.own_index(),
                user_profile_key: encrypted_profile_key,
            };
            api_client
                .ds_user_profile_key_update(params, self.signing_key(), group.group_state_ear_key())
                .await?;
        }

        let messages = vec![system_message];
        Ok(ProcessQsMessageResult::NewChat(chat.id(), messages))
    }

    async fn handle_targeted_application_message<'a>(
        &'a self,
        txn: &'a mut WriteDbTransaction<'_>,
        mls_message: MlsMessageIn,
        ds_timestamp: TimeStamp,
    ) -> Result<ProcessQsMessageResult> {
        let MlsMessageBodyIn::PrivateMessage(app_msg) = mls_message.extract() else {
            bail!("Unexpected message type")
        };
        let protocol_message = ProtocolMessage::from(app_msg);

        // MLSMessage Phase 1: Load the chat and the group.
        let group_id = protocol_message.group_id().clone();

        let chat = Chat::load_by_group_id(&mut *txn, &group_id)
            .await?
            .ok_or_else(|| anyhow!("No chat found for group ID {:?}", group_id))?;
        let mut group = Group::load_verified(&mut *txn, &group_id)
            .await?
            .ok_or_else(|| anyhow!("No group found for group ID {:?}", group_id))?;

        // MLSMessage Phase 2: Process the message
        let Some(ProcessMessageResult {
            processed_message, ..
        }) = group
            .group_mut()
            .process_message(&mut *txn, &self.inner.api_clients, protocol_message)
            .await?
        else {
            // TODO: Once we have a UX for resyncs, we should schedule one
            // here and re-enable the resync test in integration.rs
            let _resync = Resync {
                chat_id: chat.id(),
                group_id: group.group_id().clone(),
                group_state_ear_key: group.group_state_ear_key().clone(),
                identity_link_wrapper_key: group.identity_link_wrapper_key().clone(),
                original_leaf_index: group.own_index(),
            };
            return Ok(ProcessQsMessageResult::None);
        };

        let Sender::Member(sender_index) = processed_message.sender() else {
            bail!("Sender is not a member");
        };
        let sender_client_credential = group
            .credential_at(*sender_index)?
            .context("No sender client credential found")?;

        let ProcessedMessageContent::ApplicationMessage(application_message) =
            processed_message.into_content()
        else {
            bail!("Only application messages are expected in targeted messages");
        };

        let TargetedMessageContent::ConnectionRequest(connection_info) =
            TargetedMessageContent::tls_deserialize_exact_bytes(&application_message.into_bytes())?;

        // Extract connection info source from the targeted message
        let connection_info_source =
            ConnectionInfoSource::TargetedMessage(Box::new(TargetedMessageSource {
                connection_info,
                sender_client_credential,
                origin_chat_id: chat.id(),
                sent_at: ds_timestamp,
            }));

        let mut context = JobContext {
            api_clients: &self.inner.api_clients,
            http_client: &self.inner.http_client,
            db: JobContextDb::Transaction(txn),
            key_store: &self.inner.key_store,
            now: Utc::now(),
        };

        let chat_id =
            CoreUser::process_connection_offer(&mut context, connection_info_source).await?;

        Ok(ProcessQsMessageResult::NewConnection(chat_id))
    }

    async fn handle_mls_message(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        mls_message: MlsMessageIn,
        ds_timestamp: TimeStamp,
        read_receipts_enabled: bool,
    ) -> Result<ProcessQsMessageResult> {
        let protocol_message: ProtocolMessage = match mls_message.extract() {
            MlsMessageBodyIn::PublicMessage(handshake_message) =>
                handshake_message.into(),
            // Only application messages are private
            MlsMessageBodyIn::PrivateMessage(app_msg) => app_msg.into(),
            // Welcomes always come as a WelcomeBundle, not as an MLSMessage.
            MlsMessageBodyIn::Welcome(_) |
            // Neither GroupInfos nor KeyPackages should come from the queue.
            MlsMessageBodyIn::GroupInfo(_) | MlsMessageBodyIn::KeyPackage(_) => bail!("Unexpected message type"),
        };
        // MLSMessage Phase 1: Load the chat and the group.
        let group_id = protocol_message.group_id().clone();

        let chat = Chat::load_by_group_id(&mut *txn, &group_id)
            .await?
            .ok_or_else(|| anyhow!("No chat found for group ID {:?}", group_id))?;
        let chat_id = chat.id();

        // Load the group regardless of whether it has a pending commit or not.
        let mut group = Group::load_verified(&mut *txn, &group_id)
            .await?
            .ok_or_else(|| anyhow!("No group found for group ID {:?}", group_id))?;

        // MLSMessage Phase 2: Process the message

        let Some(ProcessMessageResult {
            processed_message,
            we_were_removed,
            profile_infos,
        }) = group
            .group_mut()
            .process_message(&mut *txn, &self.inner.api_clients, protocol_message)
            .await?
        else {
            // TODO: Once we have a UX for resyncs, we should schedule one
            // here and re-enable the resync test in integration.rs
            let _resync = Resync {
                chat_id,
                group_id: group.group_id().clone(),
                group_state_ear_key: group.group_state_ear_key().clone(),
                identity_link_wrapper_key: group.identity_link_wrapper_key().clone(),
                original_leaf_index: group.own_index(),
            };

            return Ok(ProcessQsMessageResult::None);
        };

        let sender = processed_message.sender().clone();
        let sender_user_id =
            VerifiableClientCredential::from_basic_credential(processed_message.credential())?
                .user_id()
                .clone();

        let aad = processed_message.aad().to_vec();

        // `chat_changed` indicates whether the state of the chat was updated
        let (new_messages, updated_messages, chat_changed) = match processed_message.into_content()
        {
            ProcessedMessageContent::ApplicationMessage(application_message) => {
                // Drop messages in 1:1 blocked chats Note: In group chats, messages
                // from blocked users are still received and processed.
                if chat.status() == &ChatStatus::Blocked {
                    bail!(BlockedContactError);
                }
                let ApplicationMessagesHandlerResult {
                    new_messages,
                    updated_messages,
                    chat_changed,
                } = self
                    .handle_application_message(
                        &mut *txn,
                        &group,
                        application_message,
                        ds_timestamp,
                        &sender_user_id,
                        read_receipts_enabled,
                    )
                    .await?;
                (new_messages, updated_messages, chat_changed)
            }
            ProcessedMessageContent::ProposalMessage(proposal) => {
                let (new_messages, updated) = self
                    .handle_proposal_message(&mut *txn, &mut group, *proposal, ds_timestamp)
                    .await?;
                group.group_mut().store_update(&mut *txn, None).await?;
                (new_messages, Vec::new(), updated)
            }
            ProcessedMessageContent::StagedCommitMessage(staged_commit) => {
                let sender_client_credential =
                    StorableClientCredential::load_by_user_id(&mut *txn, &sender_user_id)
                        .await?
                        .ok_or_else(|| anyhow!("No sender client credential found"))?
                        .into();
                let (new_messages, updated) = self
                    .handle_staged_commit_message(
                        &mut *txn,
                        &mut group,
                        chat,
                        *staged_commit,
                        aad,
                        ds_timestamp,
                        &sender,
                        &sender_client_credential,
                        we_were_removed,
                    )
                    .await?;
                group.group_mut().store_update(&mut *txn, None).await?;
                (new_messages, Vec::new(), updated)
            }
            ProcessedMessageContent::ExternalJoinProposalMessage(_) => {
                let (new_messages, updated) = self.handle_external_join_proposal_message()?;
                (new_messages, Vec::new(), updated)
            }
        };

        let mut messages = Self::store_new_messages(&mut *txn, chat_id, new_messages).await?;
        for updated_message in updated_messages {
            updated_message.update(&mut *txn).await?;
            messages.push(updated_message);
        }

        // Schedule delivery receipts for incoming messages
        let delivery_receipts = messages.iter().filter_map(|message| {
            if let Message::Content(content_message) = message.message()
                && let Disposition::Render | Disposition::Attachment =
                    content_message.content().nested_part.disposition
                && let Some(mimi_id) = content_message.mimi_id()
            {
                Some((message.id(), mimi_id, MessageStatus::Delivered))
            } else {
                None
            }
        });

        self.outbound_service()
            .schedule_receipts(&mut *txn, chat_id, delivery_receipts)
            .await?;

        let res = match (messages, chat_changed) {
            (messages, true) => ProcessQsMessageResult::ChatChanged(chat_id, messages),
            (messages, false) => ProcessQsMessageResult::Messages(messages),
        };

        // MLSMessage Phase 4: Fetch user profiles of new clients and store them.
        for profile_info in profile_infos {
            Self::schedule_fetch_user_profile(&mut *txn, profile_info).await?;
        }

        Ok(res)
    }

    /// Returns a message if it should be stored, otherwise an empty vec.
    ///
    /// Also returns whether the chat should be notified as updated.
    #[allow(clippy::too_many_arguments)]
    async fn handle_application_message(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        group: &Group,
        application_message: ApplicationMessage,
        ds_timestamp: TimeStamp,
        sender: &UserId,
        read_receipts_enabled: bool,
    ) -> anyhow::Result<ApplicationMessagesHandlerResult> {
        let mut content = MimiContent::deserialize(&application_message.into_bytes());

        // Delivery receipt
        if let Ok(content) = &content
            && let NestedPartContent::SinglePart {
                content_type,
                content: report_content,
            } = &content.nested_part.part
            && content_type == "application/mimi-message-status"
        {
            let mut report = MessageStatusReport::deserialize(report_content)?;
            if !read_receipts_enabled {
                report
                    .statuses
                    .retain(|status| status.status != MessageStatus::Read);
                if report.statuses.is_empty() {
                    debug!("Dropping read receipt because read receipts are disabled");
                    return Ok(Default::default());
                }
            }
            StatusRecord::borrowed(sender, report, ds_timestamp)
                .store_report(txn)
                .await?;
            // Delivery receipt messages are not stored
            return Ok(Default::default());
        }

        // Message edit
        if let Ok(content) = &mut content
            && let Some(replaces) = content.replaces.as_ref()
            && let Ok(mimi_id) = MimiId::from_slice(replaces)
        {
            // Don't fail here, otherwise message processing of other messages will fail.
            let mut savepoint_txn = txn.begin().await?;
            let message = handle_message_edit(
                &mut savepoint_txn,
                group.group_id(),
                ds_timestamp,
                sender,
                mimi_id,
                std::mem::take(content),
            )
            .await
            .inspect_err(|error| {
                // We don't have the message to edit in our database, so we
                // can't apply the edit. This can happen if the original message
                // was deleted or if the original message was sent before we
                // joined the group and we don't have the original message in
                // our database. In this case, we just skip the edit.
                warn!(%error, "Cannot edit message because original message is missing; skipping");
            })
            .ok();
            if message.is_some() {
                savepoint_txn.commit().await?;
            }

            return Ok(ApplicationMessagesHandlerResult {
                updated_messages: message.into_iter().collect(),
                chat_changed: true,
                ..Default::default()
            });
        }

        let message =
            TimestampedMessage::from_mimi_content_result(content, ds_timestamp, sender, group);
        Ok(ApplicationMessagesHandlerResult {
            new_messages: vec![message],
            chat_changed: true,
            ..Default::default()
        })
    }

    async fn read_receipts_enabled(&self) -> bool {
        self.user_setting::<ReadReceiptsSetting>()
            .await
            .map(|setting| setting.0)
            .unwrap_or(true)
    }

    async fn handle_proposal_message(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        group: &mut VerifiedGroup,
        proposal: QueuedProposal,
        ds_timestamp: TimeStamp,
    ) -> anyhow::Result<(Vec<TimestampedMessage>, bool)> {
        let mut messages = Vec::new();

        let Sender::Member(sender_index) = proposal.sender() else {
            bail!("No external senders supported yet");
        };

        let removed_index = removed_client(&proposal)
            .context("Only Removes and SelfRemoves are supported for now")?;

        let Some(removed_credential) = group.credential_at(removed_index)? else {
            warn!("Removed user credential not found");
            return Ok((vec![], false));
        };
        let removed = removed_credential.user_id();

        let Some(sender_credential) = group.credential_at(*sender_index)? else {
            warn!("Sender credential not found");
            return Ok((vec![], false));
        };
        let sender = sender_credential.user_id();

        ensure!(
            sender == removed,
            "A user should not send remove proposals for other users"
        );

        group
            .group_mut()
            .room_state_change_role(sender, sender, RoleIndex::Outsider)?;

        messages.push(TimestampedMessage::system_message(
            SystemMessage::Remove(sender.clone(), removed.clone()),
            ds_timestamp,
        ));

        // For now, we don't to anything here. The proposal
        // was processed by the MLS group and will be
        // committed with the next commit.
        group.group_mut().store_proposal(txn, proposal)?;

        Ok((messages, false))
    }

    #[expect(clippy::too_many_arguments)]
    async fn handle_staged_commit_message(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        group: &mut VerifiedGroup,
        mut chat: Chat,
        staged_commit: StagedCommit,
        aad: Vec<u8>,
        ds_timestamp: TimeStamp,
        sender: &Sender,
        sender_client_credential: &ClientCredential,
        we_were_removed: bool,
    ) -> anyhow::Result<(Vec<TimestampedMessage>, bool)> {
        // If a client joined externally, we check if the
        // group belongs to an unconfirmed chat.

        // StagedCommitMessage Phase 1: Confirm the chat if unconfirmed

        let (chat_changed, mut group_messages) = if chat.is_unconfirmed() {
            let group_messages = self
                .handle_unconfirmed_chat(
                    txn,
                    aad,
                    ds_timestamp,
                    sender,
                    sender_client_credential,
                    &mut chat,
                    group.group_mut(),
                )
                .await?;
            (true, vec![group_messages])
        } else {
            (false, vec![])
        };

        // StagedCommitMessage Phase 2: Merge the staged commit into the group.

        // If we were removed, we set the group to inactive.
        if we_were_removed {
            let past_members = group.members().collect();
            chat.set_inactive(&mut *txn, past_members).await?;
        }
        let (messages_from_commit, group_data_bytes) = group
            .merge_pending_commit(&mut *txn, staged_commit, ds_timestamp)
            .await?;

        group_messages.extend(messages_from_commit);

        if let Some(group_data_bytes) = group_data_bytes {
            let group_data = GroupData::decode(&group_data_bytes)?;
            let (chat_title, external_group_profile) =
                group_data.into_parts(group.identity_link_wrapper_key());
            if let Some(external_group_profile) = external_group_profile {
                Self::schedule_fetch_group_profile(
                    &mut *txn,
                    chat.group_id().clone(),
                    sender_client_credential.user_id().clone(),
                    ds_timestamp,
                    external_group_profile,
                )
                .await?;
            }
            if let Some(title) = chat_title {
                // Update chat title according to new group data
                update_chat_title(
                    txn,
                    &mut chat,
                    sender_client_credential.user_id(),
                    title,
                    ds_timestamp,
                    &mut group_messages,
                )
                .await?;
            }
        }

        Ok((group_messages, chat_changed))
    }

    #[expect(clippy::too_many_arguments)]
    async fn handle_unconfirmed_chat(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        aad: Vec<u8>,
        ds_timestamp: TimeStamp,
        sender: &Sender,
        sender_client_credential: &ClientCredential,
        chat: &mut Chat,
        group: &mut Group,
    ) -> Result<TimestampedMessage, anyhow::Error> {
        let Some(contact_type) = chat.chat_type().unconfirmed_contact() else {
            bail!("Chat is not unconfirmed");
        };

        // Check if it was an external commit
        ensure!(
            matches!(sender, Sender::NewMemberCommit),
            "Incoming commit to ConnectionGroup was not an external commit"
        );

        let sender_user_id = sender_client_credential.user_id();

        if let PartialContactType::TargetedMessage(chat_user_id) = &contact_type {
            ensure!(
                sender_user_id == chat_user_id,
                "Sender identity does not match targeted message user ID"
            );
        }

        // UnconfirmedConnection Phase 1: Load up the partial contact and decrypt the
        // friendship package
        let contact = PartialContact::load(&mut *txn, &contact_type)
            .await?
            .context("No contact found: {contact:?}")?;

        // This is a bit annoying, since we already
        // de-serialized this in the group processing
        // function, but we need the encrypted
        // friendship package here.
        let encrypted_friendship_package = if let AadPayload::JoinConnectionGroup(payload) =
            AadMessage::tls_deserialize_exact_bytes(&aad)?.into_payload()
        {
            payload.encrypted_friendship_package
        } else {
            bail!("Unexpected AAD payload")
        };

        let friendship_package = FriendshipPackage::decrypt(
            contact.friendship_package_ear_key(),
            &encrypted_friendship_package,
        )?;

        let user_profile_key = UserProfileKey::from_base_secret(
            friendship_package.user_profile_base_secret.clone(),
            sender_user_id,
        )?;

        // UnconfirmedConnection Phase 2: Fetch the user profile.
        Self::schedule_fetch_user_profile(
            &mut *txn,
            (sender_client_credential.clone(), user_profile_key),
        )
        .await?;

        // Now we can turn the partial contact into a full one.
        let contact = contact
            .mark_as_complete(&mut *txn, sender_user_id.clone(), friendship_package)
            .await?;

        // Room state update: Pretend that we just invited that user
        // We do that now, because we didn't know that user id when we created the room.
        group.room_state_change_role(self.user_id(), sender_user_id, RoleIndex::Regular)?;

        chat.confirm(txn, contact.user_id).await?;

        let user_handle = if let PartialContactType::Handle(handle) = contact_type {
            Some(handle.clone())
        } else {
            None
        };
        let system_message = SystemMessage::ReceivedConnectionConfirmation {
            sender: sender_user_id.clone(),
            user_handle,
        };

        let message = TimestampedMessage::system_message(system_message, ds_timestamp);

        Ok(message)
    }

    async fn handle_user_profile_key_update(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        params: UserProfileKeyUpdateParams,
    ) -> anyhow::Result<ProcessQsMessageResult> {
        // Don't update the profile if the chat is blocked
        let chat_id = ChatId::try_from(&params.group_id)?;
        if BlockedContact::check_blocked_chat(&mut *txn, chat_id).await? {
            bail!(BlockedContactError);
        }

        // Phase 1: Load the group and the sender.
        let group = Group::load_verified(&mut *txn, &params.group_id)
            .await?
            .context("No group found")?;
        let sender_credential = group
            .credential_at(params.sender_index)?
            .context("No sender credential found")?;
        let sender = sender_credential.user_id();

        // Phase 2: Decrypt the new user profile key
        let new_user_profile_key = UserProfileKey::decrypt(
            group.identity_link_wrapper_key(),
            &params.user_profile_key,
            sender,
        )?;

        // Phase 3: Fetch and store the (new) user profile and key
        Self::schedule_fetch_user_profile(txn, (sender_credential, new_user_profile_key)).await?;

        Ok(ProcessQsMessageResult::None)
    }

    fn handle_external_join_proposal_message(
        &self,
    ) -> anyhow::Result<(Vec<TimestampedMessage>, bool)> {
        unimplemented!()
    }

    /// Convenience function that takes a list of `QueueMessage`s retrieved from
    /// the QS, decrypts them, and processes them.
    pub async fn fully_process_qs_messages(
        &self,
        qs_messages: Vec<QueueMessage>,
    ) -> ProcessedQsMessages {
        let mut result = ProcessedQsMessages::default();
        let num_messages = qs_messages.len();
        let read_receipts_enabled = self.read_receipts_enabled().await;

        let started = Instant::now();

        // Process each qs message individually
        //
        // Each loop iteration MUST be a cancel-safe and process-safe future. The former is
        // important because the app can be shut down any time. The latter is important because the
        // QS messages are processed in the foreground and background handlers.
        for (idx, qs_message) in qs_messages.into_iter().enumerate() {
            // Start an outer transaction where the ratchet is loaded and updated. A savepoint after
            // the ratchet is loaded is passed to the processing of the QS message. This savepoint
            // can be rolled back but this transaction MUST be committed. It is needed to make sure
            // that processing is cancel-safe.
            let mut connection = match self.db().write().await {
                Ok(c) => c,
                Err(error) => {
                    error!(%error, "Failed to start the ratchet transaction");
                    result.processed = idx;
                    return result;
                }
            };

            let mut txn = match connection.begin().await {
                Ok(txn) => txn,
                Err(error) => {
                    error!(%error, "Failed to start the ratchet transaction");
                    result.processed = idx;
                    return result;
                }
            };

            // Decrypt and process the message (and Box the large future)
            if let Err(error) = Box::pin(self.decrypt_and_process_qs_message(
                &mut txn,
                qs_message,
                &mut result,
                read_receipts_enabled,
            ))
            .await
            {
                error!(%error, "Fatal error when processing a QS message; stopping loop");
                result.processed = idx;
                return result; // Stop processing
            }

            // Commit the ratchet update
            txn.commit()
                .await
                .inspect_err(|error| {
                    error!(%error, "Failed to commit the ratchet transaction");
                })
                .ok();

            connection.notify();
        }

        debug!(elapsed = ?started.elapsed(), num_messages, "Processed QS messages");

        result.processed = num_messages;
        result
    }

    /// Returns `Ok(())` if the more messages should be processed, or `Err` if the processing
    /// should be aborted.
    async fn decrypt_and_process_qs_message(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        qs_message: QueueMessage,
        result: &mut ProcessedQsMessages,
        read_receipts_enabled: bool,
    ) -> anyhow::Result<()> {
        let qs_message_payload = StorableQsQueueRatchet::decrypt_qs_queue_message(txn, qs_message)
            .await
            .context("Decrypting message failed")?;
        let qs_message_plaintext = match qs_message_payload.extract() {
            Ok(extracted) => extracted,
            Err(error) => {
                error!(%error, "Extracting message failed; dropping message");
                return Ok(());
            }
        };

        // We create a nested savepoint transaction that we can rollback independently from
        // the parent txn which contains the updates done to the queue ratchet.
        //
        // If the handler fails, we want to *silently* rollback this savepoint, while always
        // committing the parent one.
        let mut savepoint_txn = txn.begin().await?;

        let processed = match Box::pin(self.process_qs_message(
            &mut savepoint_txn,
            qs_message_plaintext,
            read_receipts_enabled,
        ))
        .await
        {
            Ok(processed) => {
                savepoint_txn.commit().await?;
                processed
            }
            Err(error) if error.downcast_ref::<BlockedContactError>().is_some() => {
                info!("Dropping message from blocked contact");
                return Ok(());
            }
            Err(error)
                if error
                    .downcast_ref::<sqlx::Error>()
                    .is_some_and(|error| error.as_database_error().is_some()) =>
            {
                // Fatal error, stop processing
                return Err(error);
            }
            Err(error) => {
                error!(%error, "Processing message failed; continue");
                result.errors.push(error);
                return Ok(());
            }
        };

        match processed {
            ProcessQsMessageResult::Messages(messages) => {
                result.new_messages.extend(messages);
            }
            ProcessQsMessageResult::ChatChanged(chat_id, messages) => {
                result.new_messages.extend(messages);
                result.changed_chats.push(chat_id);
            }
            ProcessQsMessageResult::NewChat(chat_id, messages) => {
                result.new_messages.extend(messages);
                result.new_chats.push(chat_id);
            }
            ProcessQsMessageResult::None => {}
            ProcessQsMessageResult::NewConnection(chat_id) => result.new_connections.push(chat_id),
        }

        Ok(())
    }
}

async fn handle_message_edit(
    txn: &mut WriteDbTransaction<'_>,
    group_id: &GroupId,
    ds_timestamp: TimeStamp,
    sender: &UserId,
    replaces: MimiId,
    content: MimiContent,
) -> anyhow::Result<ChatMessage> {
    let is_delete = content.nested_part.part == NestedPartContent::NullPart;

    // First try to directly load the original message by mimi id (non-edited message) and fallback
    // to the history of edits otherwise.
    let mut message = match ChatMessage::load_by_mimi_id(&mut *txn, &replaces).await? {
        Some(message) => message,
        None => {
            let message_id = MessageEdit::find_message_id(&mut *txn, &replaces)
                .await?
                .with_context(|| {
                    format!("Original message id not found for editing; mimi_id = {replaces:?}")
                })?;

            ChatMessage::load(&mut *txn, message_id)
                .await?
                .with_context(|| {
                    format!("Original message not found for editing; message_id = {message_id:?}")
                })?
        }
    };

    let original_message_id = message.id();
    let original_mimi_id = message
        .message()
        .mimi_id()
        .context("Original message does not have mimi id")?;
    let original_sender = message
        .message()
        .sender()
        .context("Original message does not have sender")?;
    let original_mimi_content = message
        .message()
        .mimi_content()
        .context("Original message does not have mimi content")?;

    // TODO: Use mimi-room-policy for capabilities
    ensure!(
        original_sender == sender,
        "Only edits and deletes from original users are allowed for now"
    );

    if is_delete {
        // We need to redact existing references to the message we delete.
        if let Ok(redacted_mimi_id_bytes) = content.mimi_id(sender, group_id)
            && let Ok(redacted_mimi_id) = MimiId::from_slice(&redacted_mimi_id_bytes)
        {
            let updated_message_ids = ChatMessage::redact_all_in_reply_to_mimi_ids(
                &mut *txn,
                &original_message_id,
                original_mimi_id,
                &redacted_mimi_id,
            )
            .await?;

            for message_id in updated_message_ids {
                txn.notifier().add(message_id);
            }
        }

        // Delete edit history when message is deleted
        MessageEdit::delete_by_message_id(&mut *txn, message.id()).await?;
        // Delete attachments for this message
        AttachmentRecord::delete_by_message_id(&mut *txn, message.id()).await?;
    } else {
        // Store message edit
        MessageEdit::new(
            original_mimi_id,
            message.id(),
            ds_timestamp,
            original_mimi_content,
        )
        .store(&mut *txn)
        .await?;
    }

    // Update the original message
    let is_sent = true;
    message.set_content_message(ContentMessage::new(
        original_sender.clone(),
        is_sent,
        content,
        group_id,
    ));
    message.set_edited_at(ds_timestamp);
    if is_delete {
        message.set_status(MessageStatus::Deleted);
    } else {
        message.set_status(MessageStatus::Unread);
    }

    // Clear the status of the message
    StatusRecord::clear(txn, message.id()).await?;

    Ok(message)
}

/// A processor for the streamed QS events.
///
/// This processor is meant to be used in the streaming context where the events are streamed one
/// by one and this process never finishes until the stream is closed. Each event is processed by
/// `[Self::process_event]`.
#[derive(Debug)]
pub struct QsStreamProcessor {
    responder: Option<QsListenResponder>,
    /// Accumulated but not yet processed messages
    ///
    /// Note: It is safe to keep messages in memory here, because they are not yet decrypted.
    /// Decryption increases the locally stored ratchet sequence number, which is used to determine
    /// which messages should be fetched from the server. In case, the app is shut down, the
    /// messages will be received again.
    messages: Vec<QueueMessage>,
}

impl QsStreamProcessor {
    pub fn new(responder: Option<QsListenResponder>) -> Self {
        Self {
            responder,
            messages: Vec::new(),
        }
    }

    pub fn replace_responder(&mut self, responder: QsListenResponder) {
        self.responder.replace(responder);
    }

    pub async fn process_event(
        &mut self,
        core_user: &CoreUser,
        event: QueueEvent,
    ) -> QsProcessEventResult {
        debug!(?event, "processing QS listen event");

        match event.event {
            None => {
                error!("received an empty event");
                QsProcessEventResult::Ignored
            }
            Some(queue_event::Event::Payload(_)) => {
                // currently, we don't handle payload events
                warn!("ignoring QS listen payload event");
                QsProcessEventResult::Ignored
            }
            Some(queue_event::Event::Message(message)) => match message.try_into() {
                Ok(message) => {
                    // Invariant: after a message there is always an Empty event as sentinel
                    // => accumulated messages will be processed there
                    self.messages.push(message);

                    // Stop the background task and wait until it is fully stopped
                    core_user.outbound_service().stop().await;

                    QsProcessEventResult::Accumulated
                }
                Err(error) => {
                    error!(%error, "failed to convert QS message; dropping");
                    QsProcessEventResult::Ignored
                }
            },
            // Empty event indicates that the queue is empty
            Some(queue_event::Event::Empty(_)) => {
                let max_sequence_number = self.messages.last().map(|m| m.sequence_number);

                let messages = std::mem::take(&mut self.messages);
                let num_messages = messages.len();

                let processed_messages = core_user.fully_process_qs_messages(messages).await;

                let result = if processed_messages.processed < num_messages {
                    error!(
                        processed_messages.processed,
                        num_messages, "failed to fully process messages"
                    );
                    QsProcessEventResult::PartiallyProcessed {
                        dropped: num_messages - processed_messages.processed,
                        processed: processed_messages,
                    }
                } else {
                    if let Some(max_sequence_number) = max_sequence_number {
                        // We received some messages, so we can ack them *after* they were fully
                        // processed. In particular, the queue ratchet sequence number has been already
                        // written back into the database.
                        if let Some(responder) = self.responder.as_ref() {
                            // Acks all messages before max_sequence_number + 1 (exclusive)
                            responder.ack(max_sequence_number + 1).await;
                        } else {
                            error!("logic error: no responder to ack QS messages");
                        }
                    }

                    QsProcessEventResult::FullyProcessed {
                        processed: processed_messages,
                    }
                };

                // Start the background task, but don't wait for it to start
                drop(core_user.outbound_service().start());

                result
            }
        }
    }
}

#[derive(Debug)]
pub enum QsProcessEventResult {
    /// Event was accumulated to be processed later
    Accumulated,
    /// Event was ignored
    Ignored,
    /// All accumulated events where fully processed
    FullyProcessed { processed: ProcessedQsMessages },
    /// Accumulated events were partially processed, some events were dropped
    PartiallyProcessed {
        processed: ProcessedQsMessages,
        dropped: usize,
    },
}

impl QsProcessEventResult {
    pub fn processed(&self) -> usize {
        match self {
            Self::Accumulated => 0,
            Self::Ignored => 0,
            Self::FullyProcessed { processed } => processed.processed,
            Self::PartiallyProcessed { processed, .. } => processed.processed,
        }
    }

    pub fn is_partially_processed(&self) -> bool {
        matches!(self, Self::PartiallyProcessed { .. })
    }
}

#[cfg(test)]
mod tests {
    use aircommon::{identifiers::UserId, time::TimeStamp};
    use mimi_content::{ByteBuf, MimiContent};
    use sqlx::SqlitePool;

    use crate::{
        ChatMessage, ContentMessage, MessageId,
        chats::persistence::tests::test_chat,
        clients::process::process_qs::handle_message_edit,
        db_access::{DbAccess, WriteConnection},
    };

    /// Editing a message (without deleting) should not update any `in_reply_to` references.
    #[sqlx::test]
    async fn test_handle_message_edit_does_not_update_reply_references(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        let chat = test_chat();
        chat.store(pool.write().await?).await?;

        let group_id = chat.group_id();
        let domain = "localhost".parse().unwrap();
        let alice = UserId::random(domain);
        let bob = UserId::random("localhost".parse().unwrap());

        // Alice sends a message
        let alice_message = ChatMessage::new_for_test(
            chat.id(),
            MessageId::random(),
            TimeStamp::now(),
            ContentMessage::new(
                alice.clone(),
                false,
                MimiContent::simple_markdown_message("Hello from Alice!".to_string(), [0; 16]),
                group_id,
            ),
        );
        alice_message.store(pool.write().await?).await?;
        let original_alice_mimi_id = *alice_message.message().mimi_id().unwrap();

        // Bob replies to Alice's message
        let mut bob_mimi_content =
            MimiContent::simple_markdown_message("Hello from Bob!".to_string(), [1; 16]);
        bob_mimi_content.in_reply_to = alice_message
            .message()
            .mimi_id()
            .map(|mimi_id| ByteBuf::from(mimi_id.as_slice()));
        let bob_message = ChatMessage::new_for_test(
            chat.id(),
            MessageId::random(),
            TimeStamp::now(),
            ContentMessage::new(bob.clone(), false, bob_mimi_content, group_id),
        );
        bob_message.store(pool.write().await?).await?;

        // Alice edits her message (no delete)
        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;
        let edited_alice_content = MimiContent::simple_markdown_message(
            "Hello from Alice! WITH EDIT".to_string(),
            [0; 16],
        );
        let alice_message = handle_message_edit(
            &mut txn,
            group_id,
            TimeStamp::now(),
            &alice,
            original_alice_mimi_id,
            edited_alice_content,
        )
        .await?;
        alice_message.update(&mut txn).await?;

        // Bob's in_reply_to should still reference the original MIMI ID
        let bob_message = ChatMessage::load(&mut txn, bob_message.id())
            .await?
            .unwrap();
        assert_eq!(bob_message.in_reply_to().unwrap().0, original_alice_mimi_id);

        Ok(())
    }

    /// Deleting a message with no replies should succeed without any side effects.
    #[sqlx::test]
    async fn test_handle_message_delete_without_replies(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        let chat = test_chat();
        chat.store(pool.write().await?).await?;

        let group_id = chat.group_id();
        let domain = "localhost".parse().unwrap();
        let alice = UserId::random(domain);

        // Alice sends a message
        let alice_message = ChatMessage::new_for_test(
            chat.id(),
            MessageId::random(),
            TimeStamp::now(),
            ContentMessage::new(
                alice.clone(),
                false,
                MimiContent::simple_markdown_message("Hello from Alice!".to_string(), [0; 16]),
                group_id,
            ),
        );
        alice_message.store(pool.write().await?).await?;

        // Alice deletes her message
        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;
        let alice_message = handle_message_edit(
            &mut txn,
            group_id,
            TimeStamp::now(),
            &alice,
            *alice_message.message().mimi_id().unwrap(),
            alice_message.null_part_content()?,
        )
        .await?;
        alice_message.update(&mut txn).await?;

        let alice_message = ChatMessage::load(&mut txn, alice_message.id())
            .await?
            .unwrap();
        assert_eq!(alice_message.status(), mimi_content::MessageStatus::Deleted);

        Ok(())
    }

    /// When multiple messages reply to the same message, deleting it should update all of their
    /// `in_reply_to` references.
    #[sqlx::test]
    async fn test_handle_message_delete_updates_multiple_replies(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        let chat = test_chat();
        chat.store(pool.write().await?).await?;

        let group_id = chat.group_id();
        let domain = "localhost".parse().unwrap();
        let alice = UserId::random(domain);
        let bob = UserId::random("localhost".parse().unwrap());
        let carol = UserId::random("localhost".parse().unwrap());

        // Alice sends a message
        let alice_message = ChatMessage::new_for_test(
            chat.id(),
            MessageId::random(),
            TimeStamp::now(),
            ContentMessage::new(
                alice.clone(),
                false,
                MimiContent::simple_markdown_message("Hello from Alice!".to_string(), [0; 16]),
                group_id,
            ),
        );
        alice_message.store(pool.write().await?).await?;

        // Bob replies to Alice's message
        let mut bob_mimi_content =
            MimiContent::simple_markdown_message("Reply from Bob!".to_string(), [1; 16]);
        bob_mimi_content.in_reply_to = alice_message
            .message()
            .mimi_id()
            .map(|mimi_id| ByteBuf::from(mimi_id.as_slice()));
        let bob_message = ChatMessage::new_for_test(
            chat.id(),
            MessageId::random(),
            TimeStamp::now(),
            ContentMessage::new(bob.clone(), false, bob_mimi_content, group_id),
        );
        bob_message.store(pool.write().await?).await?;

        // Carol also replies to Alice's message
        let mut carol_mimi_content =
            MimiContent::simple_markdown_message("Reply from Carol!".to_string(), [2; 16]);
        carol_mimi_content.in_reply_to = alice_message
            .message()
            .mimi_id()
            .map(|mimi_id| ByteBuf::from(mimi_id.as_slice()));
        let carol_message = ChatMessage::new_for_test(
            chat.id(),
            MessageId::random(),
            TimeStamp::now(),
            ContentMessage::new(carol.clone(), false, carol_mimi_content, group_id),
        );
        carol_message.store(pool.write().await?).await?;

        // Alice deletes her message
        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;
        let alice_message = handle_message_edit(
            &mut txn,
            group_id,
            TimeStamp::now(),
            &alice,
            *alice_message.message().mimi_id().unwrap(),
            alice_message.null_part_content()?,
        )
        .await?;
        alice_message.update(&mut txn).await?;

        // Both Bob's and Carol's in_reply_to should reference Alice's deleted MIMI ID
        let deleted_mimi_id = alice_message.message().mimi_id().unwrap();
        let bob_message = ChatMessage::load(&mut txn, bob_message.id())
            .await?
            .unwrap();
        let carol_message = ChatMessage::load(&mut txn, carol_message.id())
            .await?
            .unwrap();
        assert_eq!(&bob_message.in_reply_to().unwrap().0, deleted_mimi_id);
        assert_eq!(&carol_message.in_reply_to().unwrap().0, deleted_mimi_id);

        Ok(())
    }

    /// If a message is edited and then another user replies to the *edited* version, deleting the
    /// message should still update the reply's `in_reply_to` reference.
    #[sqlx::test]
    async fn test_handle_message_delete_updates_reply_to_edited_message(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        let chat = test_chat();
        chat.store(pool.write().await?).await?;

        let group_id = chat.group_id();
        let domain = "localhost".parse().unwrap();
        let alice = UserId::random(domain);
        let bob = UserId::random("localhost".parse().unwrap());

        // Alice sends a message
        let alice_message = ChatMessage::new_for_test(
            chat.id(),
            MessageId::random(),
            TimeStamp::now(),
            ContentMessage::new(
                alice.clone(),
                false,
                MimiContent::simple_markdown_message("Hello from Alice!".to_string(), [0; 16]),
                group_id,
            ),
        );
        alice_message.store(pool.write().await?).await?;

        // Alice edits her message — the MIMI ID changes
        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;
        let edited_alice_content = MimiContent::simple_markdown_message(
            "Hello from Alice! WITH EDIT".to_string(),
            [0; 16],
        );
        let alice_message = handle_message_edit(
            &mut txn,
            group_id,
            TimeStamp::now(),
            &alice,
            *alice_message.message().mimi_id().unwrap(),
            edited_alice_content,
        )
        .await?;
        alice_message.update(&mut txn).await?;
        txn.commit().await?;

        // Bob replies to the *edited* version of Alice's message
        let edited_alice_mimi_id = *alice_message.message().mimi_id().unwrap();
        let mut bob_mimi_content =
            MimiContent::simple_markdown_message("Reply to edited message!".to_string(), [1; 16]);
        bob_mimi_content.in_reply_to =
            Some(ByteBuf::from(edited_alice_mimi_id.as_slice().to_vec()));
        let bob_message = ChatMessage::new_for_test(
            chat.id(),
            MessageId::random(),
            TimeStamp::now(),
            ContentMessage::new(bob.clone(), false, bob_mimi_content, group_id),
        );
        bob_message.store(pool.write().await?).await?;

        // Alice deletes her (edited) message
        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;
        let alice_message = ChatMessage::load(&mut txn, alice_message.id())
            .await?
            .unwrap();
        let alice_message = handle_message_edit(
            &mut txn,
            group_id,
            TimeStamp::now(),
            &alice,
            *alice_message.message().mimi_id().unwrap(),
            alice_message.null_part_content()?,
        )
        .await?;
        alice_message.update(&mut txn).await?;

        // Bob's in_reply_to should reference Alice's deleted MIMI ID (not the edited one)
        let deleted_mimi_id = alice_message.message().mimi_id().unwrap();
        let bob_message = ChatMessage::load(&mut txn, bob_message.id())
            .await?
            .unwrap();
        assert_eq!(&bob_message.in_reply_to().unwrap().0, deleted_mimi_id);

        Ok(())
    }
}
