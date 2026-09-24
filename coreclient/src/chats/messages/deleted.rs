// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Synchronizing local message deletions ("delete for me") with the user's
//! other devices.
//!
//! Deleting a message locally erases it and parks its Mimi ID in the outbox.
//! The outbound service sends the parked IDs as a [`DeletedMessages`] self-group
//! application message, and the siblings erase their copy on receipt.

use aircommon::identifiers::MimiId;
use airprotos::client::self_group::DeletedMessages;
use tracing::debug;

use crate::{
    ChatId, ChatMessage,
    chats::{messages::edit::MessageEdit, reactions::Reaction},
    clients::self_group_outbox::{self, OutboxKind},
    db::access::{ReadConnection, WriteConnection, WriteDbTransaction},
};

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
pub(crate) async fn store_outgoing(
    connection: impl WriteConnection,
    mimi_id: &MimiId,
) -> sqlx::Result<()> {
    self_group_outbox::stage(
        connection,
        OutboxKind::DeletedMessage,
        mimi_id.as_slice(),
        &[],
        None,
    )
    .await
}

pub(crate) async fn staged(connection: impl ReadConnection) -> sqlx::Result<Vec<MimiId>> {
    Ok(
        self_group_outbox::load_kind(connection, OutboxKind::DeletedMessage)
            .await?
            .iter()
            .filter_map(|entry| MimiId::from_slice(&entry.key).ok())
            .collect(),
    )
}

/// Drops the parked deletions.
pub(crate) async fn remove_staged(
    mut connection: impl WriteConnection,
    mimi_ids: &[MimiId],
) -> sqlx::Result<()> {
    for mimi_id in mimi_ids {
        self_group_outbox::remove(
            &mut connection,
            OutboxKind::DeletedMessage,
            mimi_id.as_slice(),
        )
        .await?;
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
        if !chat_ids.contains(&chat_id) {
            chat_ids.push(chat_id);
        }
    }
    remove_staged(&mut *txn, &deleted.mimi_ids).await?;
    Ok(chat_ids)
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
    use aircommon::{identifiers::UserId, time::TimeStamp};
    use mimi_content::MimiContent;
    use openmls::group::GroupId;
    use sqlx::SqlitePool;

    use crate::{
        ContentMessage, Message, MessageId, chats::persistence::tests::test_chat,
        db::access::DbAccess,
    };

    use super::*;

    fn mimi_id(n: u8) -> MimiId {
        MimiId::from([n; 32])
    }

    fn deleted(mimi_ids: impl IntoIterator<Item = MimiId>) -> DeletedMessages {
        DeletedMessages {
            mimi_ids: mimi_ids.into_iter().collect(),
        }
    }

    fn received_message(chat_id: ChatId) -> ChatMessage {
        let content = MimiContent::simple_markdown_message("Hello".to_string(), [7; 16]);
        let message = ContentMessage::new(
            UserId::random("localhost".parse().unwrap()),
            true,
            content,
            &GroupId::from_slice(&[0]),
        );
        ChatMessage::new_for_test(
            chat_id,
            MessageId::random(),
            TimeStamp::now(),
            Message::Content(Box::new(message)),
        )
    }

    #[sqlx::test]
    async fn staged_is_sorted_and_deduplicated(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            store_outgoing(&mut *txn, &mimi_id(2)).await?;
            store_outgoing(&mut *txn, &mimi_id(1)).await?;
            store_outgoing(&mut *txn, &mimi_id(2)).await?;
            assert_eq!(staged(&mut *txn).await?, vec![mimi_id(1), mimi_id(2)]);

            remove_staged(&mut *txn, &[mimi_id(1)]).await?;
            assert_eq!(staged(&mut *txn).await?, vec![mimi_id(2)]);
            Ok(())
        })
        .await
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
                UserId::random("localhost".parse().unwrap()),
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

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            store_outgoing(&mut *txn, &mimi_id(1)).await?;
            store_outgoing(&mut *txn, &mimi_id(2)).await?;

            let changed = apply_deleted_messages(txn, &deleted([mimi_id(1)])).await?;

            assert!(changed.is_empty());
            assert_eq!(staged(&mut *txn).await?, vec![mimi_id(2)]);
            Ok(())
        })
        .await
    }
}
