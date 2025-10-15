// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    codec::PersistenceCodec,
    credentials::ClientCredential,
    crypto::{ear::EarDecryptable, indexed_aead::keys::UserProfileKey},
    identifiers::{MimiId, QualifiedGroupId, UserHandle, UserId},
    messages::{
        QueueMessage,
        client_ds::{
            AadMessage, AadPayload, ExtractedQsQueueMessage, ExtractedQsQueueMessagePayload,
            UserProfileKeyUpdateParams, WelcomeBundle,
        },
    },
    time::TimeStamp,
};
use airprotos::queue_service::v1::{QueueEvent, queue_event};
use anyhow::{Context, Result, bail, ensure};
use mimi_content::{
    Disposition, MessageStatus, MessageStatusReport, MimiContent, NestedPartContent,
};
use mimi_room_policy::RoleIndex;
use openmls::{
    group::QueuedProposal,
    prelude::{
        ApplicationMessage, MlsMessageBodyIn, MlsMessageIn, ProcessedMessageContent, Proposal,
        ProtocolMessage, Sender,
    },
};
use sqlx::{Acquire, SqliteTransaction};
use tls_codec::DeserializeBytes;
use tracing::{debug, error, info, warn};

use crate::{
    ChatMessage, ChatStatus, ContentMessage, Message, SystemMessage,
    chats::{ChatType, StatusRecord, messages::edit::MessageEdit},
    clients::{
        QsListenResponder,
        block_contact::{BlockedContact, BlockedContactError},
    },
    contacts::HandleContact,
    groups::{Group, client_auth_info::StorableClientCredential, process::ProcessMessageResult},
    key_stores::{indexed_keys::StorableIndexedKey, queue_ratchets::StorableQsQueueRatchet},
    store::StoreNotifier,
};

use super::{
    Chat, ChatAttributes, ChatId, CoreUser, FriendshipPackage, TimestampedMessage, anyhow,
};

pub enum ProcessQsMessageResult {
    None,
    NewChat(ChatId),
    ChatChanged(ChatId, Vec<ChatMessage>),
    Messages(Vec<ChatMessage>),
}

#[derive(Debug, Default)]
pub struct ProcessedQsMessages {
    pub new_chats: Vec<ChatId>,
    pub changed_chats: Vec<ChatId>,
    pub new_messages: Vec<ChatMessage>,
    pub errors: Vec<anyhow::Error>,
    pub processed: usize,
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
    async fn process_qs_message(
        &self,
        qs_queue_message: ExtractedQsQueueMessage,
    ) -> Result<ProcessQsMessageResult> {
        // TODO: We should verify whether the messages are valid messages, i.e.
        // if it doesn't mix requests, etc. I think the DS already does some of this
        // and we might be able to re-use code.

        // Keep track of freshly joined groups s.t. we can later update our user auth keys.
        let ds_timestamp = qs_queue_message.timestamp;
        match qs_queue_message.payload {
            ExtractedQsQueueMessagePayload::WelcomeBundle(welcome_bundle) => {
                self.handle_welcome_bundle(welcome_bundle).await
            }
            ExtractedQsQueueMessagePayload::MlsMessage(mls_message) => {
                self.handle_mls_message(*mls_message, ds_timestamp).await
            }
            ExtractedQsQueueMessagePayload::UserProfileKeyUpdate(
                user_profile_key_update_params,
            ) => {
                self.handle_user_profile_key_update(user_profile_key_update_params)
                    .await
            }
        }
    }

    async fn handle_welcome_bundle(
        &self,
        welcome_bundle: WelcomeBundle,
    ) -> Result<ProcessQsMessageResult> {
        // WelcomeBundle Phase 1: Join the group. This might involve
        // loading AS credentials or fetching them from the AS.
        let (own_profile_key, own_profile_key_in_group, group, chat_id) = self
            .with_transaction_and_notifier(async |txn, notifier| {
                let (group, member_profile_info) = Group::join_group(
                    welcome_bundle,
                    &self.inner.key_store.wai_ear_key,
                    txn,
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
                    if profile_info.client_credential.identity() == self.user_id() {
                        // We already have our own profile info.
                        own_profile_key_in_group = Some(profile_info.user_profile_key);
                        continue;
                    }
                    self.fetch_and_store_user_profile(txn, notifier, profile_info)
                        .await?;
                }

                let Some(own_profile_key_in_group) = own_profile_key_in_group else {
                    bail!("No profile info for our user found");
                };

                // WelcomeBundle Phase 3: Store the user profiles of the group
                // members if they don't exist yet and store the group and the
                // new chat.

                // Set the chat attributes according to the group's
                // group data.
                let group_data = group.group_data().context("No group data")?;
                let attributes: ChatAttributes = PersistenceCodec::from_slice(group_data.bytes())?;

                let chat = Chat::new_group_chat(group_id.clone(), attributes);
                let own_profile_key = UserProfileKey::load_own(txn.as_mut()).await?;
                // If we've been in that chat before, we delete the old
                // chat (and the corresponding MLS group) first and then
                // create a new one. We do leave the messages intact, though.
                Chat::delete(txn.as_mut(), notifier, chat.id()).await?;
                Group::delete_from_db(txn, &group_id).await?;
                group.store(txn.as_mut()).await?;
                chat.store(txn.as_mut(), notifier).await?;

                Ok((own_profile_key, own_profile_key_in_group, group, chat.id()))
            })
            .await?;

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

        Ok(ProcessQsMessageResult::NewChat(chat_id))
    }

    async fn handle_mls_message(
        &self,
        mls_message: MlsMessageIn,
        ds_timestamp: TimeStamp,
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

        let (messages, chat_changed, chat_id, profile_infos) = self
            .with_transaction_and_notifier(async |txn, notifier| {
                let chat = Chat::load_by_group_id(txn.as_mut(), &group_id)
                    .await?
                    .ok_or_else(|| anyhow!("No chat found for group ID {:?}", group_id))?;
                let chat_id = chat.id();

                let mut group = Group::load_clean(txn, &group_id)
                    .await?
                    .ok_or_else(|| anyhow!("No group found for group ID {:?}", group_id))?;

                // MLSMessage Phase 2: Process the message
                let ProcessMessageResult {
                    processed_message,
                    we_were_removed,
                    sender_client_credential,
                    profile_infos,
                } = group
                    .process_message(txn, &self.inner.api_clients, protocol_message)
                    .await?;

                let sender = processed_message.sender().clone();
                let aad = processed_message.aad().to_vec();

                // `chat_changed` indicates whether the state of the chat was updated
                let (new_messages, updated_messages, chat_changed) =
                    match processed_message.into_content() {
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
                                    txn,
                                    notifier,
                                    &group,
                                    application_message,
                                    ds_timestamp,
                                    sender_client_credential.identity(),
                                )
                                .await?;
                            (new_messages, updated_messages, chat_changed)
                        }
                        ProcessedMessageContent::ProposalMessage(proposal) => {
                            let (new_messages, updated) = self
                                .handle_proposal_message(txn, &mut group, *proposal, ds_timestamp)
                                .await?;
                            (new_messages, Vec::new(), updated)
                        }
                        ProcessedMessageContent::StagedCommitMessage(staged_commit) => {
                            let (new_messages, updated) = self
                                .handle_staged_commit_message(
                                    txn,
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
                            (new_messages, Vec::new(), updated)
                        }
                        ProcessedMessageContent::ExternalJoinProposalMessage(_) => {
                            let (new_messages, updated) =
                                self.handle_external_join_proposal_message()?;
                            (new_messages, Vec::new(), updated)
                        }
                    };

                // MLSMessage Phase 3: Store the updated group and the messages.
                group.store_update(txn.as_mut()).await?;

                let mut messages =
                    Self::store_new_messages(txn, notifier, chat_id, new_messages).await?;
                for updated_message in updated_messages {
                    updated_message.update(txn.as_mut(), notifier).await?;
                    messages.push(updated_message);
                }

                Ok((messages, chat_changed, chat_id, profile_infos))
            })
            .await?;

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
        self.schedule_receipts(chat_id, delivery_receipts).await?;

        let res = match (messages, chat_changed) {
            (messages, true) => ProcessQsMessageResult::ChatChanged(chat_id, messages),
            (messages, false) => ProcessQsMessageResult::Messages(messages),
        };

        // MLSMessage Phase 4: Fetch user profiles of new clients and store them.
        self.with_transaction_and_notifier(async |txn, notifier| -> anyhow::Result<_> {
            for client in profile_infos {
                self.fetch_and_store_user_profile(&mut *txn, notifier, client)
                    .await?;
            }
            Ok(())
        })
        .await?;

        Ok(res)
    }

    /// Returns a message if it should be stored, otherwise an empty vec.
    ///
    /// Also returns whether the chat should be notified as updated.
    async fn handle_application_message(
        &self,
        txn: &mut SqliteTransaction<'_>,
        notifier: &mut StoreNotifier,
        group: &Group,
        application_message: ApplicationMessage,
        ds_timestamp: TimeStamp,
        sender: &UserId,
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
            let report = MessageStatusReport::deserialize(report_content)?;
            StatusRecord::borrowed(sender, report, ds_timestamp)
                .store_report(txn, notifier)
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
                notifier,
                group,
                ds_timestamp,
                sender,
                mimi_id,
                std::mem::take(content),
            )
            .await
            .inspect_err(|error| {
                error!(%error, "Failed to handle message edit; skipping");
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

    async fn handle_proposal_message(
        &self,
        txn: &mut SqliteTransaction<'_>,
        group: &mut Group,
        proposal: QueuedProposal,
        ds_timestamp: TimeStamp,
    ) -> anyhow::Result<(Vec<TimestampedMessage>, bool)> {
        let mut messages = Vec::new();

        if let Proposal::Remove(remove_proposal) = proposal.proposal() {
            let Some(removed) = group.client_by_index(txn, remove_proposal.removed()).await else {
                warn!("removed client not found");
                return Ok((vec![], false));
            };

            // TODO: Handle external sender for when the server wants to kick a user?
            let Sender::Member(sender) = proposal.sender() else {
                return Ok((vec![], false));
            };

            let Some(sender) = group.client_by_index(txn, *sender).await else {
                warn!("sending client not found");
                return Ok((vec![], false));
            };

            ensure!(
                sender == removed,
                "A user should not send remove proposals for other users"
            );

            group.room_state_change_role(&sender, &sender, RoleIndex::Outsider)?;

            messages.push(TimestampedMessage::system_message(
                SystemMessage::Remove(sender, removed),
                ds_timestamp,
            ));
        }

        // For now, we don't to anything here. The proposal
        // was processed by the MLS group and will be
        // committed with the next commit.
        group.store_proposal(txn.as_mut(), proposal)?;

        Ok((messages, false))
    }

    #[expect(clippy::too_many_arguments)]
    async fn handle_staged_commit_message(
        &self,
        txn: &mut SqliteTransaction<'_>,
        group: &mut Group,
        mut chat: Chat,
        staged_commit: openmls::prelude::StagedCommit,
        aad: Vec<u8>,
        ds_timestamp: TimeStamp,
        sender: &openmls::prelude::Sender,
        sender_client_credential: &ClientCredential,
        we_were_removed: bool,
    ) -> anyhow::Result<(Vec<TimestampedMessage>, bool)> {
        // If a client joined externally, we check if the
        // group belongs to an unconfirmed chat.

        // StagedCommitMessage Phase 1: Confirm the chat if unconfirmed
        let mut notifier = self.store_notifier();

        let chat_changed = match &chat.chat_type() {
            ChatType::HandleConnection(handle) => {
                let handle = handle.clone();
                self.handle_unconfirmed_chat(
                    txn,
                    &mut notifier,
                    aad,
                    sender,
                    sender_client_credential,
                    &mut chat,
                    &handle,
                    group,
                )
                .await?;
                true
            }
            _ => false,
        };

        // StagedCommitMessage Phase 2: Merge the staged commit into the group.

        // If we were removed, we set the group to inactive.
        if we_were_removed {
            let past_members = group.members(txn.as_mut()).await.into_iter().collect();
            chat.set_inactive(txn.as_mut(), &mut notifier, past_members)
                .await?;
        }
        let group_messages = group
            .merge_pending_commit(txn, staged_commit, ds_timestamp)
            .await?;

        notifier.notify();

        Ok((group_messages, chat_changed))
    }

    #[expect(clippy::too_many_arguments)]
    async fn handle_unconfirmed_chat(
        &self,
        txn: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        notifier: &mut StoreNotifier,
        aad: Vec<u8>,
        sender: &Sender,
        sender_client_credential: &ClientCredential,
        chat: &mut Chat,
        handle: &UserHandle,
        group: &mut Group,
    ) -> Result<(), anyhow::Error> {
        // Check if it was an external commit
        ensure!(
            matches!(sender, Sender::NewMemberCommit),
            "Incoming commit to ConnectionGroup was not an external commit"
        );
        let user_id = sender_client_credential.identity();

        // UnconfirmedConnection Phase 1: Load up the partial contact and decrypt the
        // friendship package
        let contact = HandleContact::load(txn.as_mut(), handle)
            .await?
            .with_context(|| format!("No contact found with handle: {}", handle.plaintext()))?;

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
            &contact.friendship_package_ear_key,
            &encrypted_friendship_package,
        )?;

        let user_profile_key = UserProfileKey::from_base_secret(
            friendship_package.user_profile_base_secret.clone(),
            user_id,
        )?;

        // UnconfirmedConnection Phase 2: Fetch the user profile.
        self.fetch_and_store_user_profile(
            txn,
            notifier,
            (sender_client_credential.clone(), user_profile_key),
        )
        .await?;

        // Now we can turn the partial contact into a full one.
        let contact = contact
            .mark_as_complete(txn, notifier, user_id.clone(), friendship_package)
            .await?;

        // Room state update: Pretend that we just invited that user
        // We do that now, because we didn't know that user id when we created the room.
        group.room_state_change_role(self.user_id(), user_id, RoleIndex::Regular)?;

        chat.confirm(txn.as_mut(), notifier, contact.user_id)
            .await?;

        Ok(())
    }

    async fn handle_user_profile_key_update(
        &self,
        params: UserProfileKeyUpdateParams,
    ) -> anyhow::Result<ProcessQsMessageResult> {
        let mut connection = self.pool().acquire().await?;

        // Phase 1: Load the group and the sender.
        let group = Group::load(&mut connection, &params.group_id)
            .await?
            .context("No group found")?;
        let sender = group
            .client_by_index(&mut connection, params.sender_index)
            .await
            .context("No sender found")?;
        let sender_credential =
            StorableClientCredential::load_by_user_id(&mut *connection, &sender)
                .await?
                .context("No sender credential found")?;

        let chat_id = ChatId::try_from(group.group_id())?;
        if BlockedContact::check_blocked_chat(&mut *connection, chat_id).await? {
            bail!(BlockedContactError);
        }

        // Phase 2: Decrypt the new user profile key
        let new_user_profile_key = UserProfileKey::decrypt(
            group.identity_link_wrapper_key(),
            &params.user_profile_key,
            &sender,
        )?;

        // Phase 3: Fetch and store the (new) user profile and key
        self.with_notifier(async |notifier| {
            self.fetch_and_store_user_profile(
                &mut connection,
                notifier,
                (sender_credential.into(), new_user_profile_key),
            )
            .await
        })
        .await?;

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

        // Process each qs message individually
        for (idx, qs_message) in qs_messages.into_iter().enumerate() {
            let qs_message_payload =
                match StorableQsQueueRatchet::decrypt_qs_queue_message(self.pool(), qs_message)
                    .await
                {
                    Ok(plaintext) => plaintext,
                    Err(error) => {
                        error!(%error, "Decrypting message failed");
                        result.processed = idx;
                        return result;
                    }
                };
            let qs_message_plaintext = match qs_message_payload.extract() {
                Ok(extracted) => extracted,
                Err(error) => {
                    error!(%error, "Extracting message failed; dropping message");
                    continue;
                }
            };

            let processed = match self.process_qs_message(qs_message_plaintext).await {
                Ok(processed) => processed,
                Err(e) if e.downcast_ref::<BlockedContactError>().is_some() => {
                    info!("Dropping message from blocked contact");
                    continue;
                }
                Err(e) => {
                    error!(error = %e, "Processing message failed");
                    result.errors.push(e);
                    continue;
                }
            };

            match processed {
                ProcessQsMessageResult::Messages(messages) => {
                    result.new_messages.extend(messages);
                }
                ProcessQsMessageResult::ChatChanged(chat_id, messages) => {
                    result.new_messages.extend(messages);
                    result.changed_chats.push(chat_id)
                }
                ProcessQsMessageResult::NewChat(chat_id) => result.new_chats.push(chat_id),
                ProcessQsMessageResult::None => {}
            };
        }

        result.processed = num_messages;
        result
    }
}

async fn handle_message_edit(
    txn: &mut SqliteTransaction<'_>,
    notifier: &mut StoreNotifier,
    group: &Group,
    ds_timestamp: TimeStamp,
    sender: &UserId,
    replaces: MimiId,
    content: MimiContent,
) -> anyhow::Result<ChatMessage> {
    let is_delete = content.nested_part.part == NestedPartContent::NullPart;

    // First try to directly load the original message by mimi id (non-edited message) and fallback
    // to the history of edits otherwise.
    let mut message = match ChatMessage::load_by_mimi_id(txn.as_mut(), &replaces).await? {
        Some(message) => message,
        None => {
            let message_id = MessageEdit::find_message_id(txn.as_mut(), &replaces)
                .await?
                .with_context(|| {
                    format!("Original message id not found for editing; mimi_id = {replaces:?}")
                })?;

            ChatMessage::load(txn.as_mut(), message_id)
                .await?
                .with_context(|| {
                    format!("Original message not found for editing; message_id = {message_id:?}")
                })?
        }
    };

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

    if !is_delete {
        // Store message edit
        MessageEdit::new(
            original_mimi_id,
            message.id(),
            ds_timestamp,
            original_mimi_content,
        )
        .store(txn.as_mut())
        .await?;
    }

    // Update the original message
    let is_sent = true;
    message.set_content_message(ContentMessage::new(
        original_sender.clone(),
        is_sent,
        content,
        group.group_id(),
    ));
    message.set_edited_at(ds_timestamp);
    message.set_status(MessageStatus::Unread);

    // Clear the status of the message
    StatusRecord::clear(txn.as_mut(), notifier, message.id()).await?;

    Chat::mark_as_unread(txn, notifier, message.chat_id(), message.id()).await?;

    Ok(message)
}

/// A processor for the streamed QS events.
///
/// This processor is meant to be used in the streaming context where the events are streamed one
/// by one and this process never finishes until the stream is closed. Each event is processed by
/// `[Self::process_event]`.
#[derive(Debug)]
pub struct QsStreamProcessor {
    core_user: CoreUser,
    responder: Option<QsListenResponder>,
    /// Accumulated but not yet processed messages
    ///
    /// Note: It is safe to keep messages in memory here, because they are not yet decrypted.
    /// Decryption increases the locally stored ratchet sequence number, which is used to determine
    /// which messages should be fetched from the server. In case, the app is shut down, the
    /// messages will be received again.
    messages: Vec<QueueMessage>,
}

pub trait QsNotificationProcessor {
    fn show_notifications(
        &mut self,
        messages: ProcessedQsMessages,
    ) -> impl Future<Output = ()> + Send;
}

impl QsStreamProcessor {
    pub fn new(core_user: CoreUser) -> Self {
        Self {
            core_user,
            responder: None,
            messages: Vec::new(),
        }
    }

    pub fn with_responder(core_user: CoreUser, responder: QsListenResponder) -> Self {
        Self {
            core_user,
            responder: Some(responder),
            messages: Vec::new(),
        }
    }

    pub fn replace_responder(&mut self, responder: QsListenResponder) {
        self.responder.replace(responder);
    }

    pub async fn process_event(
        &mut self,
        event: QueueEvent,
        notification_processor: &mut impl QsNotificationProcessor,
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

                let processed_messages = self.core_user.fully_process_qs_messages(messages).await;

                let result = if processed_messages.processed < num_messages {
                    error!(
                        processed_messages.processed,
                        num_messages, "failed to fully process messages"
                    );
                    QsProcessEventResult::PartiallyProcessed {
                        processed: processed_messages.processed,
                        dropped: num_messages - processed_messages.processed,
                    }
                } else {
                    QsProcessEventResult::FullyProcessed {
                        processed: processed_messages.processed,
                    }
                };

                notification_processor
                    .show_notifications(processed_messages)
                    .await;

                if let Some(max_sequence_number) = max_sequence_number {
                    // We received some messages, so we can ack them *after* they were fully
                    // processed. In particular, the queue ratchet sequence number has been already
                    // written back into the database.
                    if let Some(responder) = self.responder.as_ref() {
                        responder
                            .ack(max_sequence_number + 1)
                            .await
                            .inspect_err(|error| {
                                error!(%error, "failed to ack QS messages");
                            })
                            .ok();
                    } else {
                        error!("logic error: no responder to ack QS messages");
                    }
                }

                // Scheduled tasks to execute after the queue was fully processed
                if let Err(error) = self.core_user.send_scheduled_receipts().await {
                    error!(%error, "Failed to send scheduled receipts");
                }

                result
            }
        }
    }
}

pub enum QsProcessEventResult {
    /// Event was accumulated to be processed later
    Accumulated,
    /// Event was ignored
    Ignored,
    /// All accumulated events where fully processed
    FullyProcessed { processed: usize },
    /// Accumulated events were partially processed, some events were dropped
    PartiallyProcessed { processed: usize, dropped: usize },
}

impl QsProcessEventResult {
    pub fn processed(&self) -> usize {
        match self {
            Self::Accumulated => 0,
            Self::Ignored => 0,
            Self::FullyProcessed { processed } => *processed,
            Self::PartiallyProcessed { processed, .. } => *processed,
        }
    }

    pub fn is_partially_processed(&self) -> bool {
        matches!(self, Self::PartiallyProcessed { .. })
    }
}
