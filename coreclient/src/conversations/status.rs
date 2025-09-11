// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::borrow::Cow;

use aircommon::{identifiers::UserId, time::TimeStamp};
use mimi_content::MessageStatusReport;

pub(crate) struct StatusRecord<'a> {
    sender: Cow<'a, UserId>,
    report: MessageStatusReport,
    created_at: TimeStamp,
}

mod persistence {
    use std::collections::HashSet;

    use mimi_content::PerMessageStatus;
    use sqlx::{SqliteExecutor, SqliteTransaction, query, query_scalar};

    use crate::{ConversationMessageId, store::StoreNotifier};

    use super::*;

    impl<'a> StatusRecord<'a> {
        pub(crate) fn borrowed(
            sender: &'a UserId,
            report: MessageStatusReport,
            created_at: TimeStamp,
        ) -> Self {
            Self {
                sender: Cow::Borrowed(sender),
                report,
                created_at,
            }
        }

        pub(crate) async fn store_report(
            &self,
            txn: &mut SqliteTransaction<'_>,
            notifier: &mut StoreNotifier,
        ) -> sqlx::Result<()> {
            let sender_uuid = self.sender.uuid();
            let sender_domain = self.sender.domain();

            // Store the message status for each mimi id
            // Note: A user could send multiple status updates for the same message. The last one is the final status.
            let mut already_handled: HashSet<&[u8]> = HashSet::new();

            for PerMessageStatus { mimi_id, status } in self.report.statuses.iter().rev() {
                if already_handled.contains(mimi_id.as_slice()) {
                    continue;
                }
                already_handled.insert(mimi_id);

                // Load the message id
                let mimi_id = mimi_id.as_slice();
                let status = status.repr();
                let Some(message_id) = query_scalar!(
                    r#"SELECT message_id AS "message_id: ConversationMessageId"
                        FROM message
                        WHERE mimi_id = ?"#,
                    mimi_id,
                )
                .fetch_optional(&mut **txn)
                .await?
                else {
                    continue;
                };

                // Set the statuses for the message and user
                query!(
                    "INSERT INTO message_status
                        (message_id,  sender_user_uuid, sender_user_domain, status, created_at)
                    VALUES (?1, ?2, ?3, ?4, ?5)
                    ON CONFLICT (message_id, sender_user_domain, sender_user_uuid)
                    DO UPDATE SET status = ?4, created_at = ?5",
                    message_id,
                    sender_uuid,
                    sender_domain,
                    status,
                    self.created_at,
                )
                .execute(&mut **txn)
                .await?;

                // Now we go through statuses from all other users as well to build the final aggregated message status

                let final_status = query_scalar!(
                    "SELECT COALESCE(MAX(status), 0) AS max
                    FROM message_status
                    WHERE message_id = ?1 AND (status = 1 OR status = 2)",
                    message_id,
                )
                .fetch_one(&mut **txn)
                .await?;

                // Aggregate the status for the message
                query!(
                    "UPDATE message SET status = ?1 WHERE message_id = ?2",
                    final_status,
                    message_id,
                )
                .execute(&mut **txn)
                .await?;

                notifier.update(message_id);
            }

            Ok(())
        }

        pub(crate) async fn clear(
            txn: impl SqliteExecutor<'_>,
            notifier: &mut crate::store::StoreNotifier,
            message_id: crate::ConversationMessageId,
        ) -> sqlx::Result<()> {
            query!(
                "DELETE FROM message_status WHERE message_id = ?",
                message_id,
            )
            .execute(txn)
            .await?;
            notifier.update(message_id);
            Ok(())
        }
    }

    #[cfg(test)]
    mod test {
        use chrono::Utc;
        use mimi_content::MessageStatus;
        use sqlx::{SqlitePool, query_scalar};

        use crate::conversations::{
            messages::persistence::tests::test_conversation_message_with_salt,
            persistence::tests::test_conversation,
        };

        use super::*;

        #[sqlx::test]
        async fn store_report(pool: SqlitePool) -> anyhow::Result<()> {
            let mut notifier = StoreNotifier::noop();

            let alice = UserId::random("localhost".parse().unwrap());

            let conversation = test_conversation();
            conversation
                .store(pool.acquire().await?.as_mut(), &mut notifier)
                .await?;

            let message_a = test_conversation_message_with_salt(conversation.id(), [0; 16]);
            message_a.store(&pool, &mut notifier).await?;
            let mimi_id_a = message_a.message().mimi_id().unwrap();
            let message_b = test_conversation_message_with_salt(conversation.id(), [1; 16]);
            message_b.store(&pool, &mut notifier).await?;
            let mimi_id_b = message_b.message().mimi_id().unwrap();
            assert_ne!(mimi_id_a, mimi_id_b);

            let mut report = MessageStatusReport {
                statuses: Vec::new(),
            };

            report.statuses.push(PerMessageStatus {
                mimi_id: mimi_id_a.as_ref().to_vec().into(),
                status: MessageStatus::Delivered,
            });
            report.statuses.push(PerMessageStatus {
                mimi_id: mimi_id_a.as_ref().to_vec().into(),
                status: MessageStatus::Read,
            });
            report.statuses.push(PerMessageStatus {
                mimi_id: mimi_id_b.as_ref().to_vec().into(),
                status: MessageStatus::Deleted,
            });

            let mut txn = pool.begin().await?;
            StatusRecord::borrowed(&alice, report, Utc::now().into())
                .store_report(&mut txn, &mut notifier)
                .await?;
            txn.commit().await?;

            let status_a: i64 =
                query_scalar("SELECT status FROM message_status WHERE message_id = ?")
                    .bind(message_a.id())
                    .fetch_one(&mut *pool.acquire().await?)
                    .await?;

            let status_b: i64 =
                query_scalar("SELECT status FROM message_status WHERE message_id = ?")
                    .bind(message_b.id())
                    .fetch_one(&mut *pool.acquire().await?)
                    .await?;

            assert_eq!(status_a, i64::from(MessageStatus::Read.repr()));
            assert_eq!(status_b, i64::from(MessageStatus::Deleted.repr()));

            Ok(())
        }
    }
}
