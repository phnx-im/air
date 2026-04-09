// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! A list of messages feature

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use aircoreclient::{
    ChatId, ChatMessage, ChatType, MessageId,
    store::{Store, StoreEntityId, StoreNotification, StoreOperation},
};
use flutter_rust_bridge::frb;
use tokio::sync::{mpsc, watch};
use tokio_stream::{Stream, StreamExt};
use tokio_util::sync::CancellationToken;
use tracing::{error, warn};

use crate::{
    StreamSink,
    util::{Cubit, CubitCore, spawn_from_sync},
};

use super::{
    types::{UiChatMessage, UiFlightPosition},
    user_cubit::UserCubitBase,
};

const PAGE_SIZE: usize = 50;
/// Maximum number of messages kept in the loaded window.
/// When a prepend/append would exceed this, messages are dropped from the far end.
const MAX_WINDOW: usize = PAGE_SIZE * 4;

/// The state representing a list of messages in a chat
///
/// The state is cheaply cloneable (internally reference counted).
#[frb(opaque)]
#[derive(Debug, Default, Clone)]
pub struct MessageListState {
    /// Copy-on-write inner ref to make the state cheaply cloneable when emitting new state
    inner: Arc<MessageListStateInner>,
}

#[frb(ignore)]
#[derive(Debug, Default)]
struct MessageListStateInner {
    /// Whether the chat the messages are in is a connection chat
    is_connection_chat: Option<bool>,
    /// Loaded messages (not all messages in the chat)
    messages: Vec<UiChatMessage>,
    /// Lookup index mapping a message id to the index in `messages`
    message_ids_index: HashMap<MessageId, usize>,
    /// Newly added messages
    new_messages: HashSet<MessageId>,
    /// More messages exist before the loaded window
    has_older: bool,
    /// More messages exist after the loaded window
    has_newer: bool,
    /// Whether the window is anchored at the most recent messages
    is_at_bottom: bool,
    /// Index Flutter should scroll to after a load (transient, cleared after read)
    scroll_to_index: Option<usize>,
    /// Index of the first unread message (set on initial load only)
    first_unread_index: Option<usize>,
}

/// Attributes of the message list state.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[frb(dart_metadata = ("freezed"), type_64bit_int)]
pub struct MessageListMeta {
    pub is_connection_chat: Option<bool>,
    pub has_older: bool,
    pub has_newer: bool,
    pub is_at_bottom: bool,
    pub scroll_to_index: Option<usize>,
    pub first_unread_index: Option<usize>,
}

#[frb(ignore)]
enum LoadDirection {
    /// Full replacement (initial load, jump-to-message, jump-to-bottom)
    Replace {
        has_older: bool,
        has_newer: bool,
        is_at_bottom: bool,
        scroll_to_index: Option<usize>,
        first_unread_index: Option<usize>,
    },
    /// Prepend older messages before the current window
    PrependOlder { has_older: bool },
    /// Append newer messages after the current window
    AppendNewer { has_newer: bool },
}

/// Compute flight positions for a slice of `UiChatMessage` in order.
///
/// `flight_break_at` inserts an unconditional flight break before that index
/// (used for the unread divider).
fn compute_flight_positions(messages: &mut [UiChatMessage], flight_break_at: Option<usize>) {
    recompute_flight_positions_range(messages, 0, messages.len(), flight_break_at);
}

/// Recompute flight positions for messages in range `[start, end)`, using neighbors
/// outside the range for context. Messages outside the range are NOT modified.
///
/// `flight_break_at` inserts an unconditional flight break before that index
/// (used for the unread divider).
fn recompute_flight_positions_range(
    messages: &mut [UiChatMessage],
    start: usize,
    end: usize,
    flight_break_at: Option<usize>,
) {
    let end = end.min(messages.len());
    for i in start..end {
        let pos = {
            // Treat the unread divider boundary as a flight break by hiding
            // the previous/next message from the position calculation.
            let prev = if i > 0 && flight_break_at != Some(i) {
                Some(&messages[i - 1])
            } else {
                None
            };
            let next = if flight_break_at == Some(i + 1) {
                None
            } else {
                messages.get(i + 1)
            };
            UiFlightPosition::calculate(&messages[i], prev, next)
        };
        messages[i].position = pos;
    }
}

impl MessageListState {
    /// Apply new messages to the state according to the given direction.
    fn apply_messages(
        &mut self,
        new_messages: Vec<ChatMessage>,
        is_connection_chat: Option<bool>,
        direction: LoadDirection,
    ) {
        match direction {
            LoadDirection::Replace {
                has_older,
                has_newer,
                is_at_bottom,
                scroll_to_index,
                first_unread_index,
            } => {
                let mut messages: Vec<UiChatMessage> =
                    new_messages.into_iter().map(From::from).collect();
                compute_flight_positions(&mut messages, first_unread_index);

                let mut message_ids_index = HashMap::with_capacity(messages.len());
                for (i, msg) in messages.iter().enumerate() {
                    message_ids_index.insert(msg.id, i);
                }

                let inner = MessageListStateInner {
                    is_connection_chat: is_connection_chat.or(self.inner.is_connection_chat),
                    message_ids_index,
                    messages,
                    new_messages: HashSet::new(),
                    has_older,
                    has_newer,
                    is_at_bottom,
                    scroll_to_index,
                    first_unread_index,
                };
                self.inner = Arc::new(inner);
            }
            LoadDirection::PrependOlder { has_older } => {
                let mut prepended: Vec<UiChatMessage> =
                    new_messages.into_iter().map(From::from).collect();
                let prepend_count = prepended.len();
                let shifted_unread = self.inner.first_unread_index.map(|i| i + prepend_count);

                // Compute positions for the new (prepended) messages only
                compute_flight_positions(&mut prepended, None);

                let mut messages = prepended;
                messages.extend(self.inner.messages.iter().cloned());

                // Recompute only the boundary: last prepended + first existing
                if prepend_count > 0 && messages.len() > prepend_count {
                    let boundary_start = prepend_count.saturating_sub(1);
                    let boundary_end = (prepend_count + 1).min(messages.len());
                    recompute_flight_positions_range(
                        &mut messages,
                        boundary_start,
                        boundary_end,
                        shifted_unread,
                    );
                }

                // Evict newer messages if the window exceeds the cap
                let has_newer = if messages.len() > MAX_WINDOW {
                    messages.truncate(MAX_WINDOW);
                    true
                } else {
                    self.inner.has_newer
                };

                // Unread index may have been evicted
                let first_unread_index = shifted_unread.filter(|&i| i < messages.len());

                let mut message_ids_index = HashMap::with_capacity(messages.len());
                for (i, msg) in messages.iter().enumerate() {
                    message_ids_index.insert(msg.id, i);
                }

                let new_messages = self.inner.new_messages.clone();

                // Preserve is_at_bottom when the newest tail is still in the
                // window (no eviction happened). Forcing it to false here
                // would block StoreOperation::Add from appending incoming
                // messages even though the window still reaches the bottom.
                let is_at_bottom = !has_newer && self.inner.is_at_bottom;

                let inner = MessageListStateInner {
                    is_connection_chat: self.inner.is_connection_chat,
                    message_ids_index,
                    messages,
                    new_messages,
                    has_older,
                    has_newer,
                    is_at_bottom,
                    scroll_to_index: None,
                    first_unread_index,
                };
                self.inner = Arc::new(inner);
            }
            LoadDirection::AppendNewer { has_newer } => {
                let mut appended: Vec<UiChatMessage> =
                    new_messages.into_iter().map(From::from).collect();

                let mut messages: Vec<UiChatMessage> = self.inner.messages.to_vec();
                // Track which messages are new
                let mut new_message_ids = self.inner.new_messages.clone();
                for msg in &appended {
                    if !self.inner.message_ids_index.contains_key(&msg.id) {
                        new_message_ids.insert(msg.id);
                    }
                }

                // Compute positions for the new (appended) messages only
                compute_flight_positions(&mut appended, None);

                let old_count = messages.len();
                messages.extend(appended);

                // Recompute only the boundary: last existing + first appended
                let unread_idx = self.inner.first_unread_index;
                if old_count > 0 && messages.len() > old_count {
                    let boundary_start = old_count.saturating_sub(1);
                    let boundary_end = (old_count + 1).min(messages.len());
                    recompute_flight_positions_range(
                        &mut messages,
                        boundary_start,
                        boundary_end,
                        unread_idx,
                    );
                }

                // Evict older messages from the front if the window exceeds the cap
                let evict_count = messages.len().saturating_sub(MAX_WINDOW);
                let has_older = if evict_count > 0 {
                    messages.drain(..evict_count);
                    true
                } else {
                    self.inner.has_older
                };

                // Shift or invalidate the unread index after front eviction
                let first_unread_index = self
                    .inner
                    .first_unread_index
                    .and_then(|i| i.checked_sub(evict_count));

                let mut message_ids_index = HashMap::with_capacity(messages.len());
                for (i, msg) in messages.iter().enumerate() {
                    message_ids_index.insert(msg.id, i);
                }

                let is_at_bottom = !has_newer;

                let inner = MessageListStateInner {
                    is_connection_chat: self.inner.is_connection_chat,
                    message_ids_index,
                    messages,
                    new_messages: new_message_ids,
                    has_older,
                    has_newer,
                    is_at_bottom,
                    scroll_to_index: None,
                    first_unread_index,
                };
                self.inner = Arc::new(inner);
            }
        }
    }

    /// The number of loaded messages in the list
    ///
    /// Note that this is not the number of all messages in the chat.
    #[frb(sync, getter, type_64bit_int)]
    pub fn loaded_messages_count(&self) -> usize {
        self.inner.messages.len()
    }

    /// Returns the message at the given index.
    #[frb(sync, type_64bit_int, positional)]
    pub fn message_at(&self, index: usize) -> Option<UiChatMessage> {
        self.inner.messages.get(index).cloned()
    }

    /// Returns the lookup table mapping a message id to the index in the list.
    #[frb(sync, type_64bit_int, positional)]
    pub fn message_id_index(&self, message_id: MessageId) -> Option<usize> {
        self.inner.message_ids_index.get(&message_id).copied()
    }

    #[frb(sync, positional)]
    pub fn is_new_message(&self, message_id: MessageId) -> bool {
        self.inner.new_messages.contains(&message_id)
    }

    #[frb(sync, getter)]
    pub fn meta(&self) -> MessageListMeta {
        MessageListMeta {
            is_connection_chat: self.inner.is_connection_chat,
            has_older: self.inner.has_older,
            has_newer: self.inner.has_newer,
            is_at_bottom: self.inner.is_at_bottom,
            scroll_to_index: self.inner.scroll_to_index,
            first_unread_index: self.inner.first_unread_index,
        }
    }
}

/// Provides access to the list of messages in a chat.
///
/// Loads messages in pages of ~50 ([#287]).
///
/// [#287]: https://github.com/phnx-im/air/issues/287
#[frb(opaque)]
pub struct MessageListCubitBase {
    core: CubitCore<MessageListState>,
    commands_tx: mpsc::Sender<Command>,
}

#[frb(ignore)]
enum Command {
    LoadOlder,
    LoadNewer,
    JumpToBottom,
    JumpToMessage(MessageId),
}

impl MessageListCubitBase {
    #[frb(sync)]
    pub fn new(user_cubit: &UserCubitBase, chat_id: ChatId) -> Self {
        let store = user_cubit.core_user().clone();
        let store_notifications = store.subscribe();

        let core = CubitCore::new();
        let (commands_tx, commands_rx) = mpsc::channel(4);

        MessageListContext::new(store, core.state_tx().clone(), chat_id.into(), commands_rx)
            .spawn(store_notifications, core.cancellation_token().clone());

        Self { core, commands_tx }
    }

    /// Request loading of older messages (prepend to window).
    pub fn load_older(&self) {
        let _ = self.commands_tx.try_send(Command::LoadOlder);
    }

    /// Request loading of newer messages (append to window).
    pub fn load_newer(&self) {
        let _ = self.commands_tx.try_send(Command::LoadNewer);
    }

    /// Jump to the most recent messages.
    pub fn jump_to_bottom(&self) {
        let _ = self.commands_tx.try_send(Command::JumpToBottom);
    }

    /// Jump to a specific message (loads a window around it if not in view).
    pub fn jump_to_message(&self, message_id: MessageId) {
        let _ = self
            .commands_tx
            .try_send(Command::JumpToMessage(message_id));
    }

    // Cubit interface

    #[frb(getter, sync)]
    pub fn is_closed(&self) -> bool {
        self.core.is_closed()
    }

    pub fn close(&mut self) {
        self.core.close();
    }

    #[frb(getter, sync)]
    pub fn state(&self) -> MessageListState {
        self.core.state()
    }

    pub async fn stream(&mut self, sink: StreamSink<MessageListState>) {
        self.core.stream(sink).await;
    }
}

/// Loads the initial state and listens to changes in a background task.
#[frb(ignore)]
struct MessageListContext<S> {
    store: S,
    state_tx: watch::Sender<MessageListState>,
    chat_id: ChatId,
    commands_rx: mpsc::Receiver<Command>,
}

impl<S: Store + Send + Sync + 'static> MessageListContext<S> {
    fn new(
        store: S,
        state_tx: watch::Sender<MessageListState>,
        chat_id: ChatId,
        commands_rx: mpsc::Receiver<Command>,
    ) -> Self {
        Self {
            store,
            state_tx,
            chat_id,
            commands_rx,
        }
    }

    fn spawn(
        mut self,
        store_notifications: impl Stream<Item = Arc<StoreNotification>> + Send + Unpin + 'static,
        stop: CancellationToken,
    ) {
        spawn_from_sync(async move {
            self.initial_load().await;
            self.run_loop(store_notifications, stop).await;
        });
    }

    // -- Initial load --

    async fn initial_load(&self) {
        let is_connection_chat = self.load_is_connection_chat().await;

        // Try to find the first unread message
        let first_unread = self
            .store
            .first_unread_message(self.chat_id)
            .await
            .inspect_err(|error| {
                error!(chat_id =% self.chat_id, %error, "Failed to load first unread message");
            })
            .ok()
            .flatten();

        if let Some(unread) = first_unread {
            let unread_ts = unread.timestamp().into();
            let unread_id = unread.id();
            let (messages, has_older, has_newer) = match self
                .store
                .messages_around(self.chat_id, unread_ts, unread_id, PAGE_SIZE)
                .await
            {
                Ok(result) => result,
                Err(error) => {
                    error!(chat_id =% self.chat_id, %error, "Failed to load messages around unread");
                    return;
                }
            };

            let scroll_to_index = messages.iter().position(|m| m.id() == unread_id);
            let first_unread_index = scroll_to_index;

            self.state_tx.send_modify(|state| {
                state.apply_messages(
                    messages,
                    is_connection_chat,
                    LoadDirection::Replace {
                        has_older,
                        has_newer,
                        is_at_bottom: !has_newer,
                        scroll_to_index,
                        first_unread_index,
                    },
                );
            });
        } else {
            // No unread messages: load from the bottom
            self.load_bottom(is_connection_chat, false).await;
        }
    }

    async fn load_bottom(&self, is_connection_chat: Option<bool>, scroll_to_bottom: bool) {
        let limit = PAGE_SIZE + 1;
        let messages = match self.store.messages(self.chat_id, limit).await {
            Ok(messages) => messages,
            Err(error) => {
                error!(chat_id =% self.chat_id, %error, "Failed to load messages");
                return;
            }
        };

        let has_older = messages.len() > PAGE_SIZE;
        let messages: Vec<ChatMessage> = if has_older {
            messages.into_iter().skip(1).collect()
        } else {
            messages
        };

        let scroll_to_index = if scroll_to_bottom {
            Some(messages.len().saturating_sub(1))
        } else {
            None
        };

        self.state_tx.send_modify(|state| {
            state.apply_messages(
                messages,
                is_connection_chat,
                LoadDirection::Replace {
                    has_older,
                    has_newer: false,
                    is_at_bottom: true,
                    scroll_to_index,
                    first_unread_index: None,
                },
            );
        });
    }

    async fn load_is_connection_chat(&self) -> Option<bool> {
        self.store
            .chat(self.chat_id)
            .await
            .inspect_err(|error| {
                error!(chat_id =% self.chat_id, %error, "Failed to load chat");
            })
            .ok()
            .flatten()
            .map(|chat| matches!(chat.chat_type(), ChatType::Connection(_)))
    }

    // -- Main loop --

    async fn run_loop(
        &mut self,
        mut store_notifications: impl Stream<Item = Arc<StoreNotification>> + Unpin,
        stop: CancellationToken,
    ) {
        loop {
            tokio::select! {
                _ = stop.cancelled() => return,
                cmd = self.commands_rx.recv() => {
                    match cmd {
                        Some(Command::LoadOlder) => self.handle_load_older().await,
                        Some(Command::LoadNewer) => self.handle_load_newer().await,
                        Some(Command::JumpToBottom) => self.handle_jump_to_bottom().await,
                        Some(Command::JumpToMessage(id)) => {
                            self.handle_jump_to_message(id).await;
                        }
                        None => return,
                    }
                }
                notification = store_notifications.next() => {
                    match notification {
                        Some(n) => self.process_store_notification(&n).await,
                        None => return,
                    }
                }
            }
        }
    }

    // -- Command handlers --

    async fn handle_load_older(&self) {
        let (oldest_ts, oldest_id) = {
            let state = self.state_tx.borrow();
            match state.inner.messages.first() {
                Some(msg) => (msg.timestamp.with_timezone(&chrono::Utc).into(), msg.id),
                None => return,
            }
        };

        let (messages, has_older) = match self
            .store
            .messages_before(self.chat_id, oldest_ts, oldest_id, PAGE_SIZE)
            .await
        {
            Ok(result) => result,
            Err(error) => {
                error!(chat_id =% self.chat_id, %error, "Failed to load older messages");
                return;
            }
        };

        if messages.is_empty() {
            // Still emit state to clear has_older so Flutter resets its load guard
            self.state_tx.send_modify(|state| {
                let new_inner = MessageListStateInner {
                    has_older,
                    ..(*state.inner).clone()
                };
                state.inner = Arc::new(new_inner);
            });
            return;
        }

        self.state_tx.send_modify(|state| {
            state.apply_messages(messages, None, LoadDirection::PrependOlder { has_older });
        });
    }

    async fn handle_load_newer(&self) {
        let (newest_ts, newest_id) = {
            let state = self.state_tx.borrow();
            match state.inner.messages.last() {
                Some(msg) => (msg.timestamp.with_timezone(&chrono::Utc).into(), msg.id),
                None => return,
            }
        };

        let (messages, has_newer) = match self
            .store
            .messages_after(self.chat_id, newest_ts, newest_id, PAGE_SIZE)
            .await
        {
            Ok(result) => result,
            Err(error) => {
                error!(chat_id =% self.chat_id, %error, "Failed to load newer messages");
                return;
            }
        };

        if messages.is_empty() {
            // Still emit state to clear has_newer so Flutter resets its load guard
            self.state_tx.send_modify(|state| {
                let new_inner = MessageListStateInner {
                    has_newer,
                    ..(*state.inner).clone()
                };
                state.inner = Arc::new(new_inner);
            });
            return;
        }

        self.state_tx.send_modify(|state| {
            state.apply_messages(messages, None, LoadDirection::AppendNewer { has_newer });
        });
    }

    async fn handle_jump_to_bottom(&self) {
        let is_connection_chat = self.load_is_connection_chat().await;
        self.load_bottom(is_connection_chat, true).await;
    }

    async fn handle_jump_to_message(&self, message_id: MessageId) {
        // Check if already in the loaded window
        let already_loaded = self
            .state_tx
            .borrow()
            .inner
            .message_ids_index
            .get(&message_id)
            .copied();

        if let Some(index) = already_loaded {
            self.state_tx.send_modify(|state| {
                let new_inner = MessageListStateInner {
                    scroll_to_index: Some(index),
                    ..(*state.inner).clone()
                };
                state.inner = Arc::new(new_inner);
            });
            return;
        }

        // Load the target message to get its timestamp
        let message = match self.store.message(message_id).await {
            Ok(Some(msg)) => msg,
            Ok(None) => {
                warn!(?message_id, "Jump target message not found");
                return;
            }
            Err(error) => {
                error!(?message_id, %error, "Failed to load jump target message");
                return;
            }
        };

        // Load a window around the target message
        let anchor_ts = message.timestamp().into();
        let anchor_id = message.id();
        let (messages, has_older, has_newer) = match self
            .store
            .messages_around(self.chat_id, anchor_ts, anchor_id, PAGE_SIZE)
            .await
        {
            Ok(result) => result,
            Err(error) => {
                error!(chat_id =% self.chat_id, %error, "Failed to load messages around target");
                return;
            }
        };

        let scroll_to_index = messages
            .iter()
            .position(|m| m.id() == message_id)
            .unwrap_or(0);

        let is_connection_chat = self.load_is_connection_chat().await;

        self.state_tx.send_modify(|state| {
            state.apply_messages(
                messages,
                is_connection_chat,
                LoadDirection::Replace {
                    has_older,
                    has_newer,
                    is_at_bottom: !has_newer,
                    scroll_to_index: Some(scroll_to_index),
                    first_unread_index: None,
                },
            );
        });
    }

    // -- Store notification handling --

    async fn process_store_notification(&self, notification: &StoreNotification) {
        if let Err(error) = self.try_process_store_notification(notification).await {
            error!(%error, "Failed to process store notification");
        }
    }

    async fn try_process_store_notification(
        &self,
        notification: &StoreNotification,
    ) -> anyhow::Result<()> {
        for (id, op) in &notification.ops {
            if let StoreEntityId::Message(message_id) = id {
                if op.contains(StoreOperation::Remove) {
                    let in_window = self
                        .state_tx
                        .borrow()
                        .inner
                        .message_ids_index
                        .contains_key(message_id);
                    if in_window {
                        self.notify_message_neighbors(*message_id);
                        // Reload from current position
                        self.reload_current_window().await;
                    }
                    return Ok(());
                }

                if op.contains(StoreOperation::Add) {
                    if let Some(message) = self.store.message(*message_id).await?
                        && message.chat_id() == self.chat_id
                    {
                        // Own message (not yet sent to server) clears the
                        // unread divider — the user has engaged with the chat.
                        if !message.is_sent() {
                            self.clear_first_unread_index();
                        }

                        let is_at_bottom = self.state_tx.borrow().inner.is_at_bottom;
                        if is_at_bottom {
                            self.handle_load_newer().await;
                            self.notify_message_neighbors(message.id());
                        }
                    }
                    return Ok(());
                }

                if op.contains(StoreOperation::Update) {
                    let in_window = self
                        .state_tx
                        .borrow()
                        .inner
                        .message_ids_index
                        .contains_key(message_id);
                    if in_window
                        && let Some(message) = self.store.message(*message_id).await?
                        && message.chat_id() == self.chat_id
                    {
                        self.update_message_in_place(message);
                    }
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    /// Clear the unread divider and recompute affected flight positions.
    fn clear_first_unread_index(&self) {
        self.state_tx.send_modify(|state| {
            let Some(unread_idx) = state.inner.first_unread_index else {
                return;
            };

            let mut messages = state.inner.messages.clone();

            // Recompute flight positions around the old divider boundary
            // (the divider acted as a flight break that is now removed).
            let start = unread_idx.saturating_sub(1);
            let end = (unread_idx + 1).min(messages.len());
            recompute_flight_positions_range(&mut messages, start, end, None);

            let new_inner = MessageListStateInner {
                messages,
                first_unread_index: None,
                ..(*state.inner).clone()
            };
            state.inner = Arc::new(new_inner);
        });
    }

    /// Update a single message in place and recompute its flight position + neighbors.
    fn update_message_in_place(&self, message: ChatMessage) {
        self.state_tx.send_modify(|state| {
            let Some(&idx) = state.inner.message_ids_index.get(&message.id()) else {
                return;
            };

            let mut messages = state.inner.messages.clone();
            messages[idx] = message.into();

            // Recompute flight positions for the updated message and its neighbors
            let start = idx.saturating_sub(1);
            let end = (idx + 2).min(messages.len());
            recompute_flight_positions_range(
                &mut messages,
                start,
                end,
                state.inner.first_unread_index,
            );

            let new_inner = MessageListStateInner {
                messages,
                ..(*state.inner).clone()
            };
            state.inner = Arc::new(new_inner);
        });
    }

    /// Reload the current window position (used after message deletion).
    async fn reload_current_window(&self) {
        let anchor = {
            let state = self.state_tx.borrow();
            let messages = &state.inner.messages;
            if messages.is_empty() {
                None
            } else {
                let mid = messages.len() / 2;
                let msg = &messages[mid];
                Some((msg.timestamp.with_timezone(&chrono::Utc).into(), msg.id))
            }
        };

        let Some((anchor_ts, anchor_id)) = anchor else {
            let is_cc = self.load_is_connection_chat().await;
            self.load_bottom(is_cc, false).await;
            return;
        };

        let (messages, has_older, has_newer) = match self
            .store
            .messages_around(self.chat_id, anchor_ts, anchor_id, PAGE_SIZE)
            .await
        {
            Ok(result) => result,
            Err(error) => {
                error!(chat_id =% self.chat_id, %error, "Failed to reload current window");
                return;
            }
        };

        let is_connection_chat = self.load_is_connection_chat().await;

        self.state_tx.send_modify(|state| {
            state.apply_messages(
                messages,
                is_connection_chat,
                LoadDirection::Replace {
                    has_older,
                    has_newer,
                    is_at_bottom: !has_newer,
                    scroll_to_index: None,
                    first_unread_index: None,
                },
            );
        });
    }

    /// Send update notifications to the neighbors of a message that was added or removed.
    ///
    /// The message must be present in the currently loaded state: for additions, call this after
    /// loading; for removals, call this before reloading.
    fn notify_message_neighbors(&self, message_id: MessageId) {
        let state = self.state_tx.borrow();
        let messages = &state.inner.messages;
        let Some(idx) = messages.iter().position(|m| m.id == message_id) else {
            return;
        };
        let mut notification = StoreNotification::default();
        if let Some(prev) = idx.checked_sub(1).and_then(|i| messages.get(i)) {
            notification.ops.insert(
                StoreEntityId::Message(prev.id),
                StoreOperation::Update.into(),
            );
        }
        if let Some(next) = messages.get(idx + 1) {
            notification.ops.insert(
                StoreEntityId::Message(next.id),
                StoreOperation::Update.into(),
            );
        }
        if !notification.ops.is_empty() {
            self.store.notify(notification);
        }
    }
}

impl Clone for MessageListStateInner {
    fn clone(&self) -> Self {
        Self {
            is_connection_chat: self.is_connection_chat,
            messages: self.messages.clone(),
            message_ids_index: self.message_ids_index.clone(),
            new_messages: self.new_messages.clone(),
            has_older: self.has_older,
            has_newer: self.has_newer,
            is_at_bottom: self.is_at_bottom,
            scroll_to_index: self.scroll_to_index,
            first_unread_index: self.first_unread_index,
        }
    }
}

#[cfg(test)]
mod tests {
    use aircommon::{identifiers::UserId, time::TimeStamp};
    use aircoreclient::{ContentMessage, MessageId};
    use mimi_content::MimiContent;
    use openmls::group::GroupId;
    use uuid::Uuid;

    use super::*;

    fn new_test_message(sender: &UserId, timestamp_secs: i64) -> ChatMessage {
        ChatMessage::new_for_test(
            ChatId::new(Uuid::from_u128(1)),
            MessageId::new(Uuid::from_u128(1)),
            TimeStamp::from(timestamp_secs * 1_000_000_000),
            ContentMessage::new(
                sender.clone(),
                true,
                MimiContent::simple_markdown_message("some content".into(), [0; 16]),
                &GroupId::from_slice(&[0]),
            ),
        )
    }

    #[test]
    fn test_flight_positions_replace() {
        use UiFlightPosition::*;

        let alice = UserId::random("localhost".parse().unwrap());
        let bob = UserId::random("localhost".parse().unwrap());

        let messages = vec![
            new_test_message(&alice, 0),
            new_test_message(&alice, 1),
            new_test_message(&alice, 2),
            // -- break due to sender
            new_test_message(&bob, 3),
            new_test_message(&bob, 4),
            new_test_message(&bob, 5),
            // -- break due to time
            new_test_message(&bob, 65),
            // -- break due to sender and time
            new_test_message(&alice, 125),
            new_test_message(&alice, 126),
        ];

        let mut state = MessageListState::default();
        state.apply_messages(
            messages.clone(),
            None,
            LoadDirection::Replace {
                has_older: false,
                has_newer: false,
                is_at_bottom: true,
                scroll_to_index: None,
                first_unread_index: None,
            },
        );

        let positions = state
            .inner
            .messages
            .iter()
            .map(|m| m.position)
            .collect::<Vec<_>>();
        assert_eq!(
            positions,
            [Start, Middle, End, Start, Middle, End, Single, Start, End]
        );
    }

    #[test]
    fn test_flight_positions_unread_break() {
        use UiFlightPosition::*;

        let alice = UserId::random("localhost".parse().unwrap());

        // All messages from the same sender within the time threshold —
        // normally a single flight, but the unread divider at index 2
        // should split it.
        let messages = vec![
            new_test_message(&alice, 0),
            new_test_message(&alice, 1),
            // -- unread divider here --
            new_test_message(&alice, 2),
            new_test_message(&alice, 3),
        ];

        let mut state = MessageListState::default();
        state.apply_messages(
            messages,
            None,
            LoadDirection::Replace {
                has_older: false,
                has_newer: false,
                is_at_bottom: true,
                scroll_to_index: None,
                first_unread_index: Some(2),
            },
        );

        let positions = state
            .inner
            .messages
            .iter()
            .map(|m| m.position)
            .collect::<Vec<_>>();
        assert_eq!(positions, [Start, End, Start, End]);
    }
}
