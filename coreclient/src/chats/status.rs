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
    use std::collections::{HashMap, HashSet};

    use chrono::{DateTime, Utc};
    use mimi_content::{MessageStatus, PerMessageStatus};
    use sqlx::{query, query_scalar};

    use crate::{
        Chat, ChatId, MessageId,
        db::access::{WriteConnection, WriteDbTransaction},
    };

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
            txn: &mut WriteDbTransaction<'_>,
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
                let status: u8 = (*status).into();
                let Some(message_id) = query_scalar!(
                    r#"SELECT message_id AS "message_id: MessageId"
                        FROM message
                        WHERE mimi_id = ?"#,
                    mimi_id,
                )
                .fetch_optional(txn.as_mut())
                .await?
                else {
                    continue;
                };

                // Set the statuses for the message and user.
                //
                // A `Read` must never change: the row is shared by all of a
                // user's clients, and each of them reports `Delivered` for a
                // message it receives, possibly after a sibling read it.
                let read_status: u8 = MessageStatus::Read.into();
                let unread_status: u8 = MessageStatus::Unread.into();
                let delivered_status: u8 = MessageStatus::Delivered.into();
                query!(
                    "INSERT INTO message_status
                        (message_id,  sender_user_uuid, sender_user_domain, status, created_at)
                    VALUES (?1, ?2, ?3, ?4, ?5)
                    ON CONFLICT (message_id, sender_user_domain, sender_user_uuid)
                    DO UPDATE SET status = ?4, created_at = ?5
                    WHERE NOT (
                        message_status.status = ?6 AND (?4 = ?7 OR ?4 = ?8)
                    )",
                    message_id,
                    sender_uuid,
                    sender_domain,
                    status,
                    self.created_at,
                    read_status,
                    unread_status,
                    delivered_status,
                )
                .execute(txn.as_mut())
                .await?;

                // Now we go through statuses from all other users as well to build the final aggregated message status

                let final_status = query_scalar!(
                    "SELECT COALESCE(MAX(status), 0) AS max
                    FROM message_status
                    WHERE message_id = ?1 AND (status = 1 OR status = 2)",
                    message_id,
                )
                .fetch_one(txn.as_mut())
                .await?;

                // Aggregate the status for the message
                query!(
                    "UPDATE message SET status = ?1 WHERE message_id = ?2",
                    final_status,
                    message_id,
                )
                .execute(txn.as_mut())
                .await?;

                txn.notifier().update(message_id);
            }

            Ok(())
        }

        /// Advances the read marker of every chat this report covers.
        ///
        /// Only meaningful for a report sent by ourselves: a sibling client read
        /// those messages, so this client's marker follows.
        ///
        /// Returns the chats whose marker moved.
        pub(crate) async fn advance_read_markers(
            &self,
            txn: &mut WriteDbTransaction<'_>,
            own_user: &UserId,
        ) -> sqlx::Result<Vec<ChatId>> {
            // A report from anyone else carries their marker, not ours.
            if self.sender.as_ref() != own_user {
                return Ok(Vec::new());
            }

            let mut newest: HashMap<ChatId, (DateTime<Utc>, MessageId)> = HashMap::new();

            for PerMessageStatus { mimi_id, status } in &self.report.statuses {
                if *status != MessageStatus::Read {
                    continue;
                }
                // We want to find the most recent message_id from the report.
                let mimi_id = mimi_id.as_slice();
                let Some(message) = query!(
                    r#"SELECT
                        message_id AS "message_id: MessageId",
                        chat_id AS "chat_id: ChatId",
                        timestamp AS "timestamp: DateTime<Utc>"
                    FROM message
                    WHERE mimi_id = ?"#,
                    mimi_id,
                )
                .fetch_optional(txn.as_mut())
                .await?
                else {
                    continue;
                };

                // We update our map only if what we find is more recent
                // compared to what we have already seen.
                let candidate = (message.timestamp, message.message_id);
                newest
                    .entry(message.chat_id)
                    .and_modify(|current| {
                        if *current < candidate {
                            *current = candidate;
                        }
                    })
                    .or_insert(candidate);
            }

            let mut changed_chats = Vec::new();
            for (chat_id, (_, message_id)) in newest {
                let (marked, _) =
                    Chat::mark_as_read_until_message_id(txn, chat_id, message_id, own_user).await?;
                if marked {
                    changed_chats.push(chat_id);
                }
            }
            Ok(changed_chats)
        }

        pub(crate) async fn clear(
            mut connection: impl WriteConnection,
            message_id: crate::MessageId,
        ) -> sqlx::Result<()> {
            query!(
                "DELETE FROM message_status WHERE message_id = ?",
                message_id,
            )
            .execute(connection.as_mut())
            .await?;
            connection.notifier().update(message_id);
            Ok(())
        }
    }

    #[cfg(test)]
    mod test {
        use aircommon::identifiers::MimiId;
        use chrono::Utc;
        use mimi_content::MessageStatus;
        use sqlx::{SqlitePool, query_scalar};

        use crate::{
            chats::{
                messages::persistence::tests::{test_chat_message_at, test_chat_message_with_salt},
                persistence::tests::test_chat,
            },
            db::access::DbAccess,
        };

        use super::*;

        fn at(second: u32) -> DateTime<Utc> {
            format!("2026-01-01T00:00:{second:02}Z").parse().unwrap()
        }

        fn read_report(mimi_ids: impl IntoIterator<Item = MimiId>) -> MessageStatusReport {
            MessageStatusReport {
                statuses: mimi_ids
                    .into_iter()
                    .map(|mimi_id| PerMessageStatus {
                        mimi_id: mimi_id.as_ref().to_vec(),
                        status: MessageStatus::Read,
                    })
                    .collect(),
            }
        }

        #[sqlx::test]
        async fn store_report(pool: SqlitePool) -> anyhow::Result<()> {
            let pool = DbAccess::for_tests(pool);

            let alice = UserId::random("localhost".parse().unwrap());

            let chat = test_chat();
            chat.store(pool.write().await?).await?;

            let message_a = test_chat_message_with_salt(chat.id(), [0; 16]);
            message_a.store(pool.write().await?).await?;
            let mimi_id_a = message_a.message().mimi_id().unwrap();
            let message_b = test_chat_message_with_salt(chat.id(), [1; 16]);
            message_b.store(pool.write().await?).await?;
            let mimi_id_b = message_b.message().mimi_id().unwrap();
            assert_ne!(mimi_id_a, mimi_id_b);

            let mut report = MessageStatusReport {
                statuses: Vec::new(),
            };

            report.statuses.push(PerMessageStatus {
                mimi_id: mimi_id_a.as_ref().to_vec(),
                status: MessageStatus::Delivered,
            });
            report.statuses.push(PerMessageStatus {
                mimi_id: mimi_id_a.as_ref().to_vec(),
                status: MessageStatus::Read,
            });
            report.statuses.push(PerMessageStatus {
                mimi_id: mimi_id_b.as_ref().to_vec(),
                status: MessageStatus::Deleted,
            });

            let mut connection = pool.write().await?;
            let mut txn = connection.begin().await?;
            StatusRecord::borrowed(&alice, report, Utc::now().into())
                .store_report(&mut txn)
                .await?;
            txn.commit().await?;

            let status_a: u8 =
                query_scalar("SELECT status FROM message_status WHERE message_id = ?")
                    .bind(message_a.id())
                    .fetch_one(pool.read().await?.as_mut())
                    .await?;

            let status_b: u8 =
                query_scalar("SELECT status FROM message_status WHERE message_id = ?")
                    .bind(message_b.id())
                    .fetch_one(pool.read().await?.as_mut())
                    .await?;

            assert_eq!(status_a, u8::from(MessageStatus::Read));
            assert_eq!(status_b, u8::from(MessageStatus::Deleted));

            Ok(())
        }

        #[sqlx::test]
        async fn clear_status_records(pool: SqlitePool) -> anyhow::Result<()> {
            use crate::ChatMessage;

            let pool = DbAccess::for_tests(pool);

            let alice = UserId::random("localhost".parse().unwrap());
            let bob = UserId::random("localhost".parse().unwrap());

            let chat = test_chat();
            chat.store(pool.write().await?).await?;

            let message = test_chat_message_with_salt(chat.id(), [0; 16]);
            message.store(pool.write().await?).await?;
            let mimi_id = message.message().mimi_id().unwrap();

            // Create status records from multiple users
            let report_alice = MessageStatusReport {
                statuses: vec![PerMessageStatus {
                    mimi_id: mimi_id.as_ref().to_vec(),
                    status: MessageStatus::Delivered,
                }],
            };
            let report_bob = MessageStatusReport {
                statuses: vec![PerMessageStatus {
                    mimi_id: mimi_id.as_ref().to_vec(),
                    status: MessageStatus::Read,
                }],
            };

            let mut connection = pool.write().await?;
            let mut txn = connection.begin().await?;
            StatusRecord::borrowed(&alice, report_alice, Utc::now().into())
                .store_report(&mut txn)
                .await?;
            StatusRecord::borrowed(&bob, report_bob, Utc::now().into())
                .store_report(&mut txn)
                .await?;
            txn.commit().await?;

            // Verify status records exist
            let count: i64 =
                query_scalar("SELECT COUNT(*) FROM message_status WHERE message_id = ?")
                    .bind(message.id())
                    .fetch_one(pool.read().await?.as_mut())
                    .await?;
            assert_eq!(count, 2);

            // Clear status records
            StatusRecord::clear(pool.write().await?, message.id()).await?;

            // Verify status records are gone
            let count: i64 =
                query_scalar("SELECT COUNT(*) FROM message_status WHERE message_id = ?")
                    .bind(message.id())
                    .fetch_one(pool.read().await?.as_mut())
                    .await?;
            assert_eq!(count, 0);

            // Verify message still exists
            let loaded = ChatMessage::load(pool.read().await?, message.id()).await?;
            assert!(loaded.is_some());

            Ok(())
        }

        #[sqlx::test]
        async fn read_status_is_not_undone_by_a_later_delivered(
            pool: SqlitePool,
        ) -> anyhow::Result<()> {
            let pool = DbAccess::for_tests(pool);

            let own_user = UserId::random("localhost".parse().unwrap());

            let chat = test_chat();
            chat.store(pool.write().await?).await?;

            let message = test_chat_message_with_salt(chat.id(), [0; 16]);
            message.store(pool.write().await?).await?;
            let mimi_id = *message.message().mimi_id().unwrap();

            let delivered = MessageStatusReport {
                statuses: vec![PerMessageStatus {
                    mimi_id: mimi_id.as_ref().to_vec(),
                    status: MessageStatus::Delivered,
                }],
            };

            let mut connection = pool.write().await?;
            let mut txn = connection.begin().await?;
            StatusRecord::borrowed(&own_user, read_report([mimi_id]), Utc::now().into())
                .store_report(&mut txn)
                .await?;
            StatusRecord::borrowed(&own_user, delivered, Utc::now().into())
                .store_report(&mut txn)
                .await?;
            txn.commit().await?;

            let status: u8 = query_scalar("SELECT status FROM message_status WHERE message_id = ?")
                .bind(message.id())
                .fetch_one(pool.read().await?.as_mut())
                .await?;
            assert_eq!(status, u8::from(MessageStatus::Read));

            let aggregated: u8 = query_scalar("SELECT status FROM message WHERE message_id = ?")
                .bind(message.id())
                .fetch_one(pool.read().await?.as_mut())
                .await?;
            assert_eq!(aggregated, u8::from(MessageStatus::Read));

            Ok(())
        }

        #[sqlx::test]
        async fn advance_read_markers_follows_the_newest_read_message(
            pool: SqlitePool,
        ) -> anyhow::Result<()> {
            let pool = DbAccess::for_tests(pool);
            let mut connection = pool.write().await?;

            let own_user = UserId::random("localhost".parse().unwrap());

            let mut chat = test_chat();
            chat.last_read = at(0);
            chat.store(&mut connection).await?;

            let mut other_chat = test_chat();
            other_chat.last_read = at(0);
            other_chat.store(&mut connection).await?;

            let mut mimi_ids = Vec::new();
            for (chat_id, salt, second) in [
                (chat.id(), 0, 1),
                (chat.id(), 1, 2),
                (chat.id(), 2, 3),
                (other_chat.id(), 3, 1),
            ] {
                let message = test_chat_message_at(chat_id, [salt; 16], at(second).into());
                message.store(&mut connection).await?;
                mimi_ids.push(*message.message().mimi_id().unwrap());
            }
            let [first, second, _unread, other] = mimi_ids[..] else {
                unreachable!()
            };

            // Out of order and spanning both chats, the way a batched report
            // from a sibling arrives.
            let report = read_report([second, other, first]);

            let mut txn = connection.begin().await?;
            let mut changed_chats = StatusRecord::borrowed(&own_user, report, Utc::now().into())
                .advance_read_markers(&mut txn, &own_user)
                .await?;
            txn.commit().await?;

            changed_chats.sort();
            let mut expected = vec![chat.id(), other_chat.id()];
            expected.sort();
            assert_eq!(changed_chats, expected);

            let n = Chat::unread_messages_count(&mut connection, chat.id(), &own_user).await?;
            assert_eq!(
                n, 1,
                "only the message the sibling did not read stays unread"
            );

            let n =
                Chat::unread_messages_count(&mut connection, other_chat.id(), &own_user).await?;
            assert_eq!(n, 0);

            Ok(())
        }

        #[sqlx::test]
        async fn advance_read_markers_ignores_a_report_from_someone_else(
            pool: SqlitePool,
        ) -> anyhow::Result<()> {
            let pool = DbAccess::for_tests(pool);
            let mut connection = pool.write().await?;

            let own_user = UserId::random("localhost".parse().unwrap());
            let peer = UserId::random("localhost".parse().unwrap());

            let mut chat = test_chat();
            chat.last_read = at(0);
            chat.store(&mut connection).await?;

            let message = test_chat_message_at(chat.id(), [0; 16], at(1).into());
            message.store(&mut connection).await?;
            let mimi_id = *message.message().mimi_id().unwrap();

            let mut txn = connection.begin().await?;
            let changed_chats =
                StatusRecord::borrowed(&peer, read_report([mimi_id]), Utc::now().into())
                    .advance_read_markers(&mut txn, &own_user)
                    .await?;
            txn.commit().await?;

            assert!(changed_chats.is_empty());

            let n = Chat::unread_messages_count(&mut connection, chat.id(), &own_user).await?;
            assert_eq!(n, 1, "another member must not clear our unread messages");

            Ok(())
        }
    }
}
