// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Recovery of attachment messages left behind by a killed upload.
//!
//! Sending an attachment persists the message and the attachment record first,
//! uploads the content, and only then enqueues the message. A client killed
//! between those steps leaves the message unsent and unqueued. Text messages
//! cannot end up like that, because [`CoreUser::send_message`] enqueues in the
//! same transaction that stores the message.
//!
//! The share extension makes this likely, because the OS kills it for memory
//! routinely.
//!
//! [`CoreUser::send_message`]: crate::clients::CoreUser::send_message

use std::time::Duration;

use chrono::Utc;
use tracing::info;

use crate::{
    clients::attachment::{
        AttachmentRecord,
        persistence::{AttachmentStatus, UnqueuedAttachmentMessage},
    },
    db::access::{DbAccess, WriteConnection},
    outbound_service::chat_message_queue::ChatMessageQueue,
};

/// How long an attachment may stay in [`AttachmentStatus::Uploading`] before it
/// counts as abandoned.
///
/// An upload running in another process does not hold the global lock, so a
/// fresh record must be left alone. The share extension caps a shared file
/// well below what could legitimately take this long to upload.
const UPLOAD_STALE_AFTER: Duration = Duration::from_secs(30 * 60);

/// Picks up attachment messages that a killed upload left behind.
///
/// Runs under the global lock, so no other outbound service works on the same
/// database at the same time.
pub(super) async fn reconcile_attachment_messages(db: &DbAccess) -> anyhow::Result<()> {
    let stale_before = Utc::now() - UPLOAD_STALE_AFTER;
    db.with_write_transaction(async |txn| -> anyhow::Result<()> {
        let uploaded = AttachmentRecord::load_unqueued_uploaded_messages(&mut *txn).await?;
        for UnqueuedAttachmentMessage {
            chat_id,
            message_id,
        } in uploaded
        {
            info!(
                ?message_id,
                "Enqueueing an attachment message whose upload finished without a send"
            );
            ChatMessageQueue::new(chat_id, message_id)
                .enqueue(&mut *txn)
                .await?;
        }

        let abandoned = AttachmentRecord::load_stale_uploading(&mut *txn, stale_before).await?;
        for attachment_id in abandoned {
            info!(
                ?attachment_id,
                "Failing an attachment whose upload was abandoned"
            );
            // `UploadFailed` is the only status the retry API accepts, so this
            // is what turns a stuck spinner into a retry.
            AttachmentRecord::update_status(
                &mut *txn,
                attachment_id,
                AttachmentStatus::UploadFailed,
            )
            .await?;
            txn.notifier().update(attachment_id);
        }

        Ok(())
    })
    .await
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use sqlx::SqlitePool;
    use uuid::Uuid;

    use crate::{
        AttachmentId, ChatMessage, MessageId,
        chats::{
            messages::persistence::tests::test_chat_message_with_salt,
            persistence::tests::test_chat,
        },
        clients::attachment::{
            AttachmentRecord,
            persistence::{AttachmentStatus, test::test_attachment_record_with},
        },
        db::access::DbAccess,
        outbound_service::chat_message_queue::ChatMessageQueue,
    };

    use super::{UPLOAD_STALE_AFTER, reconcile_attachment_messages};

    /// Stores a chat with one unsent message and an attachment in the state a
    /// kill at the given point of the upload leaves behind.
    async fn unsent_attachment_message(
        db: &DbAccess,
        status: AttachmentStatus,
        created_at: DateTime<Utc>,
    ) -> anyhow::Result<(ChatMessage, AttachmentId)> {
        let chat = test_chat();
        chat.store(db.write().await?).await?;

        let message = test_chat_message_with_salt(chat.id(), [1; 16]);
        message.store(db.write().await?).await?;

        let record = test_attachment_record_with(chat.id(), message.id(), status, created_at);
        let attachment_id = record.attachment_id;
        record.store(db.write().await?, Some(b"content")).await?;

        Ok((message, attachment_id))
    }

    /// Drains the queue, using a single task id so each entry is returned once.
    async fn queued_message_ids(db: &DbAccess) -> anyhow::Result<Vec<MessageId>> {
        let task_id = Uuid::new_v4();
        let mut message_ids = Vec::new();
        while let Some((_, message_id)) = db
            .with_write_transaction(async |txn| ChatMessageQueue::dequeue(txn, task_id).await)
            .await?
        {
            message_ids.push(message_id);
        }
        Ok(message_ids)
    }

    async fn stored_status(
        db: &DbAccess,
        attachment_id: AttachmentId,
    ) -> anyhow::Result<Option<AttachmentStatus>> {
        Ok(AttachmentRecord::status(db.read().await?, attachment_id).await?)
    }

    fn abandoned_at() -> DateTime<Utc> {
        Utc::now() - UPLOAD_STALE_AFTER - chrono::Duration::minutes(1)
    }

    /// Killed after the upload, before the enqueue: the content is on the
    /// server, so only the send was lost.
    #[sqlx::test]
    async fn uploaded_message_is_enqueued(pool: SqlitePool) -> anyhow::Result<()> {
        let db = DbAccess::for_tests(pool);
        let (message, attachment_id) =
            unsent_attachment_message(&db, AttachmentStatus::Ready, Utc::now()).await?;

        reconcile_attachment_messages(&db).await?;

        assert_eq!(queued_message_ids(&db).await?, vec![message.id()]);
        assert_eq!(
            stored_status(&db, attachment_id).await?,
            Some(AttachmentStatus::Ready)
        );

        Ok(())
    }

    /// A message already queued must not be enqueued a second time.
    #[sqlx::test]
    async fn queued_message_is_left_alone(pool: SqlitePool) -> anyhow::Result<()> {
        let db = DbAccess::for_tests(pool);
        let (message, _) =
            unsent_attachment_message(&db, AttachmentStatus::Ready, Utc::now()).await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<_> {
            ChatMessageQueue::new(message.chat_id(), message.id())
                .enqueue(&mut *txn)
                .await?;
            Ok(())
        })
        .await?;

        reconcile_attachment_messages(&db).await?;

        assert_eq!(queued_message_ids(&db).await?, vec![message.id()]);

        Ok(())
    }

    /// Killed during the upload: the record would stay `Uploading` forever, and
    /// only `UploadFailed` gives the UI a retry.
    #[sqlx::test]
    async fn abandoned_upload_is_marked_as_failed(pool: SqlitePool) -> anyhow::Result<()> {
        let db = DbAccess::for_tests(pool);
        let (_, attachment_id) =
            unsent_attachment_message(&db, AttachmentStatus::Uploading, abandoned_at()).await?;

        reconcile_attachment_messages(&db).await?;

        assert_eq!(
            stored_status(&db, attachment_id).await?,
            Some(AttachmentStatus::UploadFailed)
        );
        // The content never reached the server, so there is nothing to send.
        assert!(queued_message_ids(&db).await?.is_empty());

        Ok(())
    }

    /// An upload still running in the share extension does not hold the global
    /// lock, so a fresh record must survive the pass.
    #[sqlx::test]
    async fn running_upload_is_left_alone(pool: SqlitePool) -> anyhow::Result<()> {
        let db = DbAccess::for_tests(pool);
        let (_, attachment_id) =
            unsent_attachment_message(&db, AttachmentStatus::Uploading, Utc::now()).await?;

        reconcile_attachment_messages(&db).await?;

        assert_eq!(
            stored_status(&db, attachment_id).await?,
            Some(AttachmentStatus::Uploading)
        );

        Ok(())
    }

    /// A message the outbound service already gave up on stays failed instead
    /// of being retried forever.
    #[sqlx::test]
    async fn failed_message_is_not_resurrected(pool: SqlitePool) -> anyhow::Result<()> {
        let db = DbAccess::for_tests(pool);
        let (message, _) =
            unsent_attachment_message(&db, AttachmentStatus::Ready, Utc::now()).await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<_> {
            ChatMessageQueue::new(message.chat_id(), message.id())
                .enqueue(&mut *txn)
                .await?;
            ChatMessageQueue::new(message.chat_id(), message.id())
                .remove_and_mark_as_failed(txn)
                .await?;
            Ok(())
        })
        .await?;

        reconcile_attachment_messages(&db).await?;

        assert!(queued_message_ids(&db).await?.is_empty());

        Ok(())
    }
}
