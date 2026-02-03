// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! A single chat details feature

use std::path::PathBuf;
use std::time::Duration;

use aircommon::{
    OpenMlsRand, RustCrypto,
    identifiers::{AttachmentId, UserId},
};
use aircoreclient::{
    AttachmentProgress, Chat, ChatId, ChatMessage, MessageDraft, ProvisionAttachmentError,
    UploadTaskError,
};
use aircoreclient::{MessageId, clients::CoreUser, store::Store};
use chrono::{DateTime, Local, SubsecRound, Utc};
use flutter_rust_bridge::frb;
use mimi_content::{ByteBuf, Disposition, MimiContent, NestedPart, NestedPartContent};
use tokio::{sync::watch, time::sleep};
use tokio_stream::StreamExt;
use tracing::{error, info};

use crate::message_content::MimiContentExt;
use crate::{StreamSink, mark_as_read::MarkAsReadState};
use crate::{
    api::{
        attachments_repository::{AttachmentTaskHandle, AttachmentsRepository, InProgressMap},
        chats_repository::ChatsRepository,
        types::{UiChatType, UiUserId},
        user_settings_cubit::{UserSettings, UserSettingsCubitBase},
    },
    mark_as_read::MarkAsRead,
};
use crate::{
    notifications::NotificationService,
    util::{Cubit, CubitCore, spawn_from_sync},
};

use super::{types::UiChatDetails, user_cubit::UserCubitBase};

/// The state of a single chat
///
/// Contains the chat details and the list of members.
///
/// Also see [`ChatDetailsCubitBase`].
#[frb(dart_metadata = ("freezed"))]
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash)]
pub struct ChatDetailsState {
    pub chat: Option<UiChatDetails>,
    pub members: Vec<UiUserId>,
}

/// The cubit responsible for a single chat
///
/// Fetches the chat details and the list of members. Allows to modify the chat details, send
/// messages and mark the chat as read up to a given message.
#[frb(opaque)]
pub struct ChatDetailsCubitBase {
    context: ChatDetailsContext,
    core: CubitCore<ChatDetailsState>,
    user_settings_rx: watch::Receiver<UserSettings>,
    attachment_in_progress: InProgressMap,
}

impl ChatDetailsCubitBase {
    /// Creates a new cubit for the given chat.
    ///
    /// The cubit will fetch the chat details and the list of members. It will also listen to the
    /// changes in the chat and update the state accordingly.
    #[frb(sync)]
    pub fn new(
        user_cubit: &UserCubitBase,
        user_settings_cubit: &UserSettingsCubitBase,
        chat_id: ChatId,
        chats_repository: &ChatsRepository,
        attachments_repository: &AttachmentsRepository,
        with_members: bool,
    ) -> Self {
        let store = user_cubit.core_user().clone();

        let initial_state = ChatDetailsState {
            chat: chats_repository.get(chat_id),
            members: Default::default(),
        };
        let core = CubitCore::with_initial_state(initial_state);

        let user_settings_rx = user_settings_cubit.subscribe();

        let context = ChatDetailsContext::new(
            store.clone(),
            chats_repository.clone(),
            user_cubit.notification_service().clone(),
            core.state_tx().clone(),
            chat_id,
            with_members,
        );

        let emit_initial_state_task =
            core.cancellation_token()
                .clone()
                .run_until_cancelled_owned({
                    let context = context.clone();
                    async move { context.load_and_emit_state().await }
                });
        spawn_from_sync(emit_initial_state_task);

        let update_state_task = core
            .cancellation_token()
            .clone()
            .run_until_cancelled_owned(context.clone().update_state_task());
        spawn_from_sync(update_state_task);

        Self {
            context,
            core,
            user_settings_rx,
            attachment_in_progress: attachments_repository.in_progress().clone(),
        }
    }

    // Cubit interface

    pub fn close(&mut self) {
        self.core.close();
    }

    #[frb(getter, sync)]
    pub fn is_closed(&self) -> bool {
        self.core.is_closed()
    }

    #[frb(getter, sync)]
    pub fn state(&self) -> ChatDetailsState {
        self.core.state()
    }

    pub async fn stream(&mut self, sink: StreamSink<ChatDetailsState>) {
        self.core.stream(sink).await;
    }

    // Cubit methods

    /// Sets the chat picture.
    ///
    /// When `bytes` is `None`, the chat picture is removed.
    pub async fn set_chat_picture(&mut self, bytes: Option<Vec<u8>>) -> anyhow::Result<()> {
        Store::set_chat_picture(&self.context.store, self.context.chat_id, bytes.clone()).await
    }

    pub async fn set_chat_title(&mut self, title: String) -> anyhow::Result<()> {
        Store::set_chat_title(&self.context.store, self.context.chat_id, title).await
    }

    pub async fn delete_message(&self, message_id: MessageId) -> anyhow::Result<()> {
        // Load message
        let message = self.context.store.message(message_id).await?;

        let Some(_message) = message else {
            return Ok(());
        };

        let salt: [u8; 16] = RustCrypto::default().random_array()?;
        let content = MimiContent {
            salt: ByteBuf::from(salt),
            replaces: None, // Replaces is set by store_unsent_message
            topic_id: Default::default(),
            expires: None,
            in_reply_to: None,
            extensions: Default::default(),
            nested_part: NestedPart {
                disposition: Disposition::Render,
                language: "".to_owned(),
                part: NestedPartContent::NullPart,
            },
        };

        self.context
            .store
            .send_message(self.context.chat_id, content, Some(message_id))
            .await
            .inspect_err(|error| error!(%error, "Failed to send message"))?;

        Ok(())
    }

    /// Sends a message to the chat.
    ///
    /// The not yet sent message is immediately stored in the local store and then the message is
    /// send to the DS.
    pub async fn send_message(&self, message_text: String) -> anyhow::Result<()> {
        let mut draft = None;
        self.core.state_tx().send_if_modified(|state| {
            let Some(chat) = state.chat.as_mut() else {
                return false;
            };
            draft = chat.draft.take();
            draft.is_some()
        });

        // Remove stored draft
        if draft.is_some() {
            self.context
                .store
                .store_message_draft(self.context.chat_id, None)
                .await?;
        }
        let editing_id = draft.and_then(|d| d.editing_id);

        let salt: [u8; 16] = RustCrypto::default().random_array()?;
        let content = MimiContent::simple_markdown_message(message_text, salt);

        self.context
            .store
            .send_message(self.context.chat_id, content, editing_id)
            .await
            .inspect_err(|error| error!(%error, "Failed to send message"))?;

        Ok(())
    }

    pub async fn upload_attachment(
        &self,
        path: String,
    ) -> anyhow::Result<Option<UploadAttachmentError>> {
        let path = PathBuf::from(path);
        let (attachment_id, progress, upload_task) = match self
            .context
            .store
            .upload_attachment(self.context.chat_id, &path)
            .await?
        {
            Ok(result) => result,
            Err(error) => return error.into_ui_result(),
        };
        self.upload_attachment_impl(attachment_id, progress, upload_task)
            .await?;
        Ok(None)
    }

    pub async fn retry_upload_attachment(
        &self,
        attachment_id: AttachmentId,
    ) -> anyhow::Result<Option<UploadAttachmentError>> {
        let (new_attachment_id, progress, upload_task) = match self
            .context
            .store
            .retry_upload_attachment(attachment_id)
            .await?
        {
            Ok(result) => result,
            Err(error) => return error.into_ui_result(),
        };
        self.upload_attachment_impl(new_attachment_id, progress, upload_task)
            .await?;
        Ok(None)
    }

    async fn upload_attachment_impl(
        &self,
        attachment_id: AttachmentId,
        progress: AttachmentProgress,
        upload_task: impl Future<Output = Result<ChatMessage, UploadTaskError>> + Send + 'static,
    ) -> anyhow::Result<()> {
        let handle = AttachmentTaskHandle::new(progress);
        let cancel = handle.cancellation_token().clone();
        self.attachment_in_progress.insert(attachment_id, handle);
        match cancel.run_until_cancelled_owned(upload_task).await {
            Some(Ok(message)) => {
                self.context
                    .store
                    .outbound_service()
                    .enqueue_chat_message(message.id(), Some(attachment_id))
                    .await?;
            }
            Some(Err(UploadTaskError { message_id, error })) => {
                error!(%error, ?attachment_id, "Failed to upload attachment");
                self.context
                    .store
                    .outbound_service()
                    .fail_enqueued_chat_message(message_id, Some(attachment_id))
                    .await?;
            }
            None => {
                info!(?attachment_id, "Upload was cancelled");
            }
        }
        Ok(())
    }

    /// Marks the chat as read until the given message id (including).
    ///
    /// The calls to this method are debounced with a fixed delay.
    pub async fn mark_as_read(
        &self,
        until_message_id: MessageId,
        until_timestamp: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        const MARK_AS_READ_DEBOUNCE: Duration = Duration::from_millis(300);
        let service = MarkAsRead::new(&self.context.store, &self.context.notification_service);
        crate::mark_as_read::mark_as_read(
            &service,
            &self.context.mark_as_read_tx,
            &self.user_settings_rx,
            self.context.chat_id,
            until_message_id,
            until_timestamp,
            MARK_AS_READ_DEBOUNCE,
        )
        .await
    }

    #[frb]
    pub async fn store_draft(
        &self,
        draft_message: String,
        is_committed: bool,
    ) -> anyhow::Result<()> {
        if is_committed {
            // Debounce committing the draft to avoid confusing the user. Usually, the draft is
            // committed when the user selects another chat. Committing it immediately reorders the
            // chat list and the chat the user clicked on might be moved.
            sleep(Duration::from_millis(300)).await;
        }

        let changed = self.core.state_tx().send_if_modified(|state| {
            let Some(chat) = state.chat.as_mut() else {
                return false;
            };
            match &mut chat.draft {
                Some(draft) if draft.message != draft_message => {
                    draft.message = draft_message;
                    draft.updated_at = Utc::now();
                    draft.is_committed = is_committed;
                    true
                }
                Some(draft) if draft.is_committed != is_committed => {
                    draft.is_committed = is_committed;
                    true
                }
                Some(_) => false,
                None => {
                    chat.draft.replace(MessageDraft {
                        message: draft_message,
                        is_committed,
                        ..MessageDraft::empty()
                    });
                    true
                }
            }
        });
        if changed {
            self.store_draft_from_state().await?;
        }
        Ok(())
    }

    pub async fn reset_draft(&self) {
        self.core.state_tx().send_if_modified(|state| {
            let Some(chat) = state.chat.as_mut() else {
                return false;
            };
            chat.draft.take().is_some()
        });
    }

    pub async fn edit_message(&self, message_id: Option<MessageId>) -> anyhow::Result<()> {
        // Load message
        let message = match message_id {
            Some(message_id) => self.context.store.message(message_id).await?,
            None => {
                self.context
                    .store
                    .last_message_by_user(self.context.chat_id, self.context.store.user_id())
                    .await?
            }
        };
        let Some(message) = message else {
            return Ok(());
        };

        // Get plain body if any; if none, this message is not editable.
        let Some(body) = message
            .message()
            .mimi_content()
            .and_then(|content| content.plain_body())
        else {
            return Ok(());
        };

        // Update draft in state
        let changed = self.core.state_tx().send_if_modified(|state| {
            let Some(chat) = state.chat.as_mut() else {
                return false;
            };
            let draft = chat.draft.get_or_insert_with(MessageDraft::empty);
            if draft.editing_id.is_some() {
                return false;
            }
            draft.message = body.to_owned();
            draft.editing_id = Some(message.id());
            draft.is_committed = false;
            true
        });

        if changed {
            self.store_draft_from_state().await?;
        }

        Ok(())
    }

    async fn store_draft_from_state(&self) -> anyhow::Result<()> {
        let draft = self
            .core
            .state_tx()
            .borrow()
            .chat
            .as_ref()
            .and_then(|c| c.draft.clone());
        self.context
            .store
            .store_message_draft(self.context.chat_id, draft.as_ref())
            .await?;
        Ok(())
    }

    pub async fn accept_contact_request(&self) -> anyhow::Result<()> {
        let chat_id = self.context.chat_id;
        self.context.store.accept_contact_request(chat_id).await?;
        Ok(())
    }
}

/// Loads the initial state and listen to the changes
#[frb(ignore)]
#[derive(Clone)]
struct ChatDetailsContext {
    store: CoreUser,
    chats_repository: ChatsRepository,
    notification_service: NotificationService,
    state_tx: watch::Sender<ChatDetailsState>,
    chat_id: ChatId,
    mark_as_read_tx: watch::Sender<MarkAsReadState>,
    with_members: bool,
}

impl ChatDetailsContext {
    fn new(
        store: CoreUser,
        chats_repository: ChatsRepository,
        notification_service: NotificationService,
        state_tx: watch::Sender<ChatDetailsState>,
        chat_id: ChatId,
        with_members: bool,
    ) -> Self {
        let (mark_as_read_tx, _) = watch::channel(Default::default());
        Self {
            store,
            chats_repository,
            notification_service,
            state_tx,
            chat_id,
            mark_as_read_tx,
            with_members,
        }
    }

    async fn load_and_emit_state(&self) {
        let (chat, last_read) = self.load_chat_details().await.unzip();
        let is_modified = self.state_tx.send_if_modified(|state| {
            if state.chat != chat {
                state.chat = chat.clone();
                true
            } else {
                false
            }
        });

        if is_modified && let Some(chat) = chat {
            self.chats_repository.put(chat);
        }

        if let Some(last_read) = last_read {
            let _ = self.mark_as_read_tx.send_replace(MarkAsReadState::Marked {
                // truncate nanoseconds because they are not supported by Dart's DateTime
                at: last_read.trunc_subsecs(6),
            });
        }

        if self.with_members {
            let mut members: Vec<UiUserId> = self
                .store
                .chat_participants(self.chat_id)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(From::from)
                .collect();
            members.sort_unstable();
            self.state_tx.send_if_modified(|state| {
                if state.members != members {
                    state.members = members;
                    true
                } else {
                    false
                }
            });
        }
    }

    async fn load_chat_details(&self) -> Option<(UiChatDetails, DateTime<Utc>)> {
        let chat = self.store.chat(&self.chat_id).await?;
        let last_read = chat.last_read();
        let details = load_chat_details(&self.store, chat).await;
        Some((details, last_read))
    }

    /// Returns only when `stop` is cancelled
    async fn update_state_task(self) {
        let mut notifications = self.store.subscribe();
        while let Some(notification) = notifications.next().await {
            if notification.ops.contains_key(&self.chat_id.into()) {
                self.load_and_emit_state().await;
            } else {
                // Don't hold the lock of the state too long
                let user_id = self
                    .state_tx
                    .borrow()
                    .chat
                    .as_ref()
                    .and_then(|chat| chat.connection_user_id())
                    .cloned()
                    .map(UserId::from);
                if let Some(user_id) = user_id
                    && notification.ops.contains_key(&user_id.into())
                {
                    self.load_and_emit_state().await;
                }
            }
        }
    }
}

/// Loads additional details for a chat and converts it into a [`UiChatDetails`]
pub(super) async fn load_chat_details(store: &impl Store, chat: Chat) -> UiChatDetails {
    let messages_count = store.messages_count(chat.id()).await.unwrap_or_default();
    let unread_messages = store
        .unread_messages_count(chat.id())
        .await
        .unwrap_or_default();
    let last_message = store.last_message(chat.id()).await.ok().flatten();
    let last_used = last_message
        .as_ref()
        .map(|m| m.timestamp())
        .or(chat.last_message_at())
        .unwrap_or_default() // default is UNIX_EPOCH
        .with_timezone(&Local);

    let chat_type = UiChatType::load_from_chat_type(store, chat.chat_type).await;

    let draft = store.message_draft(chat.id).await.unwrap_or_default();

    UiChatDetails {
        id: chat.id,
        status: chat.status.into(),
        chat_type,
        last_used,
        attributes: chat.attributes.into(),
        messages_count,
        unread_messages,
        last_message: last_message.map(From::from),
        draft,
    }
}

#[frb(ignore)]
trait IntoUiResult {
    type UiError;

    #[frb(ignore)]
    fn into_ui_result(self) -> anyhow::Result<Option<Self::UiError>>;
}

impl IntoUiResult for ProvisionAttachmentError {
    type UiError = UploadAttachmentError;

    fn into_ui_result(self) -> anyhow::Result<Option<UploadAttachmentError>> {
        match self {
            ProvisionAttachmentError::TooLarge(detail) => {
                Ok(Some(UploadAttachmentError::TooLarge {
                    max_size_bytes: detail.max_size_bytes,
                    actual_size_bytes: detail.actual_size_bytes,
                }))
            }
        }
    }
}

/// Error which can occur when uploading an attachment
pub enum UploadAttachmentError {
    TooLarge {
        max_size_bytes: u64,
        actual_size_bytes: u64,
    },
}
