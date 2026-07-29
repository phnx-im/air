// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    identifiers::{MimiId, UserId},
    time::TimeStamp,
};
use anyhow::{Context, ensure};
use mimi_content::{MessageStatus, MimiContent};
use openmls::group::GroupId;

use crate::{
    ChatMessage, ContentMessage, MessageId, MimiContentExt,
    chats::StatusRecord,
    clients::attachment::AttachmentRecord,
    db::access::{WriteConnection, WriteDbTransaction},
};

pub(crate) struct MessageEdit<'a> {
    mimi_id: &'a MimiId,
    message_id: MessageId,
    created_at: TimeStamp,
    mimi_content: &'a MimiContent,
}

impl<'a> MessageEdit<'a> {
    pub(crate) fn new(
        mimi_id: &'a MimiId,
        message_id: MessageId,
        created_at: TimeStamp,
        mimi_content: &'a MimiContent,
    ) -> Self {
        Self {
            mimi_id,
            message_id,
            created_at,
            mimi_content,
        }
    }
}

/// Applies an incoming edit (or delete, indicated by a null part) to the
/// message identified by `replaces`.
///
/// Returns the updated message. The caller is responsible for persisting it.
pub(crate) async fn handle_message_edit(
    txn: &mut WriteDbTransaction<'_>,
    group_id: &GroupId,
    ds_timestamp: TimeStamp,
    sender: &UserId,
    replaces: MimiId,
    content: MimiContent,
) -> anyhow::Result<ChatMessage> {
    let is_delete = content.nested_part.is_null_part();

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

mod persistence {
    use aircommon::codec::BlobEncoded;
    use sqlx::{query, query_scalar};

    use crate::{
        chats::messages::persistence::VersionedMessage,
        db::access::{ReadConnection, WriteConnection},
    };

    use super::*;

    impl MessageEdit<'_> {
        pub(crate) async fn store(
            &self,
            mut connection: impl WriteConnection,
        ) -> anyhow::Result<()> {
            let versioned_message =
                BlobEncoded(VersionedMessage::from_mimi_content(self.mimi_content)?);
            query!(
                "INSERT INTO message_edit (
                    mimi_id,
                    message_id,
                    created_at,
                    content
                ) VALUES (?, ?, ?, ?)",
                self.mimi_id,
                self.message_id,
                self.created_at,
                versioned_message,
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }

        pub(crate) async fn find_message_id(
            mut connection: impl ReadConnection,
            mimi_id: &MimiId,
        ) -> sqlx::Result<Option<MessageId>> {
            query_scalar!(
                r#"SELECT message_id AS "message_id: _"
                FROM message_edit
                WHERE mimi_id = ?"#,
                mimi_id,
            )
            .fetch_optional(connection.as_mut())
            .await
        }

        /// Delete all edit history for a message.
        pub(crate) async fn delete_by_message_id(
            mut connection: impl WriteConnection,
            message_id: MessageId,
        ) -> sqlx::Result<()> {
            query!("DELETE FROM message_edit WHERE message_id = ?", message_id,)
                .execute(connection.as_mut())
                .await?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use aircommon::identifiers::MimiId;
    use mimi_content::MimiContent;
    use sqlx::SqlitePool;

    use crate::{
        MessageId,
        chats::{messages::persistence::tests::test_chat_message, persistence::tests::test_chat},
        db::access::{DbAccess, WriteConnection},
    };

    use super::*;

    #[sqlx::test]
    async fn delete_edit_history_by_message_id(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        let chat = test_chat();
        chat.store(pool.write().await?).await?;

        let message = test_chat_message(chat.id());
        message.store(pool.write().await?).await?;

        // Create multiple edit history entries
        let mimi_id_1 = MimiId::from_slice(&[1u8; 32])?;
        let mimi_id_2 = MimiId::from_slice(&[2u8; 32])?;
        let edit_content_1 =
            MimiContent::simple_markdown_message("First edit".to_string(), [1; 16]);
        let edit_content_2 =
            MimiContent::simple_markdown_message("Second edit".to_string(), [2; 16]);

        let edit_1 = MessageEdit::new(&mimi_id_1, message.id(), TimeStamp::now(), &edit_content_1);
        let edit_2 = MessageEdit::new(&mimi_id_2, message.id(), TimeStamp::now(), &edit_content_2);
        edit_1.store(pool.write().await?).await?;
        edit_2.store(pool.write().await?).await?;

        // Verify edit history exists
        assert_eq!(
            MessageEdit::find_message_id(pool.read().await?, &mimi_id_1).await?,
            Some(message.id())
        );
        assert_eq!(
            MessageEdit::find_message_id(pool.read().await?, &mimi_id_2).await?,
            Some(message.id())
        );

        // Delete edit history by message ID
        MessageEdit::delete_by_message_id(pool.write().await?, message.id()).await?;

        // Verify edit history is gone
        assert!(
            MessageEdit::find_message_id(pool.read().await?, &mimi_id_1)
                .await?
                .is_none()
        );
        assert!(
            MessageEdit::find_message_id(pool.read().await?, &mimi_id_2)
                .await?
                .is_none()
        );

        // Verify message still exists
        let loaded = crate::ChatMessage::load(pool.read().await?, message.id()).await?;
        assert!(loaded.is_some());

        Ok(())
    }

    #[sqlx::test]
    async fn delete_edit_history_nonexistent_message(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        let chat = test_chat();
        chat.store(pool.write().await?).await?;

        // Try to delete edit history for a nonexistent message
        let fake_message_id = MessageId::random();
        let result = MessageEdit::delete_by_message_id(pool.write().await?, fake_message_id).await;

        // Should succeed without error (idempotent operation)
        assert!(result.is_ok());

        Ok(())
    }

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
            .map(|mimi_id| mimi_id.as_slice().to_vec());
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
            .map(|mimi_id| mimi_id.as_slice().to_vec());
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
            .map(|mimi_id| mimi_id.as_slice().to_vec());
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

        // Alice edits her message -- the MIMI ID changes
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
        bob_mimi_content.in_reply_to = Some(edited_alice_mimi_id.as_slice().to_vec());
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
