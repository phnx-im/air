// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! A single chat details feature

use std::path::PathBuf;
use std::time::Duration;

use aircommon::{OpenMlsRand, RustCrypto, identifiers::UserId};
use aircoreclient::{Chat, ChatId};
use aircoreclient::{MessageId, clients::CoreUser, store::Store};
use chrono::{DateTime, SubsecRound, Utc};
use flutter_rust_bridge::frb;
use mimi_content::{
    ByteBuf, Disposition, MessageStatus, MimiContent, NestedPart, NestedPartContent,
};
use tokio::{sync::watch, time::sleep};
use tokio_stream::StreamExt;
use tracing::error;

use crate::api::{
    chats_repository::ChatsRepository,
    types::{UiChatMessage, UiChatType, UiMessageDraft, UiUserId},
};
use crate::message_content::MimiContentExt;
use crate::util::{Cubit, CubitCore, spawn_from_sync};
use crate::{StreamSink, api::types::UiMessageDraftSource};

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
}

impl ChatDetailsCubitBase {
    /// Creates a new cubit for the given chat.
    ///
    /// The cubit will fetch the chat details and the list of members. It will also listen to the
    /// changes in the chat and update the state accordingly.
    #[frb(sync)]
    pub fn new(
        user_cubit: &UserCubitBase,
        chat_id: ChatId,
        chats_repository: &ChatsRepository,
        with_members: bool,
    ) -> Self {
        let store = user_cubit.core_user().clone();

        let initial_state = ChatDetailsState {
            chat: chats_repository.get(chat_id),
            members: Default::default(),
        };
        let core = CubitCore::with_initial_state(initial_state);

        let context = ChatDetailsContext::new(
            store.clone(),
            chats_repository.clone(),
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

        Self { context, core }
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

    pub async fn delete_message(&self) -> anyhow::Result<()> {
        let mut draft = None;
        self.core.state_tx().send_if_modified(|state| {
            let Some(chat) = state.chat.as_mut() else {
                return false;
            };
            draft = chat.draft.take();
            draft.is_some()
        });

        let Some(draft) = draft else {
            return Err(anyhow::anyhow!("You did not select a message to delete"));
        };
        if draft.editing_id.is_none() {
            return Err(anyhow::anyhow!("You did not select a message to delete"));
        }

        // Remove stored draft
        self.context
            .store
            .store_message_draft(self.context.chat_id, None)
            .await?;

        let editing_id = draft.editing_id;

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
            .send_message(self.context.chat_id, content, editing_id)
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
        let content = if message_text == "delete" {
            MimiContent {
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
            }
        } else {
            MimiContent::simple_markdown_message(message_text, salt)
        };

        self.context
            .store
            .send_message(self.context.chat_id, content, editing_id)
            .await
            .inspect_err(|error| error!(%error, "Failed to send message"))?;

        Ok(())
    }

    pub async fn upload_attachment(&self, path: String) -> anyhow::Result<()> {
        let path = PathBuf::from(path);
        self.context
            .store
            .upload_attachment(self.context.chat_id, &path)
            .await?;
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
        let scheduled = self
            .context
            .mark_as_read_tx
            .send_if_modified(|state| match &state {
                MarkAsReadState::NotLoaded => {
                    error!("Marking as read while chat is not loaded");
                    false
                }
                MarkAsReadState::Marked { at }
                | MarkAsReadState::Scheduled {
                    until_timestamp: at,
                    until_message_id: _,
                } if *at < until_timestamp => {
                    *state = MarkAsReadState::Scheduled {
                        until_timestamp,
                        until_message_id,
                    };
                    true
                }
                MarkAsReadState::Marked { .. } => {
                    false // already marked as read
                }
                MarkAsReadState::Scheduled { .. } => {
                    false // already scheduled at a later timestamp
                }
            });
        if !scheduled {
            return Ok(());
        }

        // debounce
        const MARK_AS_READ_DEBOUNCE: Duration = Duration::from_secs(2);
        let mut rx = self.context.mark_as_read_tx.subscribe();
        tokio::select! {
            _ = rx.changed() => return Ok(()),
            _ = sleep(MARK_AS_READ_DEBOUNCE) => {},
        };

        // check if the scheduled state is still valid and if so, mark it as read
        let scheduled = self
            .context
            .mark_as_read_tx
            .send_if_modified(|state| match state {
                MarkAsReadState::Scheduled {
                    until_message_id: scheduled_message_id,
                    until_timestamp,
                } if *scheduled_message_id == until_message_id => {
                    *state = MarkAsReadState::Marked {
                        at: *until_timestamp,
                    };
                    true
                }
                _ => false,
            });
        if !scheduled {
            return Ok(());
        }

        let (_, read_message_ids) = self
            .context
            .store
            .mark_chat_as_read(self.context.chat_id, until_message_id)
            .await?;

        let statuses = read_message_ids
            .iter()
            .map(|(message_id, mimi_id)| (*message_id, mimi_id, MessageStatus::Read));
        if let Err(error) = self
            .context
            .store
            .outbound_service()
            .enqueue_receipts(self.context.chat_id, statuses)
            .await
        {
            error!(%error, "Failed to send read receipt");
        }

        Ok(())
    }

    pub async fn store_draft(&self, draft_message: String) -> anyhow::Result<()> {
        let changed = self.core.state_tx().send_if_modified(|state| {
            let Some(chat) = state.chat.as_mut() else {
                return false;
            };
            match &mut chat.draft {
                Some(draft) if draft.message != draft_message => {
                    draft.message = draft_message;
                    draft.updated_at = Utc::now();
                    true
                }
                Some(_) => false,
                None => {
                    chat.draft.replace(UiMessageDraft::new(
                        draft_message,
                        UiMessageDraftSource::User,
                    ));
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
            let draft = chat.draft.get_or_insert_with(|| UiMessageDraft {
                message: String::new(),
                editing_id: None,
                updated_at: Utc::now(),
                source: UiMessageDraftSource::System,
            });
            if draft.editing_id.is_some() {
                return false;
            }
            draft.message = body.to_owned();
            draft.editing_id = Some(message.id());
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
            .store_message_draft(self.context.chat_id, draft.map(|d| d.into_draft()).as_ref())
            .await?;
        Ok(())
    }
}

/// Loads the initial state and listen to the changes
#[frb(ignore)]
#[derive(Clone)]
struct ChatDetailsContext {
    store: CoreUser,
    chats_repository: ChatsRepository,
    state_tx: watch::Sender<ChatDetailsState>,
    chat_id: ChatId,
    mark_as_read_tx: watch::Sender<MarkAsReadState>,
    with_members: bool,
}

impl ChatDetailsContext {
    fn new(
        store: CoreUser,
        chats_repository: ChatsRepository,
        state_tx: watch::Sender<ChatDetailsState>,
        chat_id: ChatId,
        with_members: bool,
    ) -> Self {
        let (mark_as_read_tx, _) = watch::channel(Default::default());
        Self {
            store,
            chats_repository,
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
    let last_message = store
        .last_message(chat.id())
        .await
        .ok()
        .flatten()
        .map(From::from);
    let last_used = last_message
        .as_ref()
        .map(|m: &UiChatMessage| m.timestamp.clone())
        .unwrap_or_default();
    // default is UNIX_EPOCH

    let chat_type = UiChatType::load_from_chat_type(store, chat.chat_type).await;

    let draft = store
        .message_draft(chat.id)
        .await
        .unwrap_or_default()
        .map(|d| UiMessageDraft::from_draft(d, UiMessageDraftSource::System));

    UiChatDetails {
        id: chat.id,
        status: chat.status.into(),
        chat_type,
        last_used,
        attributes: chat.attributes.into(),
        messages_count,
        unread_messages,
        last_message,
        draft,
    }
}

#[frb(ignore)]
#[derive(Debug, Default)]
enum MarkAsReadState {
    #[default]
    NotLoaded,
    /// Chat is marked as read until the given timestamp
    Marked { at: DateTime<Utc> },
    /// Chat is scheduled to be marked as read until the given timestamp and message id
    Scheduled {
        until_timestamp: DateTime<Utc>,
        until_message_id: MessageId,
    },
}
