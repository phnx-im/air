// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    identifiers::{Fqdn, MimiId, UserHandle, UserId},
    time::TimeStamp,
};
use chrono::{DateTime, Utc};
use mimi_content::MessageStatus;
use openmls::group::GroupId;
use sqlx::{
    Connection, SqliteConnection, SqliteExecutor, SqliteTransaction, query, query_as, query_scalar,
};
use tokio_stream::StreamExt;
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    Chat, ChatAttributes, ChatId, ChatStatus, ChatType, MessageId, store::StoreNotifier,
    utils::persistence::GroupIdWrapper,
};

use super::InactiveChat;

struct SqlChat {
    chat_id: ChatId,
    chat_title: String,
    chat_picture: Option<Vec<u8>>,
    group_id: GroupIdWrapper,
    last_read: DateTime<Utc>,
    connection_user_uuid: Option<Uuid>,
    connection_user_domain: Option<Fqdn>,
    connection_user_handle: Option<UserHandle>,
    is_confirmed_connection: bool,
    is_active: bool,
    is_blocked: bool,
}

impl SqlChat {
    fn convert(self, past_members: Vec<SqlPastMember>) -> Option<Chat> {
        let Self {
            chat_id,
            chat_title,
            chat_picture,
            group_id: GroupIdWrapper(group_id),
            last_read,
            connection_user_uuid,
            connection_user_domain,
            connection_user_handle,
            is_confirmed_connection,
            is_active,
            is_blocked,
        } = self;

        let chat_type = match (
            connection_user_uuid,
            connection_user_domain,
            connection_user_handle,
        ) {
            (Some(user_uuid), Some(domain), _) => {
                let connection_user_id = UserId::new(user_uuid, domain);
                if is_confirmed_connection {
                    ChatType::Connection(connection_user_id)
                } else {
                    warn!("Unconfirmed user connections are not supported anymore");
                    return None;
                }
            }

            (None, None, Some(handle)) => ChatType::HandleConnection(handle),
            _ => ChatType::Group,
        };

        let status = match (is_active, is_blocked) {
            (_, true) => ChatStatus::Blocked,
            (true, false) => ChatStatus::Active,
            (false, false) => ChatStatus::Inactive(InactiveChat::new(
                past_members.into_iter().map(From::from).collect(),
            )),
        };

        Some(Chat {
            id: chat_id,
            group_id,
            last_read,
            status,
            chat_type,
            attributes: ChatAttributes {
                title: chat_title,
                picture: chat_picture,
            },
        })
    }

    async fn load_past_members(
        &self,
        connection: &mut SqliteConnection,
    ) -> sqlx::Result<Vec<SqlPastMember>> {
        if self.is_active {
            return Ok(Vec::new());
        }
        Chat::load_past_members(connection, self.chat_id).await
    }
}

struct SqlPastMember {
    member_user_uuid: Uuid,
    member_user_domain: Fqdn,
}

impl From<SqlPastMember> for UserId {
    fn from(
        SqlPastMember {
            member_user_uuid,
            member_user_domain,
        }: SqlPastMember,
    ) -> Self {
        UserId::new(member_user_uuid, member_user_domain)
    }
}

impl Chat {
    pub(crate) async fn store(
        &self,
        connection: &mut SqliteConnection,
        notifier: &mut StoreNotifier,
    ) -> sqlx::Result<()> {
        info!(
            id =% self.id,
            title =% self.attributes().title(),
            "Storing chat"
        );
        let title = self.attributes().title();
        let picture = self.attributes().picture();
        let group_id = self.group_id.as_slice();
        let (is_active, past_members) = match self.status() {
            ChatStatus::Inactive(inactive_chat) => (false, inactive_chat.past_members().to_vec()),
            ChatStatus::Active => (true, Vec::new()),
            ChatStatus::Blocked => (false, Vec::new()),
        };
        let (
            is_confirmed_connection,
            connection_user_uuid,
            connection_user_domain,
            connection_user_handle,
        ) = match self.chat_type() {
            ChatType::HandleConnection(handle) => (false, None, None, Some(handle)),
            ChatType::Connection(user_id) => (
                true,
                Some(user_id.uuid()),
                Some(user_id.domain().clone()),
                None,
            ),
            ChatType::Group => (true, None, None, None),
        };
        query!(
            "INSERT INTO chat (
                chat_id,
                chat_title,
                chat_picture,
                group_id,
                last_read,
                connection_user_uuid,
                connection_user_domain,
                connection_user_handle,
                is_confirmed_connection,
                is_active
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            self.id,
            title,
            picture,
            group_id,
            self.last_read,
            connection_user_uuid,
            connection_user_domain,
            connection_user_handle,
            is_confirmed_connection,
            is_active,
        )
        .execute(&mut *connection)
        .await?;

        for member in past_members {
            let (uuid, domain) = member.into_parts();
            query!(
                "INSERT OR IGNORE INTO chat_past_member (
                    chat_id,
                    member_user_uuid,
                    member_user_domain
                )
                VALUES (?, ?, ?)",
                self.id,
                uuid,
                domain,
            )
            .execute(&mut *connection)
            .await?;
        }

        notifier.add(self.id);
        Ok(())
    }

    pub(crate) async fn load(
        connection: &mut SqliteConnection,
        chat_id: &ChatId,
    ) -> sqlx::Result<Option<Chat>> {
        let mut transaction = connection.begin().await?;
        let chat = query_as!(
            SqlChat,
            r#"SELECT
                chat_id AS "chat_id: _",
                chat_title,
                chat_picture,
                group_id AS "group_id: _",
                last_read AS "last_read: _",
                connection_user_uuid AS "connection_user_uuid: _",
                connection_user_domain AS "connection_user_domain: _",
                connection_user_handle AS "connection_user_handle: _",
                is_confirmed_connection,
                is_active,
                blocked_contact.user_uuid IS NOT NULL AS "is_blocked!: _"
            FROM chat
                LEFT JOIN blocked_contact
                ON blocked_contact.user_uuid = chat.connection_user_uuid
                AND blocked_contact.user_domain = chat.connection_user_domain
            WHERE chat_id = ?"#,
            chat_id
        )
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(chat) = chat else {
            return Ok(None);
        };
        let members = chat.load_past_members(&mut transaction).await?;
        transaction.commit().await?;
        Ok(chat.convert(members))
    }

    pub(crate) async fn load_ordered_ids(
        executor: impl SqliteExecutor<'_>,
    ) -> sqlx::Result<Vec<ChatId>> {
        // Note: Sqlite considers NULL values as the smallest value.
        // Note: A draft is empty <=> trimmed text is empty AND editing_id is null.
        query_scalar!(
            r#"SELECT
                c.chat_id AS "chat_id: _"
            FROM chat c
            LEFT OUTER JOIN message_draft d ON
                d.chat_id = c.chat_id AND
                d.is_committed = TRUE AND
                NOT (TRIM(d.message) = '' AND d.editing_id IS NULL)
            ORDER BY
                d.updated_at DESC,
                (SELECT timestamp
                    FROM message
                    WHERE chat_id = c.chat_id
                    ORDER BY timestamp DESC
                    LIMIT 1
                ) DESC,
                c.chat_id
            "#,
        )
        .fetch_all(executor)
        .await
    }

    pub(crate) async fn load_by_group_id(
        connection: &mut SqliteConnection,
        group_id: &GroupId,
    ) -> sqlx::Result<Option<Chat>> {
        let group_id = group_id.as_slice();
        let mut transaction = connection.begin().await?;
        let chat = query_as!(
            SqlChat,
            r#"SELECT
                chat_id AS "chat_id: _",
                chat_title,
                chat_picture,
                group_id AS "group_id: _",
                last_read AS "last_read: _",
                connection_user_uuid AS "connection_user_uuid: _",
                connection_user_domain AS "connection_user_domain: _",
                connection_user_handle AS "connection_user_handle: _",
                is_confirmed_connection,
                is_active,
                blocked_contact.user_uuid IS NOT NULL AS "is_blocked!: _"
            FROM chat
                LEFT JOIN blocked_contact
                ON blocked_contact.user_uuid = chat.connection_user_uuid
                AND blocked_contact.user_domain = chat.connection_user_domain
            WHERE group_id = ?"#,
            group_id
        )
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(chat) = chat else {
            return Ok(None);
        };
        let members = chat.load_past_members(&mut transaction).await?;
        transaction.commit().await?;
        Ok(chat.convert(members))
    }

    pub(crate) async fn load_all(connection: &mut SqliteConnection) -> sqlx::Result<Vec<Chat>> {
        let mut transaction = connection.begin().await?;
        let mut chats = query_as!(
            SqlChat,
            r#"SELECT
                chat_id AS "chat_id: _",
                chat_title,
                chat_picture,
                group_id AS "group_id: _",
                last_read AS "last_read: _",
                connection_user_uuid AS "connection_user_uuid: _",
                connection_user_domain AS "connection_user_domain: _",
                connection_user_handle AS "connection_user_handle: _",
                is_confirmed_connection,
                is_active,
                blocked_contact.user_uuid IS NOT NULL AS "is_blocked!: _"
            FROM chat
                LEFT JOIN blocked_contact
                ON blocked_contact.user_uuid = chat.connection_user_uuid
                AND blocked_contact.user_domain = chat.connection_user_domain
            "#,
        )
        .fetch(&mut *transaction)
        .filter_map(|res| res.map(|chat| chat.convert(Vec::new())).transpose())
        .collect::<sqlx::Result<Vec<Chat>>>()
        .await?;
        for chat in &mut chats {
            let id = chat.id();
            if let ChatStatus::Inactive(inactive) = chat.status_mut() {
                let members = Chat::load_past_members(&mut *transaction, id).await?;
                inactive
                    .past_members_mut()
                    .extend(members.into_iter().map(From::from));
            }
        }
        transaction.commit().await?;
        Ok(chats)
    }

    pub(super) async fn update_picture(
        executor: impl SqliteExecutor<'_>,
        notifier: &mut StoreNotifier,
        chat_id: ChatId,
        chat_picture: Option<&[u8]>,
    ) -> sqlx::Result<()> {
        query!(
            "UPDATE chat SET chat_picture = ? WHERE chat_id = ?",
            chat_picture,
            chat_id,
        )
        .execute(executor)
        .await?;
        notifier.update(chat_id);
        Ok(())
    }

    pub(super) async fn update_status(
        connection: &mut SqliteConnection,
        notifier: &mut StoreNotifier,
        chat_id: ChatId,
        status: &ChatStatus,
    ) -> sqlx::Result<()> {
        let mut transaction = connection.begin().await?;
        match status {
            ChatStatus::Inactive(inactive) => {
                query!(
                    "UPDATE chat SET is_active = false WHERE chat_id = ?",
                    chat_id,
                )
                .execute(&mut *transaction)
                .await?;
                query!("DELETE FROM chat_past_member WHERE chat_id = ?", chat_id,)
                    .execute(&mut *transaction)
                    .await?;
                for member in inactive.past_members() {
                    let uuid = member.uuid();
                    let domain = member.domain();
                    query!(
                        "INSERT OR IGNORE INTO chat_past_member (
                            chat_id,
                            member_user_uuid,
                            member_user_domain
                        )
                        VALUES (?, ?, ?)",
                        chat_id,
                        uuid,
                        domain,
                    )
                    .execute(&mut *transaction)
                    .await?;
                }
            }
            ChatStatus::Active => {
                query!(
                    "UPDATE chat SET is_active = true WHERE chat_id = ?",
                    chat_id,
                )
                .execute(&mut *transaction)
                .await?;
            }
            ChatStatus::Blocked => {
                // This status is a no-op
            }
        }
        transaction.commit().await?;
        notifier.update(chat_id);
        Ok(())
    }

    pub(crate) async fn delete(
        executor: impl SqliteExecutor<'_>,
        notifier: &mut StoreNotifier,
        chat_id: ChatId,
    ) -> sqlx::Result<()> {
        query!("DELETE FROM chat WHERE chat_id = ?", chat_id)
            .execute(executor)
            .await?;
        notifier.remove(chat_id);
        Ok(())
    }

    /// Set the `last_read` marker of all chats with the given
    /// [`chatId`]s to the given timestamps. This is used to mark all
    /// messages up to this timestamp as read.
    pub(crate) async fn mark_as_read(
        connection: &mut sqlx::SqliteConnection,
        notifier: &mut StoreNotifier,
        mark_as_read_data: impl IntoIterator<Item = (ChatId, DateTime<Utc>)>,
    ) -> sqlx::Result<()> {
        let mut transaction = connection.begin().await?;

        for (chat_id, timestamp) in mark_as_read_data {
            let unread_messages: Vec<MessageId> = query_scalar!(
                r#"SELECT
                    message_id AS "message_id: _"
                FROM message
                INNER JOIN chat c ON c.chat_id = ?1
                WHERE c.chat_id = ?1 AND timestamp > c.last_read"#,
                chat_id,
            )
            .fetch_all(&mut *transaction)
            .await?;

            for message_id in unread_messages {
                notifier.update(message_id);
            }

            let updated = query!(
                "UPDATE chat
                SET last_read = ?1
                WHERE chat_id = ?2 AND last_read < ?1",
                timestamp,
                chat_id,
            )
            .execute(&mut *transaction)
            .await?;
            if updated.rows_affected() == 1 {
                notifier.update(chat_id);
            }
        }

        transaction.commit().await?;
        Ok(())
    }

    /// Mark all messages in the chat as read until including the given message id.
    ///
    /// Returns whether the chat was marked as read and the mimi ids of the messages that
    /// were marked as read.
    pub(crate) async fn mark_as_read_until_message_id(
        txn: &mut SqliteTransaction<'_>,
        notifier: &mut StoreNotifier,
        chat_id: ChatId,
        until_message_id: MessageId,
        own_user: &UserId,
    ) -> sqlx::Result<(bool, Vec<(MessageId, MimiId)>)> {
        let (our_user_uuid, our_user_domain) = own_user.clone().into_parts();

        let timestamp: Option<DateTime<Utc>> = query_scalar!(
            r#"SELECT
                timestamp AS "timestamp: _"
            FROM message WHERE message_id = ?"#,
            until_message_id
        )
        .fetch_optional(txn.as_mut())
        .await?;

        let Some(timestamp) = timestamp else {
            return Ok((false, Vec::new()));
        };

        let old_timestamp = query!(
            "SELECT last_read FROM chat
            WHERE chat_id = ?",
            chat_id,
        )
        .fetch_one(txn.as_mut())
        .await?
        .last_read;

        struct Record {
            message_id: MessageId,
            mimi_id: MimiId,
        }

        let unread_status = MessageStatus::Unread.repr();
        let delivered_status = MessageStatus::Delivered.repr();
        let new_marked_as_read: Vec<(MessageId, MimiId)> = query_as!(
            Record,
            r#"SELECT
                m.message_id AS "message_id: _",
                m.mimi_id AS "mimi_id!: _"
            FROM message m
            LEFT JOIN message_status s
                ON s.message_id = m.message_id
                AND s.sender_user_uuid = ?2
                AND s.sender_user_domain = ?3
            WHERE chat_id = ?1
                AND m.timestamp > ?2
                AND (m.sender_user_uuid != ?3 OR m.sender_user_domain != ?4)
                AND mimi_id IS NOT NULL
                AND (s.status IS NULL OR s.status = ?5 OR s.status = ?6)"#,
            chat_id,
            old_timestamp,
            our_user_uuid,
            our_user_domain,
            unread_status,
            delivered_status,
        )
        .fetch(txn.as_mut())
        .map(|record| record.map(|record| (record.message_id, record.mimi_id)))
        .collect::<sqlx::Result<Vec<_>>>()
        .await?;

        let updated = query!(
            "UPDATE chat SET last_read = ?1
            WHERE chat_id = ?2 AND last_read != ?1",
            timestamp,
            chat_id,
        )
        .execute(txn.as_mut())
        .await?;

        let marked_as_read = updated.rows_affected() == 1;
        if marked_as_read {
            notifier.update(chat_id);
        }
        Ok((marked_as_read, new_marked_as_read))
    }

    pub(crate) async fn mark_as_unread(
        txn: &mut SqliteTransaction<'_>,
        notifier: &mut StoreNotifier,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> sqlx::Result<()> {
        let timestamp: Option<TimeStamp> = query_scalar!(
            r#"SELECT
                timestamp AS "timestamp: _"
            FROM message
            WHERE timestamp < (
                SELECT timestamp
                FROM message
                WHERE message_id = ?
            )
            ORDER BY timestamp DESC
            LIMIT 1"#,
            message_id
        )
        .fetch_optional(txn.as_mut())
        .await?;

        query!(
            "UPDATE chat SET last_read = ?1
            WHERE chat_id = ?2",
            timestamp,
            chat_id,
        )
        .execute(txn.as_mut())
        .await?;

        notifier.update(message_id);

        Ok(())
    }

    pub(crate) async fn global_unread_message_count(
        executor: impl SqliteExecutor<'_>,
    ) -> sqlx::Result<usize> {
        query_scalar!(
            r#"SELECT
                COUNT(m.chat_id) AS "count: _"
            FROM
                chat c
            LEFT JOIN
                message m
            ON
                c.chat_id = m.chat_id
                AND m.sender_user_uuid IS NOT NULL
                AND m.sender_user_domain IS NOT NULL
                AND m.timestamp > c.last_read"#
        )
        .fetch_one(executor)
        .await
        .map(|n: u32| n.try_into().expect("usize overflow"))
    }

    pub(crate) async fn messages_count(
        executor: impl SqliteExecutor<'_>,
        chat_id: ChatId,
    ) -> sqlx::Result<usize> {
        query_scalar!(
            r#"SELECT
                COUNT(*) AS "count: _"
            FROM
                message m
            WHERE
                m.chat_id = ?
                AND m.sender_user_uuid IS NOT NULL
                AND m.sender_user_domain IS NOT NULL"#,
            chat_id
        )
        .fetch_one(executor)
        .await
        .map(|n: u32| n.try_into().expect("usize overflow"))
    }

    pub(crate) async fn unread_messages_count(
        executor: impl SqliteExecutor<'_>,
        chat_id: ChatId,
    ) -> sqlx::Result<usize> {
        query_scalar!(
            r#"SELECT
                COUNT(*) AS "count: _"
            FROM
                message
            WHERE
                chat_id = ?1
                AND sender_user_uuid IS NOT NULL
                AND sender_user_domain IS NOT NULL
                AND timestamp >
                (
                    SELECT
                        last_read
                    FROM
                        chat
                    WHERE
                        chat_id = ?1
                )"#,
            chat_id
        )
        .fetch_one(executor)
        .await
        .map(|n: u32| n.try_into().expect("usize overflow"))
    }

    pub(super) async fn set_chat_type(
        &self,
        executor: impl SqliteExecutor<'_>,
        notifier: &mut StoreNotifier,
        chat_type: &ChatType,
    ) -> sqlx::Result<()> {
        match chat_type {
            ChatType::HandleConnection(handle) => {
                query!(
                    "UPDATE chat SET
                        connection_user_uuid = NULL,
                        connection_user_domain = NULL,
                        connection_user_handle = ?,
                        is_confirmed_connection = false
                    WHERE chat_id = ?",
                    handle,
                    self.id,
                )
                .execute(executor)
                .await?;
            }
            ChatType::Connection(user_id) => {
                let uuid = user_id.uuid();
                let domain = user_id.domain();
                query!(
                    "UPDATE chat SET
                        connection_user_uuid = ?,
                        connection_user_domain = ?,
                        is_confirmed_connection = true
                    WHERE chat_id = ?",
                    uuid,
                    domain,
                    self.id,
                )
                .execute(executor)
                .await?;
            }
            ChatType::Group => {
                query!(
                    "UPDATE chat SET
                        connection_user_uuid = NULL,
                        connection_user_domain = NULL
                    WHERE chat_id = ?",
                    self.id,
                )
                .execute(executor)
                .await?;
            }
        }
        notifier.update(self.id);
        Ok(())
    }

    async fn load_past_members(
        executor: impl SqliteExecutor<'_>,
        chat_id: ChatId,
    ) -> sqlx::Result<Vec<SqlPastMember>> {
        let mut members = query_as!(
            SqlPastMember,
            r#"SELECT
                member_user_uuid AS "member_user_uuid: _",
                member_user_domain AS "member_user_domain: _"
            FROM chat_past_member
            WHERE chat_id = ?"#,
            chat_id
        )
        .fetch_all(executor)
        .await?;
        // make the order deterministic
        members.sort_unstable_by(|a, b| {
            a.member_user_uuid
                .cmp(&b.member_user_uuid)
                .then(a.member_user_domain.cmp(&b.member_user_domain))
        });
        Ok(members)
    }
}

#[cfg(test)]
pub mod tests {
    use chrono::{Days, Duration};
    use sqlx::{Sqlite, pool::PoolConnection};
    use uuid::Uuid;

    use crate::{
        InactiveChat, MessageDraft, chats::messages::persistence::tests::test_chat_message,
    };

    use super::*;

    pub(crate) fn test_chat() -> Chat {
        let id = ChatId {
            uuid: Uuid::new_v4(),
        };
        Chat {
            id,
            group_id: GroupId::from_slice(&[0; 32]),
            last_read: Utc::now(),
            status: ChatStatus::Active,
            chat_type: ChatType::Group,
            attributes: ChatAttributes {
                title: "Test chat".to_string(),
                picture: None,
            },
        }
    }

    #[sqlx::test]
    async fn store_load(mut connection: PoolConnection<Sqlite>) -> anyhow::Result<()> {
        let mut store_notifier = StoreNotifier::noop();

        let chat = test_chat();
        chat.store(&mut connection, &mut store_notifier).await?;
        let loaded = Chat::load(&mut connection, &chat.id)
            .await?
            .expect("missing chat");
        assert_eq!(loaded, chat);

        Ok(())
    }

    #[sqlx::test]
    async fn store_load_by_group_id(mut connection: PoolConnection<Sqlite>) -> anyhow::Result<()> {
        let mut store_notifier = StoreNotifier::noop();

        let chat = test_chat();
        chat.store(&mut connection, &mut store_notifier).await?;
        let loaded = Chat::load_by_group_id(&mut connection, &chat.group_id)
            .await?
            .expect("missing chat");
        assert_eq!(loaded, chat);

        Ok(())
    }

    #[sqlx::test]
    async fn store_load_all(mut connection: PoolConnection<Sqlite>) -> anyhow::Result<()> {
        let mut store_notifier = StoreNotifier::noop();

        let chat_a = test_chat();
        chat_a.store(&mut connection, &mut store_notifier).await?;

        let chat_b = test_chat();
        chat_b.store(&mut connection, &mut store_notifier).await?;

        let loaded = Chat::load_all(&mut connection).await?;
        assert_eq!(loaded, [chat_a, chat_b]);

        Ok(())
    }

    #[sqlx::test]
    async fn load_ordered_ids(mut connection: PoolConnection<Sqlite>) -> anyhow::Result<()> {
        let mut store_notifier = StoreNotifier::noop();

        // Chat without a message
        let chat_1 = test_chat();
        chat_1.store(&mut connection, &mut store_notifier).await?;

        // Chat with a message
        let chat_2 = test_chat();
        chat_2.store(&mut connection, &mut store_notifier).await?;
        let message = test_chat_message(chat_2.id());
        message.store(&mut *connection, &mut store_notifier).await?;

        // Chat with another more recent message
        let chat_3 = test_chat();
        chat_3.store(&mut connection, &mut store_notifier).await?;
        let mut message = test_chat_message(chat_3.id());
        message.set_timestamp(Utc::now().checked_add_days(Days::new(1)).unwrap().into());
        message.store(&mut *connection, &mut store_notifier).await?;

        // Chat with an empty draft message
        let chat_4 = test_chat();
        chat_4.store(&mut connection, &mut store_notifier).await?;
        let mut message = test_chat_message(chat_4.id());
        message.set_timestamp(Utc::now().checked_add_days(Days::new(2)).unwrap().into());
        message.store(&mut *connection, &mut store_notifier).await?;
        MessageDraft {
            message: "    ".into(), // Whitespace only
            editing_id: None,
            updated_at: TimeStamp::now().into(),
            is_committed: false,
        }
        .store(&mut *connection, &mut store_notifier, chat_4.id())
        .await?;

        // Chat with a draft message
        let chat_5 = test_chat();
        chat_5.store(&mut connection, &mut store_notifier).await?;
        let message = test_chat_message(chat_5.id());
        message.store(&mut *connection, &mut store_notifier).await?;
        MessageDraft {
            message: "Hello, world!".to_string(),
            editing_id: Some(message.id()),
            updated_at: Utc::now(),
            is_committed: true,
        }
        .store(&mut *connection, &mut store_notifier, chat_5.id())
        .await?;

        // Chat with a more recent draft message
        let chat_6 = test_chat();
        chat_6.store(&mut connection, &mut store_notifier).await?;
        let message = test_chat_message(chat_6.id());
        message.store(&mut *connection, &mut store_notifier).await?;
        MessageDraft {
            message: "Hello, world!".to_string(),
            editing_id: Some(message.id()),
            updated_at: Utc::now().checked_add_days(Days::new(1)).unwrap(),
            is_committed: true,
        }
        .store(&mut *connection, &mut store_notifier, chat_6.id())
        .await?;

        let loaded = Chat::load_ordered_ids(&mut *connection).await?;
        assert_eq!(
            loaded,
            [
                chat_6.id(), // Has the most recent draft message
                chat_5.id(), // Has a draft message
                chat_4.id(), // Empty draft message, but most recent message
                chat_3.id(), // Second most recent message
                chat_2.id(), // Has a message
                chat_1.id()  // No message
            ]
        );

        Ok(())
    }

    #[sqlx::test]
    async fn update_chat_picture(mut connection: PoolConnection<Sqlite>) -> anyhow::Result<()> {
        let mut store_notifier = StoreNotifier::noop();

        let mut chat = test_chat();
        chat.store(&mut connection, &mut store_notifier).await?;

        let new_picture = [1, 2, 3];
        Chat::update_picture(
            &mut *connection,
            &mut store_notifier,
            chat.id,
            Some(&new_picture),
        )
        .await?;

        chat.attributes.picture = Some(new_picture.to_vec());

        let loaded = Chat::load(&mut connection, &chat.id).await?.unwrap();
        assert_eq!(loaded, chat);

        Ok(())
    }

    #[sqlx::test]
    async fn update_chat_status(mut connection: PoolConnection<Sqlite>) -> anyhow::Result<()> {
        let mut store_notifier = StoreNotifier::noop();

        let mut chat = test_chat();
        chat.store(&mut connection, &mut store_notifier).await?;

        let mut past_members = vec![
            UserId::random("localhost".parse().unwrap()),
            UserId::random("localhost".parse().unwrap()),
        ];
        // implicit assumption: past members are sorted
        past_members.sort_unstable();

        let status = ChatStatus::Inactive(InactiveChat::new(past_members));
        Chat::update_status(&mut connection, &mut store_notifier, chat.id, &status).await?;

        chat.status = status;
        let loaded = Chat::load(&mut connection, &chat.id).await?.unwrap();
        assert_eq!(loaded, chat);

        Ok(())
    }

    #[sqlx::test]
    async fn delete(mut connection: PoolConnection<Sqlite>) -> anyhow::Result<()> {
        let mut store_notifier = StoreNotifier::noop();

        let chat = test_chat();
        chat.store(&mut connection, &mut store_notifier).await?;
        let loaded = Chat::load(&mut connection, &chat.id).await?.unwrap();
        assert_eq!(loaded, chat);

        Chat::delete(&mut *connection, &mut store_notifier, chat.id).await?;
        let loaded = Chat::load(&mut connection, &chat.id).await?;
        assert!(loaded.is_none());

        Ok(())
    }

    #[sqlx::test]
    async fn counters(mut connection: PoolConnection<Sqlite>) -> anyhow::Result<()> {
        let mut store_notifier = StoreNotifier::noop();

        let chat_a = test_chat();
        chat_a.store(&mut connection, &mut store_notifier).await?;

        let chat_b = test_chat();
        chat_b.store(&mut connection, &mut store_notifier).await?;

        let message_a = test_chat_message(chat_a.id());
        let message_b = test_chat_message(chat_b.id());

        message_a
            .store(&mut *connection, &mut store_notifier)
            .await?;
        message_b
            .store(&mut *connection, &mut store_notifier)
            .await?;

        let n = Chat::messages_count(&mut *connection, chat_a.id()).await?;
        assert_eq!(n, 1);

        let n = Chat::messages_count(&mut *connection, chat_b.id()).await?;
        assert_eq!(n, 1);

        let n = Chat::global_unread_message_count(&mut *connection).await?;
        assert_eq!(n, 2);

        let mut txn = connection.begin().await?;
        Chat::mark_as_read(
            &mut txn,
            &mut store_notifier,
            [(chat_a.id(), message_a.timestamp() - Duration::seconds(1))],
        )
        .await?;
        txn.commit().await?;
        let n = Chat::unread_messages_count(&mut *connection, chat_a.id()).await?;
        assert_eq!(n, 1);

        let mut txn = connection.begin().await?;
        Chat::mark_as_read(&mut txn, &mut store_notifier, [(chat_a.id(), Utc::now())]).await?;
        txn.commit().await?;
        let n = Chat::unread_messages_count(&mut *connection, chat_a.id()).await?;
        assert_eq!(n, 0);

        let mut txn = connection.begin().await?;
        Chat::mark_as_read_until_message_id(
            &mut txn,
            &mut store_notifier,
            chat_b.id(),
            MessageId::random(),
            &UserId::random("localhost".parse().unwrap()),
        )
        .await?;
        txn.commit().await?;
        let n = Chat::unread_messages_count(&mut *connection, chat_b.id()).await?;
        assert_eq!(n, 1);

        let mut txn = connection.begin().await?;
        Chat::mark_as_read_until_message_id(
            &mut txn,
            &mut store_notifier,
            chat_b.id(),
            message_b.id(),
            &UserId::random("localhost".parse().unwrap()),
        )
        .await?;
        txn.commit().await?;
        let n = Chat::unread_messages_count(&mut *connection, chat_b.id()).await?;
        assert_eq!(n, 0);

        let n = Chat::global_unread_message_count(&mut *connection).await?;
        assert_eq!(n, 0);

        Ok(())
    }
}
