// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Synchronizing local message deletions ("delete for me") with the user's
//! other devices.
//!
//! Deleting a message locally erases it and parks its ID in the outbox, the
//! Mimi ID of a content message or the event ID of a system message. The
//! outbound service sends the parked IDs as a [`DeletedMessages`] self-group
//! application message, and the siblings erase their copy on receipt.
//!
//! A sibling keeps the system messages of a contact request that is still open
//! there.

use aircommon::{
    identifiers::{EventId, MimiId},
    time::TimeStamp,
};
use airprotos::client::self_group::DeletedMessages;
use tracing::debug;

use crate::{
    ChatId, ChatMessage,
    chats::{messages::edit::MessageEdit, reactions::Reaction},
    clients::self_group_outbox::{self, OutboxKind},
    db::access::{ReadConnection, WriteConnection, WriteDbTransaction},
};

/// A deleted message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeletedMessageId {
    Content(MimiId),
    Event(EventId),
}

/// The outbox kinds that park deletions.
const OUTBOX_KINDS: [OutboxKind; 2] = [OutboxKind::DeletedMessage, OutboxKind::DeletedEvent];

impl DeletedMessageId {
    fn outbox_key(&self) -> (OutboxKind, &[u8]) {
        match self {
            DeletedMessageId::Content(mimi_id) => (OutboxKind::DeletedMessage, mimi_id.as_slice()),
            DeletedMessageId::Event(event_id) => (OutboxKind::DeletedEvent, event_id.as_slice()),
        }
    }

    fn from_outbox_key(kind: OutboxKind, key: &[u8]) -> Option<Self> {
        match kind {
            OutboxKind::DeletedMessage => MimiId::from_slice(key).ok().map(Self::Content),
            OutboxKind::DeletedEvent => EventId::from_slice(key).ok().map(Self::Event),
            OutboxKind::Settings
            | OutboxKind::BlockedContact
            | OutboxKind::DeletedChat
            | OutboxKind::ConnectionRequest
            | OutboxKind::Username => None,
        }
    }
}

/// The self-group message that tells the siblings about `ids`.
pub(crate) fn deleted_messages(ids: &[DeletedMessageId]) -> DeletedMessages {
    let mut deleted = DeletedMessages::default();
    for id in ids {
        match id {
            DeletedMessageId::Content(mimi_id) => deleted.mimi_ids.push(*mimi_id),
            DeletedMessageId::Event(event_id) => deleted.event_ids.push(*event_id),
        }
    }
    deleted
}

/// The IDs in the self-group message, the reverse of [`deleted_messages`].
fn deleted_message_ids(deleted: &DeletedMessages) -> Vec<DeletedMessageId> {
    let content = deleted
        .mimi_ids
        .iter()
        .copied()
        .map(DeletedMessageId::Content);
    let events = deleted
        .event_ids
        .iter()
        .copied()
        .map(DeletedMessageId::Event);
    content.chain(events).collect()
}

/// Erases the message and what hangs off it. Returns the chat it was in.
pub(crate) async fn erase(
    txn: &mut WriteDbTransaction<'_>,
    message: &ChatMessage,
) -> anyhow::Result<ChatId> {
    if let Some(mimi_id) = message.message().mimi_id() {
        // Replies render the message they quote, so they need a refresh.
        for reply_id in
            ChatMessage::load_message_ids_in_reply_to_mimi_id(&mut *txn, mimi_id).await?
        {
            txn.notifier().add(reply_id);
        }
    }

    // Reactions are keyed by Mimi ID and have no foreign key to the message.
    Reaction::delete_by_message_versions(&mut *txn, message.id(), message.message().mimi_id())
        .await?;
    // Edit history and status records are cascade-deleted.
    ChatMessage::delete(&mut *txn, message.id()).await?;
    Ok(message.chat_id())
}

/// Parks a local deletion for the siblings.
pub(crate) async fn store_outgoing_deletion(
    connection: impl WriteConnection,
    id: DeletedMessageId,
) -> sqlx::Result<()> {
    let (kind, key) = id.outbox_key();
    self_group_outbox::stage(connection, kind, key, &[], None).await
}

#[cfg(test)]
pub(crate) async fn staged_deletions(
    mut connection: impl ReadConnection,
) -> sqlx::Result<Vec<DeletedMessageId>> {
    let mut staged = Vec::new();
    for kind in OUTBOX_KINDS {
        for entry in self_group_outbox::load_kind(&mut connection, kind).await? {
            if let Some(id) = DeletedMessageId::from_outbox_key(kind, &entry.key) {
                staged.push(id);
            }
        }
    }
    Ok(staged)
}

/// The parked deletions that are due at `due_at`, with their failed attempts so
/// far.
pub(crate) async fn due_deletions(
    mut connection: impl ReadConnection,
    due_at: TimeStamp,
) -> sqlx::Result<Vec<(DeletedMessageId, u32)>> {
    let mut due = Vec::new();
    for kind in OUTBOX_KINDS {
        for entry in self_group_outbox::load_due(&mut connection, kind, due_at).await? {
            if let Some(id) = DeletedMessageId::from_outbox_key(kind, &entry.key) {
                due.push((id, entry.attempts));
            }
        }
    }
    Ok(due)
}

/// Keeps the parked deletions until `retry_at`.
pub(crate) async fn defer_deletions(
    mut connection: impl WriteConnection,
    ids: &[DeletedMessageId],
    attempts: u32,
    retry_at: TimeStamp,
) -> sqlx::Result<()> {
    for id in ids {
        let (kind, key) = id.outbox_key();
        self_group_outbox::record_failed_attempt(&mut connection, kind, key, attempts, retry_at)
            .await?;
    }
    Ok(())
}

/// Drops the parked deletions.
pub(crate) async fn remove_staged_deletion(
    mut connection: impl WriteConnection,
    ids: &[DeletedMessageId],
) -> sqlx::Result<()> {
    for id in ids {
        let (kind, key) = id.outbox_key();
        self_group_outbox::remove(&mut connection, kind, key).await?;
    }
    Ok(())
}

/// Erases the messages a sibling deleted. Returns the chats that lost a
/// message.
pub(crate) async fn apply_deleted_messages(
    txn: &mut WriteDbTransaction<'_>,
    deleted: &DeletedMessages,
) -> anyhow::Result<Vec<ChatId>> {
    let mut chat_ids = Vec::new();
    for mimi_id in &deleted.mimi_ids {
        let Some(message) = load_any_version(txn, mimi_id).await? else {
            debug!(?mimi_id, "Skipping deletion of an unknown message");
            continue;
        };
        let chat_id = erase(txn, &message).await?;
        add_chat(&mut chat_ids, chat_id);
    }
    for event_id in &deleted.event_ids {
        erase_event(txn, event_id, &mut chat_ids).await?;
    }
    remove_staged_deletion(&mut *txn, &deleted_message_ids(deleted)).await?;
    Ok(chat_ids)
}

/// Erases the copies of a deletable system message.
async fn erase_event(
    txn: &mut WriteDbTransaction<'_>,
    event_id: &EventId,
    chat_ids: &mut Vec<ChatId>,
) -> anyhow::Result<()> {
    for message_id in ChatMessage::load_ids_by_event_id(&mut *txn, event_id).await? {
        let Some(message) = ChatMessage::load(&mut *txn, message_id).await? else {
            continue;
        };
        if !message.is_deletable(&mut *txn).await? {
            debug!(?event_id, "Keeping a system message of an open request");
            continue;
        }
        let chat_id = erase(txn, &message).await?;
        add_chat(chat_ids, chat_id);
    }
    Ok(())
}

fn add_chat(chat_ids: &mut Vec<ChatId>, chat_id: ChatId) {
    if !chat_ids.contains(&chat_id) {
        chat_ids.push(chat_id);
    }
}

/// Loads the message by its current Mimi ID or one an edit superseded.
async fn load_any_version(
    txn: &mut WriteDbTransaction<'_>,
    mimi_id: &MimiId,
) -> anyhow::Result<Option<ChatMessage>> {
    if let Some(message) = ChatMessage::load_by_mimi_id(&mut *txn, mimi_id).await? {
        return Ok(Some(message));
    }
    let Some(message_id) = MessageEdit::find_message_id(&mut *txn, mimi_id).await? else {
        return Ok(None);
    };
    Ok(ChatMessage::load(&mut *txn, message_id).await?)
}

#[cfg(test)]
mod tests {
    use aircommon::{
        identifiers::{QualifiedGroupId, UserId, Username},
        time::TimeStamp,
    };
    use mimi_content::MimiContent;
    use openmls::group::GroupId;
    use sqlx::SqlitePool;
    use uuid::Uuid;

    use crate::{
        Chat, ChatType, ContentMessage, Message, MessageId, SystemMessage,
        chats::persistence::tests::test_chat, db::access::DbAccess,
    };

    use super::*;

    fn mimi_id(n: u8) -> MimiId {
        MimiId::from([n; 32])
    }

    fn deleted(mimi_ids: impl IntoIterator<Item = MimiId>) -> DeletedMessages {
        DeletedMessages {
            mimi_ids: mimi_ids.into_iter().collect(),
            ..Default::default()
        }
    }

    fn deleted_events(event_ids: impl IntoIterator<Item = EventId>) -> DeletedMessages {
        DeletedMessages {
            event_ids: event_ids.into_iter().collect(),
            ..Default::default()
        }
    }

    fn user() -> UserId {
        UserId::random("localhost".parse().unwrap())
    }

    fn received_message(chat_id: ChatId) -> ChatMessage {
        let content = MimiContent::simple_markdown_message("Hello".to_string(), [7; 16]);
        let message = ContentMessage::new(user(), true, content, &GroupId::from_slice(&[0]));
        ChatMessage::new_for_test(
            chat_id,
            MessageId::random(),
            TimeStamp::now(),
            Message::Content(Box::new(message)),
        )
    }

    /// Stores the system message in the chat and returns it with its event
    /// ID.
    async fn store_system_message(
        txn: &mut WriteDbTransaction<'_>,
        chat: &Chat,
        system_message: SystemMessage,
    ) -> anyhow::Result<(ChatMessage, EventId)> {
        let timestamp = TimeStamp::now();
        let event_id = system_message
            .event_id(chat.group_id(), timestamp)
            .expect("system message without an event ID");
        let message = ChatMessage::new_system_message(chat.id(), timestamp, system_message);
        message.store(&mut *txn).await?;
        Ok((message, event_id))
    }

    fn open_request_chat() -> Chat {
        let group_id = QualifiedGroupId::new(Uuid::new_v4(), "localhost".parse().unwrap());
        Chat::new_pending_connection_chat(group_id.into(), user())
    }

    #[sqlx::test]
    async fn staged_is_sorted_and_deduplicated(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let content = |n| DeletedMessageId::Content(mimi_id(n));
        let event = DeletedMessageId::Event(EventId::from([5; 32]));

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            store_outgoing_deletion(&mut *txn, content(2)).await?;
            store_outgoing_deletion(&mut *txn, event).await?;
            store_outgoing_deletion(&mut *txn, content(1)).await?;
            store_outgoing_deletion(&mut *txn, content(2)).await?;
            assert_eq!(
                staged_deletions(&mut *txn).await?,
                vec![content(1), content(2), event]
            );

            remove_staged_deletion(&mut *txn, &[content(1), event]).await?;
            assert_eq!(staged_deletions(&mut *txn).await?, vec![content(2)]);
            Ok(())
        })
        .await
    }

    #[test]
    fn deleted_messages_splits_the_ids_by_kind() {
        let event_id = EventId::from([5; 32]);
        let ids = [
            DeletedMessageId::Content(mimi_id(1)),
            DeletedMessageId::Event(event_id),
            DeletedMessageId::Content(mimi_id(2)),
        ];
        let deleted = deleted_messages(&ids);
        assert_eq!(
            deleted,
            DeletedMessages {
                mimi_ids: vec![mimi_id(1), mimi_id(2)],
                event_ids: vec![event_id],
            }
        );
        assert_eq!(deleted_message_ids(&deleted), [ids[0], ids[2], ids[1]]);
    }

    #[sqlx::test]
    async fn apply_erases_by_the_current_mimi_id(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let chat = test_chat();
        let message = received_message(chat.id());
        let current = *message.message().mimi_id().unwrap();

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            chat.store(&mut *txn).await?;
            message.store(&mut *txn).await?;
            let reaction = Reaction::new(
                mimi_id(9),
                current,
                chat.id(),
                user(),
                "👍".into(),
                TimeStamp::now(),
            );
            reaction.store(&mut *txn).await?;

            let changed = apply_deleted_messages(txn, &deleted([current])).await?;

            assert_eq!(changed, vec![chat.id()]);
            assert!(ChatMessage::load(&mut *txn, message.id()).await?.is_none());
            assert!(
                Reaction::load_by_target(&mut *txn, &current)
                    .await?
                    .is_empty()
            );
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn apply_erases_by_a_superseded_mimi_id(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let chat = test_chat();
        let message = received_message(chat.id());
        let superseded = mimi_id(1);
        let content = MimiContent::simple_markdown_message("Before".to_string(), [1; 16]);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            chat.store(&mut *txn).await?;
            message.store(&mut *txn).await?;
            MessageEdit::new(&superseded, message.id(), TimeStamp::now(), &content)
                .store(&mut *txn)
                .await?;

            let changed = apply_deleted_messages(txn, &deleted([superseded])).await?;

            assert_eq!(changed, vec![chat.id()]);
            assert!(ChatMessage::load(&mut *txn, message.id()).await?.is_none());
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn apply_skips_unknown_messages_and_drops_staged_ones(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let content = |n| DeletedMessageId::Content(mimi_id(n));

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            store_outgoing_deletion(&mut *txn, content(1)).await?;
            store_outgoing_deletion(&mut *txn, content(2)).await?;

            let changed = apply_deleted_messages(txn, &deleted([mimi_id(1)])).await?;

            assert!(changed.is_empty());
            assert_eq!(staged_deletions(&mut *txn).await?, vec![content(2)]);
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn apply_erases_a_system_message_by_its_event_id(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let chat = test_chat();

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            chat.store(&mut *txn).await?;
            let add = SystemMessage::Add(Some(user()), user());
            let (message, event_id) = store_system_message(txn, &chat, add).await?;
            store_outgoing_deletion(&mut *txn, DeletedMessageId::Event(event_id)).await?;

            let changed = apply_deleted_messages(txn, &deleted_events([event_id])).await?;

            assert_eq!(changed, vec![chat.id()]);
            assert!(ChatMessage::load(&mut *txn, message.id()).await?.is_none());
            assert!(staged_deletions(&mut *txn).await?.is_empty());
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn apply_keeps_the_messages_of_an_open_request(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let chat = open_request_chat();
        let ChatType::PendingConnection(sender) = chat.chat_type().clone() else {
            unreachable!()
        };

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            chat.store(&mut *txn).await?;
            let announcement = SystemMessage::ReceivedHandleConnectionRequest {
                sender,
                user_handle: Username::new("ellie-03".to_owned())?,
            };
            let (message, event_id) = store_system_message(txn, &chat, announcement).await?;

            let changed = apply_deleted_messages(txn, &deleted_events([event_id])).await?;

            assert!(changed.is_empty());
            assert!(ChatMessage::load(&mut *txn, message.id()).await?.is_some());
            Ok(())
        })
        .await
    }
}
