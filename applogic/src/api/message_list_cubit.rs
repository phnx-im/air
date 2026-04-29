// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! A list of messages feature

use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

use aircoreclient::{
    ChatId, ChatMessage, ChatType, MessageId,
    store::{Store, StoreEntityId, StoreNotification, StoreOperation},
};
use flutter_rust_bridge::frb;
use tokio::sync::{Notify, broadcast, mpsc, watch};
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
/// When a prepend/append would exceed this, messages are dropped from the far
/// end. With anchored rendering we can retain a larger buffer to make long
/// reverse-direction scrolls smoother before the window has to shift.
const MAX_WINDOW: usize = PAGE_SIZE * 10;

/// The state representing a list of messages in a chat
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[frb(dart_metadata = ("freezed"), type_64bit_int)]
pub struct MessageListState {
    /// Whether the chat the messages are in is a connection chat
    pub is_connection_chat: Option<bool>,
    /// More messages exist before the loaded window
    pub has_older: bool,
    /// More messages exist after the loaded window
    pub has_newer: bool,
    /// Whether the window is anchored at the most recent messages
    pub is_at_bottom: bool,
    /// Index of the first unread message (set on initial load only)
    pub first_unread_index: Option<usize>,
    /// Monotonic revision incremented for every emitted transition.
    pub revision: usize,
}

/// Why a message-list transition was emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[frb(dart_metadata = ("freezed"))]
pub enum MessageListTransitionKind {
    WindowReplaced,
    OlderPageLoaded,
    NewerPageLoaded,
    MessageUpdated,
    MessageDeleted,
    UnreadBoundaryChanged,
    MetaUpdated,
    CommandIssued,
}

/// A scroll/navigation command for the message list UI.
#[derive(Debug, Clone, PartialEq, Eq)]
#[frb(dart_metadata = ("freezed"))]
pub enum MessageListCommand {
    ScrollToId { message_id: MessageId },
    ScrollToBottom,
}

/// A concrete list change in AnchoredList order (index 0 = newest).
#[derive(Debug, Clone)]
#[frb(dart_metadata = ("freezed"), type_64bit_int)]
pub enum MessageListChange {
    /// Replace the entire list.
    Reload { messages: Vec<UiChatMessage> },
    /// Delete `delete_count` items at `index`, then insert `messages`.
    Splice {
        index: usize,
        messages: Vec<UiChatMessage>,
        delete_count: usize,
    },
    /// Replace the item at `index` with `message`.
    Patch {
        index: usize,
        message: UiChatMessage,
    },
}

/// A Rust-authored transition that Dart applies incrementally to the
/// anchored-list render cache.
#[derive(Debug, Clone)]
#[frb(dart_metadata = ("freezed"), type_64bit_int)]
pub struct MessageListTransition {
    pub revision: usize,
    pub kind: MessageListTransitionKind,
    pub changes: Vec<MessageListChange>,
    pub command: Option<MessageListCommand>,
}

#[frb(ignore)]
enum LoadDirection {
    /// Full replacement (initial load, jump-to-message, jump-to-bottom)
    Replace {
        has_older: bool,
        has_newer: bool,
        is_at_bottom: bool,
        first_unread_index: Option<usize>,
        command: Option<MessageListCommand>,
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

fn newest_first(messages: &[UiChatMessage]) -> Vec<UiChatMessage> {
    messages.iter().cloned().rev().collect()
}

fn newest_index(len: usize, oldest_index: usize) -> usize {
    len - 1 - oldest_index
}

fn rebuild_message_ids_index(data: &mut MessageListData) {
    data.message_ids_index.clear();
    for (i, msg) in data.messages.iter().enumerate() {
        data.message_ids_index.insert(msg.id, i);
    }
}

fn push_patch_changes(
    changes: &mut Vec<MessageListChange>,
    messages: &[UiChatMessage],
    indices: impl IntoIterator<Item = usize>,
) {
    let mut deduped = BTreeSet::new();
    for index in indices {
        if index < messages.len() {
            deduped.insert(index);
        }
    }

    let len = messages.len();
    for index in deduped {
        changes.push(MessageListChange::Patch {
            index: newest_index(len, index),
            message: messages[index].clone(),
        });
    }
}

#[frb(ignore)]
struct MessageListData {
    messages: Vec<UiChatMessage>,
    message_ids_index: HashMap<MessageId, usize>,
}

impl Default for MessageListData {
    fn default() -> Self {
        Self::with_page_capacity()
    }
}

impl MessageListData {
    fn with_page_capacity() -> Self {
        const CAPACITY: usize = PAGE_SIZE * 2 + 1;
        Self {
            messages: Vec::with_capacity(CAPACITY),
            message_ids_index: HashMap::with_capacity(CAPACITY),
        }
    }

    /// Apply new messages to the state according to the given direction.
    fn apply_messages(
        &mut self,
        state: &mut MessageListState,
        new_messages: Vec<ChatMessage>,
        is_connection_chat: Option<bool>,
        direction: LoadDirection,
    ) -> MessageListTransition {
        let mut changes = Vec::new();
        let mut command = None;
        let kind;

        match direction {
            LoadDirection::Replace {
                has_older,
                has_newer,
                is_at_bottom,
                first_unread_index,
                command: next_command,
            } => {
                let mut messages: Vec<UiChatMessage> =
                    new_messages.into_iter().map(From::from).collect();
                compute_flight_positions(&mut messages, first_unread_index);

                self.messages = messages;
                rebuild_message_ids_index(self);

                state.is_connection_chat = is_connection_chat.or(state.is_connection_chat);
                state.has_older = has_older;
                state.has_newer = has_newer;
                state.is_at_bottom = is_at_bottom;
                state.first_unread_index = first_unread_index;

                changes.push(MessageListChange::Reload {
                    messages: newest_first(&self.messages),
                });
                command = next_command;
                kind = MessageListTransitionKind::WindowReplaced;
            }
            LoadDirection::PrependOlder { has_older } => {
                let mut prepended: Vec<UiChatMessage> =
                    new_messages.into_iter().map(From::from).collect();
                let prepend_count = prepended.len();
                let old_len = self.messages.len();
                let inserted_messages = newest_first(&prepended);
                let shifted_unread = state.first_unread_index.map(|i| i + prepend_count);
                let mut patch_indices = Vec::new();

                compute_flight_positions(&mut prepended, None);
                self.messages.splice(0..0, prepended);

                if prepend_count > 0 && self.messages.len() > prepend_count {
                    let boundary_start = prepend_count.saturating_sub(1);
                    let boundary_end = (prepend_count + 1).min(self.messages.len());
                    recompute_flight_positions_range(
                        &mut self.messages,
                        boundary_start,
                        boundary_end,
                        shifted_unread,
                    );
                    patch_indices.extend(boundary_start..boundary_end);
                }

                let evict_count = self.messages.len().saturating_sub(MAX_WINDOW);
                if evict_count > 0 {
                    self.messages.truncate(MAX_WINDOW);
                    state.has_newer = true;
                    if let Some(last_index) = self.messages.len().checked_sub(1) {
                        let len = self.messages.len();
                        let unread_index = shifted_unread.filter(|&i| i < len);
                        recompute_flight_positions_range(
                            &mut self.messages,
                            last_index,
                            len,
                            unread_index,
                        );
                        patch_indices.push(last_index);
                    }
                }

                state.first_unread_index = shifted_unread.filter(|&i| i < self.messages.len());
                rebuild_message_ids_index(self);

                state.has_older = has_older;
                state.is_at_bottom = !state.has_newer && state.is_at_bottom;

                changes.push(MessageListChange::Splice {
                    index: old_len,
                    delete_count: 0,
                    messages: inserted_messages,
                });
                if evict_count > 0 {
                    changes.push(MessageListChange::Splice {
                        index: 0,
                        delete_count: evict_count,
                        messages: Vec::new(),
                    });
                }
                push_patch_changes(&mut changes, &self.messages, patch_indices);
                kind = MessageListTransitionKind::OlderPageLoaded;
            }
            LoadDirection::AppendNewer { has_newer } => {
                let mut appended: Vec<UiChatMessage> =
                    new_messages.into_iter().map(From::from).collect();
                let old_count = self.messages.len();
                let appended_count = appended.len();
                let inserted_messages = newest_first(&appended);
                let mut patch_indices = Vec::new();

                compute_flight_positions(&mut appended, None);
                self.messages.extend(appended);

                if old_count > 0 && self.messages.len() > old_count {
                    let boundary_start = old_count.saturating_sub(1);
                    let boundary_end = (old_count + 1).min(self.messages.len());
                    let unread_index = state.first_unread_index;
                    recompute_flight_positions_range(
                        &mut self.messages,
                        boundary_start,
                        boundary_end,
                        unread_index,
                    );
                    patch_indices.extend(boundary_start..boundary_end);
                }

                let evict_count = self.messages.len().saturating_sub(MAX_WINDOW);
                if evict_count > 0 {
                    self.messages.drain(0..evict_count);
                    state.has_older = true;
                    patch_indices = patch_indices
                        .into_iter()
                        .filter_map(|index| index.checked_sub(evict_count))
                        .collect();
                    if !self.messages.is_empty() {
                        recompute_flight_positions_range(
                            &mut self.messages,
                            0,
                            1,
                            state
                                .first_unread_index
                                .and_then(|index| index.checked_sub(evict_count)),
                        );
                        patch_indices.push(0);
                    }
                }

                state.first_unread_index = state
                    .first_unread_index
                    .and_then(|i| i.checked_sub(evict_count));
                rebuild_message_ids_index(self);

                state.has_newer = has_newer;
                state.is_at_bottom = !has_newer;

                changes.push(MessageListChange::Splice {
                    index: 0,
                    delete_count: 0,
                    messages: inserted_messages,
                });
                if evict_count > 0 {
                    changes.push(MessageListChange::Splice {
                        index: old_count + appended_count - evict_count,
                        delete_count: evict_count,
                        messages: Vec::new(),
                    });
                }
                push_patch_changes(&mut changes, &self.messages, patch_indices);
                kind = MessageListTransitionKind::NewerPageLoaded;
            }
        }

        state.revision += 1;

        MessageListTransition {
            revision: state.revision,
            kind,
            changes,
            command,
        }
    }

    fn clear_first_unread_index(
        &mut self,
        state: &mut MessageListState,
    ) -> Option<MessageListTransition> {
        let unread_idx = state.first_unread_index?;

        let start = unread_idx.saturating_sub(1);
        let end = (unread_idx + 1).min(self.messages.len());
        recompute_flight_positions_range(&mut self.messages, start, end, None);
        state.first_unread_index = None;

        let mut changes = Vec::new();
        push_patch_changes(&mut changes, &self.messages, start..end);

        state.revision += 1;
        Some(MessageListTransition {
            revision: state.revision,
            kind: MessageListTransitionKind::UnreadBoundaryChanged,
            changes,
            command: None,
        })
    }

    fn update_message_in_place(
        &mut self,
        state: &mut MessageListState,
        message: ChatMessage,
    ) -> Option<MessageListTransition> {
        let idx = self.message_ids_index.get(&message.id()).copied()?;
        let updated: UiChatMessage = message.into();
        if self.messages[idx] == updated {
            return None;
        }

        let start = idx.saturating_sub(1);
        let end = (idx + 2).min(self.messages.len());
        let unread_index = state.first_unread_index;

        self.messages[idx] = updated;
        recompute_flight_positions_range(&mut self.messages, start, end, unread_index);

        let mut changes = Vec::new();
        push_patch_changes(&mut changes, &self.messages, start..end);

        state.revision += 1;
        Some(MessageListTransition {
            revision: state.revision,
            kind: MessageListTransitionKind::MessageUpdated,
            changes,
            command: None,
        })
    }

    fn remove_message(
        &mut self,
        state: &mut MessageListState,
        message_id: MessageId,
    ) -> Option<MessageListTransition> {
        let idx = self.message_ids_index.get(&message_id).copied()?;
        let len_before = self.messages.len();

        self.messages.remove(idx);
        state.first_unread_index = match state.first_unread_index {
            Some(first_unread) if first_unread > idx => first_unread.checked_sub(1),
            Some(first_unread) if first_unread == idx => {
                if idx < self.messages.len() {
                    Some(idx)
                } else {
                    None
                }
            }
            other => other,
        };

        let start = idx.saturating_sub(1);
        let end = (idx + 1).min(self.messages.len());
        recompute_flight_positions_range(&mut self.messages, start, end, state.first_unread_index);
        rebuild_message_ids_index(self);

        let mut changes = vec![MessageListChange::Splice {
            index: newest_index(len_before, idx),
            delete_count: 1,
            messages: Vec::new(),
        }];
        push_patch_changes(&mut changes, &self.messages, start..end);

        state.revision += 1;
        Some(MessageListTransition {
            revision: state.revision,
            kind: MessageListTransitionKind::MessageDeleted,
            changes,
            command: None,
        })
    }

    fn issue_command(
        &mut self,
        state: &mut MessageListState,
        command: MessageListCommand,
    ) -> MessageListTransition {
        state.revision += 1;
        MessageListTransition {
            revision: state.revision,
            kind: MessageListTransitionKind::CommandIssued,
            changes: Vec::new(),
            command: Some(command),
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
    transitions_tx: broadcast::Sender<MessageListTransition>,
    subscribed: Arc<Notify>,
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
        let (transitions_tx, _) = broadcast::channel(64);

        let subscribed = Arc::new(Notify::new());

        MessageListContext::new(
            store,
            core.state_tx().clone(),
            transitions_tx.clone(),
            chat_id.into(),
            commands_rx,
            subscribed.clone(),
        )
        .spawn(store_notifications, core.cancellation_token().clone());

        Self {
            core,
            commands_tx,
            transitions_tx,
            subscribed,
        }
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

    pub fn close(&self) {
        self.core.close();
    }

    #[frb(getter, sync)]
    pub fn state(&self) -> MessageListState {
        self.core.state()
    }

    pub async fn stream(&self, sink: StreamSink<MessageListState>) {
        self.core.stream(sink).await;
    }

    pub async fn transitions(&self, sink: StreamSink<MessageListTransition>) {
        let mut rx = self.transitions_tx.subscribe();
        self.subscribed.notify_one();
        loop {
            match rx.recv().await {
                Ok(transition) => {
                    if sink.add(transition).is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(skipped = n, "Transition receiver lagged");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    }
}

/// Loads the initial state and listens to changes in a background task.
#[frb(ignore)]
struct MessageListContext<S> {
    store: S,
    state_tx: watch::Sender<MessageListState>,
    transitions_tx: broadcast::Sender<MessageListTransition>,
    chat_id: ChatId,
    commands_rx: mpsc::Receiver<Command>,
    subscribed: Arc<Notify>,
    data: MessageListData,
}

impl<S: Store + Send + Sync + 'static> MessageListContext<S> {
    fn new(
        store: S,
        state_tx: watch::Sender<MessageListState>,
        transitions_tx: broadcast::Sender<MessageListTransition>,
        chat_id: ChatId,
        commands_rx: mpsc::Receiver<Command>,
        subscribed: Arc<Notify>,
    ) -> Self {
        Self {
            store,
            state_tx,
            transitions_tx,
            chat_id,
            commands_rx,
            subscribed,
            data: Default::default(),
        }
    }

    fn spawn(
        mut self,
        store_notifications: impl Stream<Item = Arc<StoreNotification>> + Send + Unpin + 'static,
        stop: CancellationToken,
    ) {
        spawn_from_sync(async move {
            // Before loading the initial state, wait for the subscription. Otherwise, we might
            // emit state before Flutter has subscribed, and therefore lose it.
            self.subscribed.notified().await;
            self.initial_load().await;
            self.run_loop(store_notifications, stop).await;
        });
    }

    fn emit_state_and_transition(
        &self,
        new_state: MessageListState,
        transition: MessageListTransition,
    ) {
        debug_assert_eq!(new_state.revision, transition.revision);
        self.state_tx.send_modify(|state| *state = new_state);
        let _ = self.transitions_tx.send(transition);
    }

    // -- Initial load --

    async fn initial_load(&mut self) {
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

            let first_unread_index = messages.iter().position(|m| m.id() == unread_id);

            let mut state = self.state_tx.borrow().clone();
            let transition = self.data.apply_messages(
                &mut state,
                messages,
                is_connection_chat,
                LoadDirection::Replace {
                    has_older,
                    has_newer,
                    is_at_bottom: !has_newer,
                    first_unread_index,
                    command: None,
                },
            );
            self.emit_state_and_transition(state, transition);
        } else {
            // No unread messages: load from the bottom
            self.load_bottom(is_connection_chat, false).await;
        }
    }

    async fn load_bottom(&mut self, is_connection_chat: Option<bool>, scroll_to_bottom: bool) {
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

        let mut state = self.state_tx.borrow().clone();
        let transition = self.data.apply_messages(
            &mut state,
            messages,
            is_connection_chat,
            LoadDirection::Replace {
                has_older,
                has_newer: false,
                is_at_bottom: true,
                first_unread_index: None,
                command: scroll_to_bottom.then_some(MessageListCommand::ScrollToBottom),
            },
        );
        self.emit_state_and_transition(state, transition);
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

    async fn handle_load_older(&mut self) {
        let (oldest_ts, oldest_id) = match self.data.messages.first() {
            Some(msg) => (msg.timestamp.with_timezone(&chrono::Utc).into(), msg.id),
            None => return,
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

        let mut state = self.state_tx.borrow().clone();
        let transition = if messages.is_empty() {
            // Still emit state to clear has_older so Flutter resets its load guard
            state.has_older = has_older;
            state.revision += 1;
            MessageListTransition {
                revision: state.revision,
                kind: MessageListTransitionKind::MetaUpdated,
                changes: Vec::new(),
                command: None,
            }
        } else {
            self.data.apply_messages(
                &mut state,
                messages,
                None,
                LoadDirection::PrependOlder { has_older },
            )
        };

        self.emit_state_and_transition(state, transition);
    }

    async fn handle_load_newer(&mut self) {
        let (newest_ts, newest_id) = match self.data.messages.last() {
            Some(msg) => (msg.timestamp.with_timezone(&chrono::Utc).into(), msg.id),
            None => return,
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

        let mut state = self.state_tx.borrow().clone();
        let transition = if messages.is_empty() {
            // Still emit state to clear has_newer so Flutter resets its load guard
            state.has_newer = has_newer;
            state.revision += 1;
            MessageListTransition {
                revision: state.revision,
                kind: MessageListTransitionKind::MetaUpdated,
                changes: Vec::new(),
                command: None,
            }
        } else {
            self.data.apply_messages(
                &mut state,
                messages,
                None,
                LoadDirection::AppendNewer { has_newer },
            )
        };
        self.emit_state_and_transition(state, transition);
    }

    async fn handle_jump_to_bottom(&mut self) {
        let is_connection_chat = self.load_is_connection_chat().await;
        self.load_bottom(is_connection_chat, true).await;
    }

    async fn handle_jump_to_message(&mut self, message_id: MessageId) {
        // Check if already in the loaded window
        let already_loaded = self.data.message_ids_index.contains_key(&message_id);

        if already_loaded {
            let mut state = self.state_tx.borrow().clone();
            let transition = self
                .data
                .issue_command(&mut state, MessageListCommand::ScrollToId { message_id });
            self.emit_state_and_transition(state, transition);
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

        let is_connection_chat = self.load_is_connection_chat().await;

        let mut state = self.state_tx.borrow().clone();
        let transition = self.data.apply_messages(
            &mut state,
            messages,
            is_connection_chat,
            LoadDirection::Replace {
                has_older,
                has_newer,
                is_at_bottom: !has_newer,
                first_unread_index: None,
                command: Some(MessageListCommand::ScrollToId { message_id }),
            },
        );
        self.emit_state_and_transition(state, transition);
    }

    // -- Store notification handling --

    async fn process_store_notification(&mut self, notification: &StoreNotification) {
        if let Err(error) = self.try_process_store_notification(notification).await {
            error!(%error, "Failed to process store notification");
        }
    }

    async fn try_process_store_notification(
        &mut self,
        notification: &StoreNotification,
    ) -> anyhow::Result<()> {
        for (id, op) in &notification.ops {
            if let StoreEntityId::Message(message_id) = id {
                if op.contains(StoreOperation::Remove) {
                    let in_window = self.data.message_ids_index.contains_key(message_id);
                    if in_window {
                        self.remove_message_in_place(*message_id);
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

                        let is_at_bottom = self.state_tx.borrow().is_at_bottom;
                        if is_at_bottom {
                            self.handle_load_newer().await;
                        }
                    }
                    return Ok(());
                }

                if op.contains(StoreOperation::Update) {
                    let in_window = self.data.message_ids_index.contains_key(message_id);
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
    fn clear_first_unread_index(&mut self) {
        let mut state = self.state_tx.borrow().clone();
        if let Some(transition) = self.data.clear_first_unread_index(&mut state) {
            self.emit_state_and_transition(state, transition);
        }
    }

    /// Update a single message in place and recompute its flight position + neighbors.
    fn update_message_in_place(&mut self, message: ChatMessage) {
        let mut state = self.state_tx.borrow().clone();
        if let Some(transition) = self.data.update_message_in_place(&mut state, message) {
            self.emit_state_and_transition(state, transition);
        }
    }

    fn remove_message_in_place(&mut self, message_id: MessageId) {
        let mut state = self.state_tx.borrow().clone();
        if let Some(transition) = self.data.remove_message(&mut state, message_id) {
            self.emit_state_and_transition(state, transition);
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
        new_test_message_with_id(sender, timestamp_secs as u128 + 1, timestamp_secs)
    }

    fn new_test_message_with_id(
        sender: &UserId,
        message_id: u128,
        timestamp_secs: i64,
    ) -> ChatMessage {
        ChatMessage::new_for_test(
            ChatId::new(Uuid::from_u128(1)),
            MessageId::new(Uuid::from_u128(message_id)),
            TimeStamp::from(timestamp_secs * 1_000_000_000),
            ContentMessage::new(
                sender.clone(),
                true,
                MimiContent::simple_markdown_message("some content".into(), [0; 16]),
                &GroupId::from_slice(&[0]),
            ),
        )
    }

    fn ui_ids(messages: &[UiChatMessage]) -> Vec<MessageId> {
        messages.iter().map(|message| message.id).collect()
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
        let mut data = MessageListData::default();

        data.apply_messages(
            &mut state,
            messages.clone(),
            None,
            LoadDirection::Replace {
                has_older: false,
                has_newer: false,
                is_at_bottom: true,
                first_unread_index: None,
                command: None,
            },
        );

        let positions: Vec<_> = data.messages.iter().map(|m| m.position).collect();
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
        let mut data = MessageListData::default();

        data.apply_messages(
            &mut state,
            messages,
            None,
            LoadDirection::Replace {
                has_older: false,
                has_newer: false,
                is_at_bottom: true,
                first_unread_index: Some(2),
                command: None,
            },
        );

        let positions: Vec<_> = data.messages.iter().map(|m| m.position).collect();
        assert_eq!(positions, [Start, End, Start, End]);
    }

    #[test]
    fn test_replace_transition_reloads_newest_first() {
        let alice = UserId::random("localhost".parse().unwrap());
        let first = new_test_message_with_id(&alice, 1, 0);
        let second = new_test_message_with_id(&alice, 2, 1);

        let mut state = MessageListState::default();
        let mut data = MessageListData::default();

        let transition = data.apply_messages(
            &mut state,
            vec![first.clone(), second.clone()],
            None,
            LoadDirection::Replace {
                has_older: false,
                has_newer: false,
                is_at_bottom: true,
                first_unread_index: None,
                command: Some(MessageListCommand::ScrollToBottom),
            },
        );

        assert_eq!(transition.revision, 1);
        assert_eq!(state.revision, 1);
        assert_eq!(transition.kind, MessageListTransitionKind::WindowReplaced);
        assert_eq!(transition.command, Some(MessageListCommand::ScrollToBottom),);

        match transition.changes.as_slice() {
            [MessageListChange::Reload { messages }] => {
                assert_eq!(ui_ids(messages), vec![second.id(), first.id()]);
            }
            other => panic!("unexpected changes: {other:?}"),
        }
    }

    #[test]
    fn test_append_newer_emits_splice_and_boundary_patches() {
        use UiFlightPosition::*;

        let alice = UserId::random("localhost".parse().unwrap());
        let first = new_test_message_with_id(&alice, 1, 0);
        let second = new_test_message_with_id(&alice, 2, 1);
        let third = new_test_message_with_id(&alice, 3, 2);

        let mut state = MessageListState::default();
        let mut data = MessageListData::default();

        data.apply_messages(
            &mut state,
            vec![first, second.clone()],
            None,
            LoadDirection::Replace {
                has_older: false,
                has_newer: false,
                is_at_bottom: true,
                first_unread_index: None,
                command: None,
            },
        );

        let transition = data.apply_messages(
            &mut state,
            vec![third.clone()],
            None,
            LoadDirection::AppendNewer { has_newer: false },
        );

        assert_eq!(transition.revision, 2);
        assert_eq!(state.revision, 2);
        assert_eq!(transition.kind, MessageListTransitionKind::NewerPageLoaded);
        assert_eq!(
            data.messages.iter().map(|m| m.position).collect::<Vec<_>>(),
            vec![Start, Middle, End],
        );

        match &transition.changes[0] {
            MessageListChange::Splice {
                index,
                delete_count,
                messages,
            } => {
                assert_eq!(*index, 0);
                assert_eq!(*delete_count, 0);
                assert_eq!(ui_ids(messages), vec![third.id()]);
            }
            other => panic!("unexpected first change: {other:?}"),
        }

        let patch_indices = transition
            .changes
            .iter()
            .filter_map(|change| match change {
                MessageListChange::Patch { index, .. } => Some(*index),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(patch_indices, vec![1, 0]);
    }

    #[test]
    fn test_prepend_older_emits_splice_at_end_and_boundary_patches() {
        use UiFlightPosition::*;

        let alice = UserId::random("localhost".parse().unwrap());
        let first = new_test_message_with_id(&alice, 1, 0);
        let second = new_test_message_with_id(&alice, 2, 1);
        let third = new_test_message_with_id(&alice, 3, 2);

        let mut state = MessageListState::default();
        let mut data = MessageListData::default();

        data.apply_messages(
            &mut state,
            vec![second.clone(), third],
            None,
            LoadDirection::Replace {
                has_older: true,
                has_newer: false,
                is_at_bottom: true,
                first_unread_index: None,
                command: None,
            },
        );

        let transition = data.apply_messages(
            &mut state,
            vec![first.clone()],
            None,
            LoadDirection::PrependOlder { has_older: false },
        );

        assert_eq!(transition.revision, 2);
        assert_eq!(state.revision, 2);
        assert_eq!(transition.kind, MessageListTransitionKind::OlderPageLoaded);
        assert_eq!(
            data.messages.iter().map(|m| m.position).collect::<Vec<_>>(),
            vec![Start, Middle, End],
        );

        match &transition.changes[0] {
            MessageListChange::Splice {
                index,
                delete_count,
                messages,
            } => {
                assert_eq!(*index, 2);
                assert_eq!(*delete_count, 0);
                assert_eq!(ui_ids(messages), vec![first.id()]);
            }
            other => panic!("unexpected first change: {other:?}"),
        }

        let patch_indices = transition
            .changes
            .iter()
            .filter_map(|change| match change {
                MessageListChange::Patch { index, .. } => Some(*index),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(patch_indices, vec![2, 1]);
    }

    #[test]
    fn test_remove_message_emits_delete_splice_and_neighbor_patches() {
        use UiFlightPosition::*;

        let alice = UserId::random("localhost".parse().unwrap());
        let first = new_test_message_with_id(&alice, 1, 0);
        let second = new_test_message_with_id(&alice, 2, 1);
        let third = new_test_message_with_id(&alice, 3, 2);

        let mut state = MessageListState::default();
        let mut data = MessageListData::default();

        data.apply_messages(
            &mut state,
            vec![first.clone(), second.clone(), third.clone()],
            None,
            LoadDirection::Replace {
                has_older: false,
                has_newer: false,
                is_at_bottom: true,
                first_unread_index: None,
                command: None,
            },
        );

        let transition = data
            .remove_message(&mut state, second.id())
            .expect("message should exist");

        assert_eq!(transition.revision, 2);
        assert_eq!(state.revision, 2);
        assert_eq!(transition.kind, MessageListTransitionKind::MessageDeleted);
        assert_eq!(
            data.messages.iter().map(|m| m.position).collect::<Vec<_>>(),
            vec![Start, End],
        );

        match &transition.changes[0] {
            MessageListChange::Splice {
                index,
                delete_count,
                messages,
            } => {
                assert_eq!(*index, 1);
                assert_eq!(*delete_count, 1);
                assert!(messages.is_empty());
            }
            other => panic!("unexpected first change: {other:?}"),
        }

        let patched_ids = transition
            .changes
            .iter()
            .filter_map(|change| match change {
                MessageListChange::Patch { message, .. } => Some(message.id),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(patched_ids, vec![first.id(), third.id()]);
    }
}
