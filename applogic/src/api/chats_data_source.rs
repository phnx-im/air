// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use aircommon::identifiers::{UserId, Username, UsernameHash};
use aircoreclient::{
    AddUsernameContactError, ChatId, MessageId,
    clients::CoreUser,
    db::notification::{DbEntityId, DbNotification},
};
use flutter_rust_bridge::{Rust2DartSendError, frb};
use tokio_stream::StreamExt;
use tokio_util::sync::{CancellationToken, DropGuard};
use tracing::error;

use crate::{
    StreamSink,
    api::{
        chat_details_cubit::load_chat_details,
        types::{UiChatDetails, UiChatMuted, UiUsername},
        user_cubit::UserCubitBase,
    },
};

/// A delta [`ChatsDataSource`] emits in its stream.
pub struct ChatsDelta {
    pub upserted: Vec<UiChatDetails>,
    pub removed: HashSet<ChatId>,
    /// Only when ordering of chats changed.
    pub order: Option<Vec<ChatId>>,
}

impl ChatsDelta {
    fn is_empty(&self) -> bool {
        self.upserted.is_empty() && self.removed.is_empty() && self.order.is_none()
    }
}

#[frb(opaque)]
#[derive(Clone)]
pub struct ChatsDataSource {
    inner: Arc<ChatsDataSourceInner>,
}

#[frb(ignore)]
struct ChatsDataSourceInner {
    core_user: CoreUser,
    cancel: CancellationToken,
    _guard: DropGuard,
}

impl ChatsDataSource {
    #[frb(sync)]
    pub fn new(user_cubit: &UserCubitBase) -> Self {
        let cancel = CancellationToken::new();
        let guard = cancel.clone().drop_guard();
        Self {
            inner: Arc::new(ChatsDataSourceInner {
                core_user: user_cubit.core_user().clone(),
                cancel,
                _guard: guard,
            }),
        }
    }

    pub fn close(&self) {
        self.inner.cancel.cancel();
    }

    pub async fn mute(
        &self,
        chat_id: ChatId,
        muted_until: Option<UiChatMuted>,
    ) -> anyhow::Result<()> {
        self.inner
            .core_user
            .set_chat_muted_until(chat_id, muted_until.map(Into::into))
            .await
    }

    pub async fn create_contact_chat(
        &self,
        username: UiUsername,
        hash: UsernameHash,
    ) -> anyhow::Result<Option<AddUsernameContactError>> {
        let username = Username::new(username.plaintext)?;
        self.inner
            .core_user
            .add_contact(username, hash)
            .await
            .map(Result::err)
    }

    pub async fn create_group_chat(
        &self,
        group_name: String,
        picture: Option<Vec<u8>>,
        is_apq: bool,
    ) -> anyhow::Result<ChatId> {
        self.inner
            .core_user
            .create_chat(group_name, picture, is_apq)
            .await
    }

    /// Long running task
    ///
    /// One delta carrying every chat and the full order, then one delta per database change.
    /// Returns when cancelled.
    pub async fn stream(&self, sink: StreamSink<ChatsDelta>) {
        // Subscribe *before* the initial load not to lose changes.
        let mut notifications = self.inner.core_user.db_notifications();

        let Ok(mut state) = self.inner.initial_load(&sink).await else {
            return;
        };

        loop {
            let next = tokio::select! {
                _ = self.inner.cancel.cancelled() => return,
                notification = notifications.next() => notification,
            };
            let Some(notification) = next else {
                return;
            };

            let affected = state.affected(&notification);
            if affected.is_empty() {
                continue;
            }

            let delta = self.inner.reload(affected, &mut state).await;
            if !delta.is_empty() && sink.add(delta).is_err() {
                return;
            }
        }
    }
}

impl ChatsDataSourceInner {
    /// Loads the initial state and publishes it to the stream (in chunks).
    ///
    /// Returns the loaded state.
    async fn initial_load(
        &self,
        sink: &StreamSink<ChatsDelta>,
    ) -> Result<LocalState, Rust2DartSendError> {
        let all = self
            .core_user
            .ordered_chat_ids()
            .await
            .inspect_err(|error| error!(%error, "Failed to load chats"))
            .unwrap_or_default();

        if all.is_empty() {
            // Empty account => emit "no chats" delta
            sink.add(ChatsDelta {
                upserted: Vec::new(),
                removed: HashSet::new(),
                order: Some(Vec::new()),
            })?;
            return Ok(LocalState::default());
        }

        let mut state = LocalState {
            // Prefix of `all`: published chats to the stream
            order: Vec::with_capacity(all.len()),
            ..Default::default()
        };

        for chunk in all.chunks(CHUNK) {
            if self.cancel.is_cancelled() {
                return Ok(state);
            }

            let upserted = self.load_chats(chunk).await;

            state.order.extend(upserted.iter().map(|chat| chat.id));
            for chat in &upserted {
                state.record(chat);
            }

            sink.add(ChatsDelta {
                upserted,
                removed: HashSet::new(),
                order: Some(state.order.clone()),
            })?;
        }

        Ok(state)
    }

    async fn reload(&self, affected: HashSet<ChatId>, state: &mut LocalState) -> ChatsDelta {
        let chat_ids: Vec<ChatId> = affected.into_iter().collect();
        let mut upserted = self.load_chats(&chat_ids).await;

        let loaded: HashSet<ChatId> = upserted.iter().map(|chat| chat.id).collect();
        let mut removed: HashSet<ChatId> = chat_ids
            .iter()
            .copied()
            .filter(|chat_id| !loaded.contains(chat_id) && state.contains(chat_id))
            .collect();

        for chat in &upserted {
            state.record(chat);
        }
        for chat_id in &removed {
            state.remove(chat_id);
        }

        // Build the order to publish
        let mut order = Vec::new();
        for chat_id in self
            .core_user
            .ordered_chat_ids()
            .await
            .inspect_err(|error| error!(%error, "Failed to load chats"))
            .unwrap_or_default()
        {
            if state.contains(&chat_id) {
                order.push(chat_id);
            } else if let Some(chat) = self.core_user.chat(&chat_id).await {
                let details = load_chat_details(&self.core_user, chat).await;
                state.record(&details);
                upserted.push(details);
                order.push(chat_id);
            }
        }

        // Chats that dropped out of the order without notification
        let retained: HashSet<ChatId> = order.iter().copied().collect();
        removed.extend(
            state
                .order
                .iter()
                .copied()
                .filter(|chat_id| !retained.contains(chat_id)),
        );
        for chat_id in &removed {
            state.remove(chat_id);
        }

        let reordered = state.order != order;
        if reordered {
            state.order = order.clone();
        }

        ChatsDelta {
            upserted,
            removed,
            order: reordered.then_some(order),
        }
    }

    async fn load_chats(&self, chat_ids: &[ChatId]) -> Vec<UiChatDetails> {
        let mut chats = Vec::with_capacity(chat_ids.len());
        for chat_id in chat_ids {
            if let Some(chat) = self.core_user.chat(chat_id).await {
                chats.push(load_chat_details(&self.core_user, chat).await);
            }
        }
        chats
    }
}

/// Should be around a full screen of chats.
const CHUNK: usize = 32;

/// What has been published to Dart.
///
/// Contains only the data which is enough to answer "which chats does this notification touch" and
/// "have we published this one". The details themselves live on the Dart side.
#[frb(ignore)]
#[derive(Default)]
struct LocalState {
    published: HashMap<ChatId, ChatKeys>,
    // Published order of chats
    order: Vec<ChatId>,
}

#[frb(ignore)]
struct ChatKeys {
    last_message: Option<MessageId>,
    /// Only for connection chats
    peer: Option<UserId>,
}

impl ChatKeys {
    fn of(chat: &UiChatDetails) -> Self {
        Self {
            last_message: chat.last_message.as_ref().map(|m| m.id),
            peer: chat.connection_user_id().cloned().map(UserId::from),
        }
    }
}

impl LocalState {
    fn contains(&self, chat_id: &ChatId) -> bool {
        self.published.contains_key(chat_id)
    }

    fn record(&mut self, chat: &UiChatDetails) {
        self.published.insert(chat.id, ChatKeys::of(chat));
    }

    fn remove(&mut self, chat_id: &ChatId) {
        self.published.remove(chat_id);
    }

    fn affected(&self, notification: &DbNotification) -> HashSet<ChatId> {
        let mut affected = HashSet::new();
        let mut changed_messages = HashSet::new();
        let mut changed_users = HashSet::new();

        for entity_id in notification.ops.keys() {
            match entity_id {
                DbEntityId::User(user_id) => {
                    changed_users.insert(user_id);
                }
                DbEntityId::Chat(chat_id) => {
                    affected.insert(*chat_id);
                }
                DbEntityId::Message(message_id) => {
                    changed_messages.insert(*message_id);
                }
                _ => {}
            }
        }

        if changed_messages.is_empty() && changed_users.is_empty() {
            return affected;
        }

        // Indirectly updated chats
        for (chat_id, keys) in &self.published {
            let indirectly_updated = keys
                .last_message
                .as_ref()
                .is_some_and(|id| changed_messages.contains(id))
                || keys
                    .peer
                    .as_ref()
                    .is_some_and(|id| changed_users.contains(id));
            if indirectly_updated {
                affected.insert(*chat_id);
            }
        }

        affected
    }
}
