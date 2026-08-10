// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::Instant;

use aircommon::{
    credentials::{LeafCredential, UserCredential},
    crypto::{aead::AeadDecryptable, indexed_aead::keys::UserProfileKey},
    identifiers::{MimiId, QualifiedGroupId, UserId},
    messages::{
        QueueMessage,
        client_ds::{
            AadMessage, AadPayload, ApqWelcomeBundle, DsCommitResponse, ExtractedQsQueueMessage,
            ExtractedQsQueueMessagePayload, QsQueueTargetedMessage, UserProfileKeyUpdateParams,
            WelcomeBundle,
        },
    },
    time::TimeStamp,
    utils::removed_client,
    virtual_client::KeyPackageBatchId,
};
use airprotos::client::group::GroupData;
use anyhow::{Context, Result, bail, ensure};
use apqmls::messages::ApqMlsMessageIn;
use chrono::Utc;
use mimi_content::{Disposition, MessageStatus, MessageStatusReport, MimiContent, NestedPart};
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
    ChatAttributes, ChatMessage, ChatStatus, Message, SystemMessage,
    chats::{
        GroupDataExt, GroupDataProfilePart, StatusRecord,
        messages::edit::{MessageEdit, handle_message_edit},
        reactions::Reaction,
    },
    clients::{
        block_contact::{BlockedContact, BlockedContactError},
        own_client_info::OwnClientInfo,
        process::process_as::{ConnectionInfoSource, TargetedMessageSource},
        targeted_message::TargetedMessageContent,
        update_key::{update_chat_attributes, update_chat_title},
        user_settings::ReadReceiptsSetting,
    },
    contacts::{PartialContact, PartialContactType},
    db::access::{WriteConnection, WriteDbTransaction},
    groups::{
        DecryptedProfileInfos, Group, GroupDataBytes, JoinSigners, VerifiedGroup,
        client_auth_info::StorableUserCredential,
        process::{ProcessMessageProcessed, ProcessMessageResult},
    },
    job::{JobContext, JobContextDb, pending_chat_operation::PendingChatOperation},
    key_stores::{indexed_keys::StorableIndexedKey, queue_ratchets::StorableQsQueueRatchet},
    outbound_service::resync::Resync,
};

use super::{Chat, ChatId, CoreUser, FriendshipPackage, TimestampedMessage, anyhow};

/// The outcome of processing a single QS message.
///
/// Folded into [`ProcessedQsMessages`].
#[derive(Default)]
pub struct QsMessageOutcome {
    new_chat: Option<NewChat>,
    new_connection: Option<ChatId>,
    new_messages: Vec<ChatMessage>,
    reaction_notifications: Vec<ReactionNotification>,
    changed_chats: Vec<ChatId>,
}

impl QsMessageOutcome {
    fn empty() -> QsMessageOutcome {
        Self::default()
    }

    fn new_chat(chat_id: ChatId, added_by: UserId, messages: Vec<ChatMessage>) -> QsMessageOutcome {
        Self {
            new_chat: Some(NewChat { chat_id, added_by }),
            new_messages: messages,
            ..Self::empty()
        }
    }

    fn new_connection(chat_id: ChatId) -> QsMessageOutcome {
        Self {
            new_connection: Some(chat_id),
            ..Self::empty()
        }
    }

    fn messages(
        messages: Vec<ChatMessage>,
        reaction_notifications: Vec<ReactionNotification>,
        changed_chats: Vec<ChatId>,
    ) -> QsMessageOutcome {
        Self {
            new_messages: messages,
            reaction_notifications,
            changed_chats,
            ..Self::empty()
        }
    }
}

#[derive(Debug)]
pub struct NewChat {
    pub chat_id: ChatId,
    pub added_by: UserId,
}

#[derive(Debug, Default)]
pub struct ProcessedQsMessages {
    pub new_chats: Vec<NewChat>,
    pub new_messages: Vec<ChatMessage>,
    pub errors: Vec<anyhow::Error>,
    pub processed: usize,
    pub new_connections: Vec<ChatId>,
    /// Reactions on our own messages, for which we should notify the user.
    pub reaction_notifications: Vec<ReactionNotification>,
    // Chats whose local notification content changed without necessarily producing a notification.
    //
    // For example, a message edit, remote delete, or a reaction retraction on one of our own
    // messages is such a change.
    pub chats_with_changed_notifications: Vec<ChatId>,
}

/// A reaction by another user on a message we sent.
///
/// Only reactions on our own messages produce these (and only additions, not
/// retractions); the consumer turns them into user-facing notifications.
#[derive(Debug, Clone)]
pub struct ReactionNotification {
    pub chat_id: ChatId,
    pub reactor: UserId,
    pub emoji: String,
    pub original_chat_message: ChatMessage,
}

impl ProcessedQsMessages {
    pub fn is_empty(&self) -> bool {
        self.new_chats.is_empty()
            && self.new_messages.is_empty()
            && self.errors.is_empty()
            && self.new_connections.is_empty()
            && self.reaction_notifications.is_empty()
            && self.chats_with_changed_notifications.is_empty()
    }

    fn merge(
        &mut self,
        QsMessageOutcome {
            new_chat,
            new_connection,
            new_messages,
            reaction_notifications,
            changed_chats,
        }: QsMessageOutcome,
    ) {
        self.new_chats.extend(new_chat);
        self.new_connections.extend(new_connection);
        self.new_messages.extend(new_messages);
        self.reaction_notifications.extend(reaction_notifications);
        self.chats_with_changed_notifications.extend(changed_chats);
    }
}

/// Messages produced by handling a single QS message.
#[derive(Default)]
struct HandledMessages {
    new_messages: Vec<TimestampedMessage>,
    updated_messages: Vec<ChatMessage>,
    reaction_notifications: Vec<ReactionNotification>,
    changed_chats: Vec<ChatId>,
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
    ) -> Result<QsMessageOutcome> {
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
            ExtractedQsQueueMessagePayload::ApqWelcomeBundle(welcome_bundle) => {
                self.handle_apq_welcome_bundle(txn, welcome_bundle, ds_timestamp)
                    .await
            }
            ExtractedQsQueueMessagePayload::MlsMessage(mls_message) => {
                Box::pin(self.handle_mls_message(
                    txn,
                    *mls_message,
                    ds_timestamp,
                    read_receipts_enabled,
                ))
                .await
            }
            ExtractedQsQueueMessagePayload::ApqMlsMessage(apq_mls_message) => {
                Box::pin(self.handle_apq_mls_message(
                    txn,
                    *apq_mls_message,
                    ds_timestamp,
                    read_receipts_enabled,
                ))
                .await
            }
            ExtractedQsQueueMessagePayload::UserProfileKeyUpdate(
                user_profile_key_update_params,
            ) => self
                .handle_user_profile_key_update(txn, user_profile_key_update_params)
                .await
                .map(|_| QsMessageOutcome::empty()),
            ExtractedQsQueueMessagePayload::TargetedMessage(
                QsQueueTargetedMessage::ApplicationMessage(mls_message_bytes),
            ) => {
                let mls_message = MlsMessageIn::tls_deserialize_exact_bytes(&mls_message_bytes)
                    .context("Failed to deserialize targeted MLS message")?;
                Box::pin(self.handle_targeted_application_message(txn, mls_message, ds_timestamp))
                    .await
            }
            ExtractedQsQueueMessagePayload::DsCommitResponse(ds_commit_response) => self
                .handle_commit_response(txn, ds_commit_response)
                .await
                .map(|_| QsMessageOutcome::empty()),
        };

        debug!(elapsed = ?started.elapsed(), "Processed QS message");
        res
    }

    async fn handle_commit_response(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        commit_response: DsCommitResponse,
    ) -> anyhow::Result<()> {
        let DsCommitResponse {
            group_id,
            epoch,
            timestamp,
            key_package_batch,
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
            return Ok(());
        }

        // If yes, merge the commit and store the updated group
        let (mut group_messages, group_data_bytes) =
            group.merge_pending_commit(txn, None, timestamp).await?;
        group
            .group_mut()
            .store_update(&mut *txn, Some(timestamp), None)
            .await?;

        let mut chat = Chat::load_by_group_id(&mut *txn, &group_id)
            .await?
            .context("Can't find chat for commit response")?;

        let pq_updated_at = group.is_apq().then_some(timestamp);
        group
            .group_mut()
            .store_update(&mut *txn, Some(timestamp), pq_updated_at)
            .await?;

        self.finalize_own_commit(
            txn,
            &group,
            &mut chat,
            group_data_bytes,
            &mut group_messages,
            key_package_batch,
            timestamp,
        )
        .await?;

        CoreUser::store_new_messages(&mut *txn, chat.id(), group_messages).await?;

        Ok(())
    }

    /// Applies the side effects of merging one of our own commits: any group
    /// data change carried by the commit (currently the chat title) is applied
    /// to `chat`, appending the corresponding system messages to
    /// `group_messages`, and the pending chat operation is deleted.
    ///
    /// Shared by the `DsCommitResponse` and `OwnPendingCommit` paths, which
    /// race to merge our own commit: the DS both responds directly and echoes
    /// the commit back via fanout, so whichever arrives first must run these
    /// side effects (the loser bails early on the now-stale epoch). The caller
    /// is responsible for storing `group_messages`.
    #[expect(clippy::too_many_arguments)]
    async fn finalize_own_commit(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        group: &Group,
        chat: &mut Chat,
        group_data_bytes: Option<GroupDataBytes>,
        group_messages: &mut Vec<TimestampedMessage>,
        key_package_batch: Option<KeyPackageBatchId>,
        ds_timestamp: TimeStamp,
    ) -> anyhow::Result<()> {
        // Update group data in chat attributes if present
        if let Some(group_data_bytes) = group_data_bytes
            && let Some(title) =
                GroupData::decode_title(&group_data_bytes, group.identity_link_wrapper_key())?
        {
            update_chat_title(
                &mut *txn,
                chat,
                self.user_id(),
                title,
                ds_timestamp,
                group_messages,
            )
            .await?;
        }

        // Our settings commit was accepted: complete the pending setting
        // changes it asserted before the operation is deleted. Only self-group
        // commits can carry a settings update. Boxed because the loaded
        // operation carries the full group state.
        if group.is_self_group() {
            Box::pin(PendingChatOperation::complete_settings_intent(
                &mut *txn,
                group.group_id(),
            ))
            .await?;
        }

        // Complete the pending chat operation
        PendingChatOperation::complete_own_commit(
            txn,
            group.group_id(),
            key_package_batch.as_ref(),
        )
        .await?;

        Ok(())
    }

    async fn handle_welcome_bundle(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        welcome_bundle: WelcomeBundle,
        ds_timestamp: TimeStamp,
    ) -> Result<QsMessageOutcome> {
        // WelcomeBundle Phase 1: Join the group. This might involve loading AS credentials or
        // fetching them from the AS.
        let (group, sender_user_id, member_profile_info) = Box::pin(Group::join_group(
            welcome_bundle,
            &self.inner.key_store.wai_ear_key,
            &mut *txn,
            &self.inner.api_clients,
            self.signing_key(),
        ))
        .await?;
        self.finalize_welcome(
            txn,
            ds_timestamp,
            group,
            sender_user_id,
            member_profile_info,
        )
        .await
    }

    async fn handle_apq_welcome_bundle(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        welcome_bundle: ApqWelcomeBundle,
        ds_timestamp: TimeStamp,
    ) -> anyhow::Result<QsMessageOutcome> {
        // WelcomeBundle Phase 1: Join the group. This might involve loading AS credentials or
        // fetching them from the AS.
        let own_client_info = OwnClientInfo::load(&mut *txn).await?;
        let signers = JoinSigners {
            client: self.signing_key(),
            self_group: own_client_info.self_group_signing_key.as_ref(),
        };
        let (mut group, sender_user_id, member_profile_info) = Box::pin(Group::join_apq_group(
            welcome_bundle,
            &self.inner.key_store.wai_ear_key,
            txn,
            &self.inner.api_clients,
            signers,
        ))
        .await?;

        if own_client_info.self_group_id.as_ref() == Some(group.group_id()) {
            debug!("joined self group as a linked device");
            let group_data_bytes = group.group_data().context("self group has no group data")?;
            let title =
                GroupData::decode_title(&group_data_bytes, group.identity_link_wrapper_key())?;
            let attributes = ChatAttributes {
                title: title.context("self group has no title")?,
                picture: None,
            };
            let chat = Chat::new_group_chat(group.group_id().clone(), attributes);
            chat.store(&mut *txn).await?;

            // Register the emulation epoch at the epoch we joined into. The
            // sibling that added us registers at the same epoch when it merges
            // its Add commit, so both derive the same `EpochId`. Joining does
            // not go through `Group::merge_pending_commit`, which covers every
            // later epoch of the self group.
            let epoch_id = group.register_vc_emulation_epoch(&mut *txn)?;
            debug!(
                ?epoch_id,
                "registered self-group VC emulation epoch on join"
            );

            return Ok(QsMessageOutcome::new_chat(
                chat.id(),
                sender_user_id,
                vec![],
            ));
        }

        self.finalize_welcome(
            txn,
            ds_timestamp,
            group,
            sender_user_id,
            member_profile_info,
        )
        .await
    }

    async fn finalize_welcome(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        ds_timestamp: TimeStamp,
        group: Group,
        sender_user_id: UserId,
        member_profile_info: DecryptedProfileInfos,
    ) -> anyhow::Result<QsMessageOutcome> {
        let group_id = group.group_id().clone();

        // WelcomeBundle Phase 2: Fetch the user profiles of the group members
        // and decrypt them.

        // TODO: This can fail in some cases. If it does, we should fetch and
        // process messages and then try again.
        for profile_info in member_profile_info.members {
            Self::schedule_fetch_user_profile(&mut *txn, profile_info).await?;
        }

        // WelcomeBundle Phase 3: Store the user profiles of the group
        // members if they don't exist yet and store the group and the
        // new chat.

        // Set the chat attributes according to the group's
        // group data.
        let group_data_bytes = group.group_data().context("No group data")?;
        let group_data = GroupData::decode(&group_data_bytes)?;
        let (title, group_profile_part) = group_data.into_parts(group.identity_link_wrapper_key());
        let title = title.context("No group title")?;
        // An external group profile is not yet available; it is fetched later.
        let picture = Self::resolve_group_profile_part(
            txn,
            &group_id,
            &sender_user_id,
            ds_timestamp,
            group_profile_part,
            true,
        )
        .await?;
        let attributes = ChatAttributes { title, picture };

        let chat = Chat::new_group_chat(group_id.clone(), attributes);
        let own_profile_key = UserProfileKey::load_own(&mut *txn).await?;
        // If we've been in that chat before, we delete the old chat
        // first and then create a new one. We do leave the messages
        // intact, though.
        chat.store(&mut *txn).await?;

        // Add system message who added us to the group.
        // TODO(gabriel): here, we could inform _other_ users that a new client has been linked.
        let system_message = ChatMessage::new_system_message(
            chat.id(),
            ds_timestamp,
            SystemMessage::Add(sender_user_id.clone(), self.user_id().clone()),
        );
        system_message.store(&mut *txn).await?;

        // WelcomeBundle Phase 4: Check whether our user profile key is up to
        // date and if not, update it.
        if member_profile_info.own_profile_key.as_ref() != Some(&own_profile_key) {
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

        Ok(QsMessageOutcome::new_chat(
            chat.id(),
            sender_user_id,
            vec![system_message],
        ))
    }

    /// Handles the profile part of decoded group data: schedules a fetch for
    /// an external group profile, or returns the picture for the legacy
    /// variant.
    pub(crate) async fn resolve_group_profile_part(
        txn: &mut WriteDbTransaction<'_>,
        group_id: &GroupId,
        sender_id: &UserId,
        ds_timestamp: TimeStamp,
        group_profile_part: Option<GroupDataProfilePart>,
        is_initial_fetch: bool,
    ) -> sqlx::Result<Option<Vec<u8>>> {
        match group_profile_part {
            Some(GroupDataProfilePart::ExternalProfile(external_group_profile)) => {
                Self::schedule_fetch_group_profile(
                    &mut *txn,
                    group_id.clone(),
                    sender_id.clone(),
                    ds_timestamp,
                    external_group_profile,
                    is_initial_fetch,
                )
                .await?;
                Ok(None)
            }
            Some(GroupDataProfilePart::LegacyPicture(picture)) => Ok(Some(picture)),
            None => Ok(None),
        }
    }

    /// Loads the chat and the verified group for the given group id.
    async fn load_chat_and_group(
        txn: &mut WriteDbTransaction<'_>,
        group_id: &GroupId,
    ) -> Result<(Chat, VerifiedGroup)> {
        let chat = Chat::load_by_group_id(&mut *txn, group_id)
            .await?
            .ok_or_else(|| anyhow!("No chat found for group ID {group_id:?}"))?;
        let group = Group::load_verified(&mut *txn, group_id)
            .await?
            .ok_or_else(|| anyhow!("No group found for group ID {group_id:?}"))?;
        Ok((chat, group))
    }

    /// Unwraps the result of processing an MLS message.
    ///
    /// Returns `None` if the message was ignored or requires a resync, in which case the pending
    /// commit is marked as failed.
    async fn take_processed(
        txn: &mut WriteDbTransaction<'_>,
        chat_id: ChatId,
        group: &mut VerifiedGroup,
        result: ProcessMessageResult,
    ) -> Result<Option<ProcessMessageProcessed>> {
        match result {
            ProcessMessageResult::Processed(processed) => Ok(Some(processed)),
            ProcessMessageResult::Ignored => Ok(None),
            ProcessMessageResult::ResyncRequired => {
                // TODO: Once we have a UX for resyncs, we should schedule one
                // here and re-enable the resync test in integration.rs
                let _resync = Resync {
                    chat_id: Some(chat_id),
                    group_id: group.group_id().clone(),
                    pq_group_id: group.pq_group_id(),
                    group_state_ear_key: group.group_state_ear_key().clone(),
                    identity_link_wrapper_key: group.identity_link_wrapper_key().clone(),
                    original_leaf_index: group.own_index(),
                    shares_vc_leaf: group.own_leaf_is_virtual_client(),
                    connection_contact: None,
                };
                group.group_mut().mark_commit_failed(&mut *txn).await?;
                Ok(None)
            }
        }
    }

    async fn handle_targeted_application_message<'a>(
        &'a self,
        txn: &'a mut WriteDbTransaction<'_>,
        mls_message: MlsMessageIn,
        ds_timestamp: TimeStamp,
    ) -> Result<QsMessageOutcome> {
        let MlsMessageBodyIn::PrivateMessage(app_msg) = mls_message.extract() else {
            bail!("Unexpected message type")
        };
        let protocol_message = ProtocolMessage::from(app_msg);

        // MLSMessage Phase 1: Load the chat and the group.
        let group_id = protocol_message.group_id().clone();
        let (chat, mut group) = Self::load_chat_and_group(txn, &group_id).await?;

        // MLSMessage Phase 2: Process the message
        let result = group
            .group_mut()
            .process_message(&mut *txn, &self.inner.api_clients, protocol_message)
            .await?;
        let Some(ProcessMessageProcessed {
            processed_message, ..
        }) = Self::take_processed(txn, chat.id(), &mut group, result).await?
        else {
            return Ok(QsMessageOutcome::empty());
        };

        let Sender::Member(sender_index) = processed_message.sender() else {
            bail!("Sender is not a member");
        };
        let sender_user_credential = group
            .credential_at(*sender_index)?
            .context("No sender user credential found")?;

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
                sender_user_credential,
                origin_chat_id: chat.id(),
                sent_at: ds_timestamp,
            }));

        let mut context = JobContext {
            api_clients: &self.inner.api_clients,
            http_client: &self.inner.http_client,
            db: JobContextDb::Transaction(txn),
            key_store: &self.inner.key_store,
            now: Utc::now(),
            qs_client_id: &self.inner.qs_client_id,
        };

        let chat_id =
            CoreUser::process_connection_offer(&mut context, connection_info_source).await?;

        Ok(QsMessageOutcome::new_connection(chat_id))
    }

    async fn handle_mls_message(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        mls_message: MlsMessageIn,
        ds_timestamp: TimeStamp,
        read_receipts_enabled: bool,
    ) -> Result<QsMessageOutcome> {
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
        //
        // The group is loaded regardless of whether it has a pending commit or
        // not.
        let group_id = protocol_message.group_id().clone();
        let (chat, mut group) = Self::load_chat_and_group(txn, &group_id).await?;

        // MLSMessage Phase 2: Process the message
        let result = group
            .group_mut()
            .process_message(&mut *txn, &self.inner.api_clients, protocol_message)
            .await?;
        let Some(processed_message) =
            Self::take_processed(txn, chat.id(), &mut group, result).await?
        else {
            return Ok(QsMessageOutcome::empty());
        };

        self.finalize_handle_message(
            txn,
            ds_timestamp,
            read_receipts_enabled,
            chat,
            group,
            processed_message,
        )
        .await
    }

    /// Test-only: drive an incoming MLS message (e.g. the DS echo of our own
    /// commit) through the same processing path the QS queue uses, so tests can
    /// exercise the `OwnPendingCommit` merge without a live server.
    #[cfg(any(test, feature = "test_utils"))]
    pub async fn process_incoming_mls_message(
        &self,
        mls_message_bytes: &[u8],
    ) -> Result<QsMessageOutcome> {
        let mls_message = MlsMessageIn::tls_deserialize_exact_bytes(mls_message_bytes)?;
        let ds_timestamp = TimeStamp::now();
        self.db()
            .with_write_transaction(async |txn| {
                Box::pin(self.handle_mls_message(txn, mls_message, ds_timestamp, false)).await
            })
            .await
    }

    async fn handle_apq_mls_message(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        apq_mls_message: ApqMlsMessageIn,
        ds_timestamp: TimeStamp,
        read_receipts_enabled: bool,
    ) -> anyhow::Result<QsMessageOutcome> {
        let protocol_message = apq_mls_message
            .into_protocol_message()
            .context("expected APQMLS protocol message")?;

        // MLSMessage Phase 1: Load the chat and the group.
        //
        // The group is loaded regardless of whether it has a pending commit or
        // not.
        let group_id = protocol_message.group_id().t_group_id().clone();
        let (chat, mut group) = Self::load_chat_and_group(txn, &group_id).await?;

        // MLSMessage Phase 2: Process the message
        let result = group
            .group_mut()
            .process_apq_message(txn, self.api_clients(), protocol_message)
            .await?;
        let Some(processed_message) =
            Self::take_processed(txn, chat.id(), &mut group, result).await?
        else {
            return Ok(QsMessageOutcome::empty());
        };

        self.finalize_handle_message(
            txn,
            ds_timestamp,
            read_receipts_enabled,
            chat,
            group,
            processed_message,
        )
        .await
    }

    async fn finalize_handle_message(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        ds_timestamp: TimeStamp,
        read_receipts_enabled: bool,
        mut chat: Chat,
        mut group: VerifiedGroup,
        processed_message: ProcessMessageProcessed,
    ) -> anyhow::Result<QsMessageOutcome> {
        let ProcessMessageProcessed {
            processed_message,
            we_were_removed,
            profile_infos,
        } = processed_message;

        let sender = processed_message.sender().clone();
        let sender_user_id = LeafCredential::from_credential(processed_message.credential())?
            .user_id(group.own_user_id())
            .clone();

        let aad = processed_message.tail_aad().to_vec();

        let chat_id = chat.id();

        let HandledMessages {
            new_messages,
            updated_messages,
            reaction_notifications,
            mut changed_chats,
        } = match processed_message.into_content() {
            ProcessedMessageContent::ApplicationMessage(application_message) => {
                // Drop messages in 1:1 blocked chats Note: In group chats, messages
                // from blocked users are still received and processed.
                if chat.status() == &ChatStatus::Blocked {
                    bail!(BlockedContactError);
                }
                self.handle_application_message(
                    &mut *txn,
                    &group,
                    application_message,
                    ds_timestamp,
                    &sender_user_id,
                    read_receipts_enabled,
                )
                .await?
            }
            ProcessedMessageContent::ProposalMessage(proposal) => {
                let new_messages = self
                    .handle_proposal_message(&mut *txn, &mut group, *proposal, ds_timestamp)
                    .await?;
                group
                    .group_mut()
                    .store_update(&mut *txn, None, None)
                    .await?;
                HandledMessages {
                    new_messages,
                    ..Default::default()
                }
            }
            ProcessedMessageContent::StagedCommitMessage(staged_commit) => {
                let sender_user_credential =
                    StorableUserCredential::load_by_user_id(&mut *txn, &sender_user_id)
                        .await?
                        .ok_or_else(|| anyhow!("No sender user credential found"))?
                        .into();
                let new_messages = self
                    .handle_staged_commit_message(
                        &mut *txn,
                        &mut group,
                        chat,
                        *staged_commit,
                        aad,
                        ds_timestamp,
                        &sender,
                        &sender_user_credential,
                        we_were_removed,
                    )
                    .await?;
                group
                    .group_mut()
                    .store_update(&mut *txn, None, None)
                    .await?;
                HandledMessages {
                    new_messages,
                    ..Default::default()
                }
            }
            ProcessedMessageContent::ExternalJoinProposalMessage(_) => {
                unimplemented!()
            }
            ProcessedMessageContent::OwnPendingCommit => {
                // Our own commit was echoed back before the matching
                // `DsCommitResponse` arrived, so we merge it here and run
                // the same side effects the response would have.
                let (mut group_messages, group_data_bytes) = group
                    .merge_pending_commit(&mut *txn, None, ds_timestamp)
                    .await?;
                let pq_updated_at = group.is_apq().then_some(ds_timestamp);
                group
                    .group_mut()
                    .store_update(&mut *txn, Some(ds_timestamp), pq_updated_at)
                    .await?;
                self.finalize_own_commit(
                    &mut *txn,
                    &group,
                    &mut chat,
                    group_data_bytes,
                    &mut group_messages,
                    None,
                    ds_timestamp,
                )
                .await?;
                HandledMessages {
                    new_messages: group_messages,
                    ..Default::default()
                }
            }
            ProcessedMessageContent::OwnPrivateMessage => {
                bail!("Unexpected OwnPrivateMessage, should have been ignored");
            }
            ProcessedMessageContent::UnresolvedAppDataCommit(_) => {
                bail!("Unexpected UnresolvedAppDataCommit, should have been resolved before")
            }
        };

        let messages = Self::store_new_messages(&mut *txn, chat_id, new_messages).await?;

        // Edits and remote deletes only rebuild the chat notification silently, they must not be
        // returned as new messages, which would alert like a new message.
        for updated_message in &updated_messages {
            changed_chats.push(updated_message.chat_id());
            updated_message.update(&mut *txn).await?;
        }

        // Schedule delivery receipts for incoming new and updated messages
        let delivery_receipts = messages
            .iter()
            .chain(&updated_messages)
            .filter_map(|message| {
                if let Message::Content(content_message) = message.message()
                    && let Disposition::Render | Disposition::Attachment =
                        content_message.content().nested_part.disposition()
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

        // MLSMessage Phase 4: Fetch user profiles of new clients and store them.
        for profile_info in profile_infos {
            Self::schedule_fetch_user_profile(&mut *txn, profile_info).await?;
        }

        Ok(QsMessageOutcome::messages(
            messages,
            reaction_notifications,
            changed_chats,
        ))
    }

    /// Returns a message if it should be stored, otherwise an empty vec.
    async fn handle_application_message(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        group: &Group,
        application_message: ApplicationMessage,
        ds_timestamp: TimeStamp,
        sender: &UserId,
        read_receipts_enabled: bool,
    ) -> anyhow::Result<HandledMessages> {
        let mut content = MimiContent::deserialize(&application_message.into_bytes());

        // Delivery receipt
        if let Ok(content) = &content
            && let NestedPart::SinglePart {
                content_type,
                content: report_content,
                ..
            } = &content.nested_part
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

        // Reaction (add or retraction).
        //
        // Must come before the message-edit branch: a retraction carries
        // `replaces` and would otherwise be misrouted as an edit.
        if let Ok(content) = &content
            && matches!(
                &content.nested_part,
                NestedPart::SinglePart {
                    disposition: Disposition::Reaction,
                    ..
                }
            )
        {
            let (notification, changed_chat) = self
                .handle_reaction(txn, group, content, sender, ds_timestamp)
                .await?;
            // Reactions are not stored as messages; the targeted message is
            // notified as updated from within the handler.
            return Ok(HandledMessages {
                reaction_notifications: notification.into_iter().collect(),
                changed_chats: changed_chat.into_iter().collect(),
                ..Default::default()
            });
        }

        // A message we already store is a replay and must not be stored again.
        // This must run before the edit branch, otherwise a replayed edit is
        // applied a second time and prematurely lands in the edit history,
        // blocking later edits of the same message.
        if let Ok(content) = &content
            && Box::pin(self.reconcile_known_message(txn, group, content, sender)).await?
        {
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

            return Ok(HandledMessages {
                updated_messages: message.into_iter().collect(),
                ..Default::default()
            });
        }

        let message =
            TimestampedMessage::from_mimi_content_result(content, ds_timestamp, sender, group);
        Ok(HandledMessages {
            new_messages: vec![message],
            ..Default::default()
        })
    }

    /// Reconciles an inbound message whose Mimi ID we already store. Returns
    /// true if the message was known.
    async fn reconcile_known_message(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        group: &Group,
        content: &MimiContent,
        sender: &UserId,
    ) -> anyhow::Result<bool> {
        let Ok(mimi_id) = MimiId::calculate(group.group_id(), sender, content) else {
            return Ok(false);
        };
        if ChatMessage::load_by_mimi_id(&mut *txn, &mimi_id)
            .await?
            .is_none()
        {
            // A Mimi ID in the edit history is a superseded version of a
            // message. Applying it again would revert a newer edit.
            let is_superseded_edit = MessageEdit::find_message_id(&mut *txn, &mimi_id)
                .await?
                .is_some();
            if is_superseded_edit {
                info!(?mimi_id, "Ignoring replay of a superseded message edit");
            }
            return Ok(is_superseded_edit);
        }

        info!(?mimi_id, "Ignoring duplicate of an already known message");
        Ok(true)
    }

    /// Apply an incoming reaction (add or retraction) to the targeted message.
    ///
    /// Returns a notification if another user reacted to one of our own
    /// messages, and the chat id whose notification content changed silently
    /// if a reaction on one of our own messages was retracted.
    async fn handle_reaction(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        group: &Group,
        content: &MimiContent,
        sender: &UserId,
        ds_timestamp: TimeStamp,
    ) -> anyhow::Result<(Option<ReactionNotification>, Option<ChatId>)> {
        // Retraction: `replaces` the previously sent reaction with an empty body.
        if let Some(replaces) = content.replaces.as_ref() {
            let replaced_mimi_id = MimiId::from_slice(replaces)?;
            let mut changed_chat = None;
            if let Some(target_mimi_id) =
                Reaction::delete_by_mimi_id(&mut *txn, &replaced_mimi_id).await?
                && let Some(target) =
                    ChatMessage::load_by_mimi_id(&mut *txn, &target_mimi_id).await?
            {
                txn.notifier().update(target.id());
                if target.message().sender() == Some(self.user_id()) {
                    changed_chat = Some(target.chat_id());
                }
            }
            return Ok((None, changed_chat));
        }

        // Add: `in_reply_to` references the reacted-to message.
        let Some(in_reply_to) = content.in_reply_to.as_ref() else {
            warn!("Received reaction without in_reply_to, ignoring");
            return Ok((None, None));
        };
        let target_mimi_id = MimiId::from_slice(in_reply_to)?;

        let Some(target) = ChatMessage::load_by_mimi_id(&mut *txn, &target_mimi_id).await? else {
            warn!("Received reaction for unknown message, ignoring");
            return Ok((None, None));
        };

        let NestedPart::SinglePart { content: body, .. } = &content.nested_part else {
            return Ok((None, None));
        };
        let emoji = String::from_utf8(body.clone()).context("Reaction emoji is not valid UTF-8")?;

        let chat_id = target.chat_id();
        // Notify only when someone else reacts to a message we sent.
        let notify = sender != self.user_id() && target.message().sender() == Some(self.user_id());

        let reaction_mimi_id = MimiId::calculate(group.group_id(), sender, content)?;
        let reaction = Reaction::new(
            reaction_mimi_id,
            target_mimi_id,
            chat_id,
            sender.clone(),
            emoji.clone(),
            ds_timestamp,
        );
        reaction.store(&mut *txn).await?;
        txn.notifier().update(target.id());

        let notification = notify.then(|| ReactionNotification {
            chat_id,
            reactor: sender.clone(),
            emoji,
            original_chat_message: target,
        });
        Ok((notification, None))
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
    ) -> anyhow::Result<Vec<TimestampedMessage>> {
        let mut messages = Vec::new();

        let Sender::Member(sender_index) = proposal.sender() else {
            bail!("No external senders supported yet");
        };

        // A peer re-sends an identical self-remove proposal when it retries a leave
        // that did not change the epoch. Storing it again is a no-op, but we must
        // not emit the system message or apply the role change twice.
        if group
            .group()
            .mls_group()
            .pending_proposals()
            .any(|pending| pending.proposal_reference_ref() == proposal.proposal_reference_ref())
        {
            debug!("Ignoring duplicate proposal");
            return Ok(vec![]);
        }

        let removed_index = removed_client(&proposal)
            .context("Only Removes and SelfRemoves are supported for now")?;

        let Some(removed_credential) = group.credential_at(removed_index)? else {
            warn!("Removed user credential not found");
            return Ok(vec![]);
        };
        let removed = removed_credential.user_id();

        let Some(sender_credential) = group.credential_at(*sender_index)? else {
            warn!("Sender credential not found");
            return Ok(vec![]);
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

        Ok(messages)
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
        sender_user_credential: &UserCredential,
        we_were_removed: bool,
    ) -> anyhow::Result<Vec<TimestampedMessage>> {
        // If a client joined externally, we check if the
        // group belongs to an unconfirmed chat.

        // StagedCommitMessage Phase 1: Confirm the chat if unconfirmed

        let mut group_messages = if chat.is_unconfirmed() {
            let message = self
                .handle_unconfirmed_chat(
                    txn,
                    aad,
                    ds_timestamp,
                    sender,
                    sender_user_credential,
                    &mut chat,
                    group.group_mut(),
                )
                .await?;
            vec![message]
        } else {
            vec![]
        };

        // StagedCommitMessage Phase 2: Merge the staged commit into the group.

        // If we were removed, we set the group to inactive.
        if we_were_removed {
            let past_members = group.members().collect();
            chat.set_status(&mut *txn, ChatStatus::inactive(past_members))
                .await?;

            // Removal from the self group means a sibling device unlinked us.
            // Record it so the app can act on it, this launch or a later one.
            if group.group().is_self_group() {
                error!("this device was unlinked by another device of this user");
                OwnClientInfo::mark_account_unlinked(&mut *txn).await?;
            }
        }
        let (messages_from_commit, group_data_bytes) = group
            .merge_pending_commit(&mut *txn, staged_commit, ds_timestamp)
            .await?;

        group_messages.extend(messages_from_commit);

        if let Some(group_data_bytes) = group_data_bytes {
            let group_data = GroupData::decode(&group_data_bytes)?;
            let (chat_title, group_profile_part) =
                group_data.into_parts(group.identity_link_wrapper_key());
            let chat_picture = Self::resolve_group_profile_part(
                txn,
                chat.group_id(),
                sender_user_credential.user_id(),
                ds_timestamp,
                group_profile_part,
                false,
            )
            .await?;
            // Update chat title according to new group data
            match (chat_title, chat_picture) {
                (Some(title), Some(picture)) => {
                    update_chat_attributes(
                        txn,
                        &mut chat,
                        sender_user_credential.user_id(),
                        ChatAttributes {
                            title,
                            picture: Some(picture),
                        },
                        ds_timestamp,
                        &mut group_messages,
                    )
                    .await?;
                }
                (Some(title), None) => {
                    update_chat_title(
                        txn,
                        &mut chat,
                        sender_user_credential.user_id(),
                        title,
                        ds_timestamp,
                        &mut group_messages,
                    )
                    .await?;
                }
                (None, Some(_)) => error!("Received group data with legacy picture and no title"),
                (None, None) => (),
            }
        }

        Ok(group_messages)
    }

    #[expect(clippy::too_many_arguments)]
    async fn handle_unconfirmed_chat(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        aad: Vec<u8>,
        ds_timestamp: TimeStamp,
        sender: &Sender,
        sender_user_credential: &UserCredential,
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

        let sender_user_id = sender_user_credential.user_id();

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
            (sender_user_credential.clone(), user_profile_key),
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
    ) -> anyhow::Result<()> {
        // Don't update the profile if the chat is blocked
        let chat_id = ChatId::try_from(&params.group_id)?;
        if BlockedContact::check_blocked_chat(&mut *txn, chat_id).await? {
            bail!(BlockedContactError);
        }

        // Phase 1: Load the group and the sender. Self-group leaves carry a
        // self-group credential instead of a user credential, so the sender of
        // a self-group update is a sibling device of the own user.
        let group = Group::load_verified(&mut *txn, &params.group_id)
            .await?
            .context("No group found")?;
        let sender_credential = if group.is_self_group() {
            self.inner.key_store.signing_key.credential().clone()
        } else {
            group
                .credential_at(params.sender_index)?
                .context("No sender credential found")?
        };
        let sender = sender_credential.user_id();

        // Phase 2: Decrypt the new user profile key
        let new_user_profile_key = UserProfileKey::decrypt(
            group.identity_link_wrapper_key(),
            &params.user_profile_key,
            sender,
        )?;

        // Phase 3: Fetch and store the (new) user profile and key
        Self::schedule_fetch_user_profile(txn, (sender_credential, new_user_profile_key)).await?;

        Ok(())
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
        for qs_message in qs_messages {
            // Start an outer transaction where the ratchet is loaded and updated. A savepoint after
            // the ratchet is loaded is passed to the processing of the QS message. This savepoint
            // can be rolled back but this transaction MUST be committed. It is needed to make sure
            // that processing is cancel-safe.
            let mut connection = match self.db().write().await {
                Ok(c) => c,
                Err(error) => {
                    error!(%error, "Failed to start the ratchet transaction");
                    return result;
                }
            };

            let mut txn = match connection.begin().await {
                Ok(txn) => txn,
                Err(error) => {
                    error!(%error, "Failed to start the ratchet transaction");
                    return result;
                }
            };

            // Decrypt and process the message (and Box the large future)
            if let Err(error) = Box::pin(self.decrypt_and_process_qs_message(
                &mut txn,
                qs_message,
                read_receipts_enabled,
                &mut result,
            ))
            .await
            {
                error!(%error, "Fatal error when processing a QS message; stopping loop");
                return result; // Stop processing
            }

            result.processed += 1;

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

        result.chats_with_changed_notifications.sort_unstable();
        result.chats_with_changed_notifications.dedup();
        result
    }

    /// Returns `Ok(())` if the more messages should be processed, or `Err` if the processing
    /// should be aborted.
    async fn decrypt_and_process_qs_message(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        qs_message: QueueMessage,
        read_receipts_enabled: bool,
        result: &mut ProcessedQsMessages,
    ) -> sqlx::Result<()> {
        let qs_message_payload =
            match StorableQsQueueRatchet::decrypt_qs_queue_message(txn, qs_message).await {
                Ok(Some(qs_message_payload)) => qs_message_payload,
                Ok(None) => {
                    // Skip the message if it is behind the ratchet (replay)
                    return Ok(());
                }
                Err(error) => {
                    // Cannot decrypt or deserialize the message's container
                    error!(%error, "QS queue message decryption failed; dropping message");
                    result.errors.push(error.into());
                    return Ok(());
                }
            };

        let qs_message_plaintext = match qs_message_payload.extract() {
            Ok(extracted) => extracted,
            Err(error) => {
                error!(%error, "Extracting message failed; dropping message");
                result.errors.push(error.into());
                return Ok(());
            }
        };

        // We create a nested savepoint transaction that we can rollback independently from
        // the parent txn which contains the updates done to the queue ratchet.
        //
        // If the handler fails, we want to *silently* rollback this savepoint, while always
        // committing the parent one.
        let mut savepoint_txn = txn.begin().await?;

        match Box::pin(self.process_qs_message(
            &mut savepoint_txn,
            qs_message_plaintext,
            read_receipts_enabled,
        ))
        .await
        {
            Ok(processed) => {
                savepoint_txn.commit().await?;
                result.merge(processed);
            }
            Err(error) if error.downcast_ref::<BlockedContactError>().is_some() => {
                info!("Dropping message from blocked contact");
            }
            Err(error) => {
                match error.downcast::<sqlx::Error>() {
                    Ok(error) if error.as_database_error().is_some() => {
                        // Fatal database error, stop processing
                        return Err(error);
                    }
                    Ok(error) => {
                        error!(%error, "Processing message failed with a recoverable database error; continue");
                        result.errors.push(error.into());
                    }
                    Err(error) => {
                        error!(%error, "Processing message failed; continue");
                        result.errors.push(error);
                    }
                }
            }
        }

        Ok(())
    }
}
