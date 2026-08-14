// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    codec::BlobDecoded,
    identifiers::{Fqdn, MimiId, UserId},
    time::TimeStamp,
};
use chrono::{DateTime, Utc};
use sqlx::{query, query_as, query_scalar};
use uuid::Uuid;

use crate::{
    ChatId, ChatMessage, MessageId,
    chats::messages::persistence::{SqlChatMessage, VersionedMessage},
    db::access::{ReadConnection, WriteConnection},
};

use super::Reaction;

struct SqlReaction {
    reaction_mimi_id: MimiId,
    target_mimi_id: MimiId,
    chat_id: ChatId,
    sender_user_uuid: Uuid,
    sender_user_domain: Fqdn,
    emoji: String,
    created_at: aircommon::time::TimeStamp,
}

impl From<SqlReaction> for Reaction {
    fn from(
        SqlReaction {
            reaction_mimi_id,
            target_mimi_id,
            chat_id,
            sender_user_uuid,
            sender_user_domain,
            emoji,
            created_at,
        }: SqlReaction,
    ) -> Self {
        Reaction {
            reaction_mimi_id,
            target_mimi_id,
            chat_id,
            sender: UserId::new(sender_user_uuid, sender_user_domain),
            emoji,
            created_at,
        }
    }
}

/// A reaction row joined with its target message row.
struct SqlReactionWithTarget {
    reaction_mimi_id: MimiId,
    target_mimi_id: MimiId,
    chat_id: ChatId,
    sender_user_uuid: Uuid,
    sender_user_domain: Fqdn,
    emoji: String,
    created_at: TimeStamp,
    message_id: MessageId,
    message_mimi_id: Option<MimiId>,
    timestamp: TimeStamp,
    message_sender_user_uuid: Option<Uuid>,
    message_sender_user_domain: Option<Fqdn>,
    content: BlobDecoded<VersionedMessage>,
    sent: bool,
    status: i64,
    edited_at: Option<TimeStamp>,
    is_blocked: bool,
    in_reply_to_mimi_id: Option<MimiId>,
}

impl From<SqlReactionWithTarget> for (Reaction, ChatMessage) {
    fn from(row: SqlReactionWithTarget) -> Self {
        let reaction = Reaction {
            reaction_mimi_id: row.reaction_mimi_id,
            target_mimi_id: row.target_mimi_id,
            chat_id: row.chat_id,
            sender: UserId::new(row.sender_user_uuid, row.sender_user_domain),
            emoji: row.emoji,
            created_at: row.created_at,
        };
        let message = ChatMessage::from(SqlChatMessage {
            message_id: row.message_id,
            mimi_id: row.message_mimi_id,
            chat_id: row.chat_id,
            timestamp: row.timestamp,
            sender_user_uuid: row.message_sender_user_uuid,
            sender_user_domain: row.message_sender_user_domain,
            content: row.content,
            sent: row.sent,
            status: row.status,
            edited_at: row.edited_at,
            is_blocked: row.is_blocked,
            in_reply_to_mimi_id: row.in_reply_to_mimi_id,
        });
        (reaction, message)
    }
}

impl Reaction {
    /// Store the reaction.
    ///
    /// Reacting again with the same emoji on the same message is idempotent;
    /// returns `true` if a new reaction row was inserted, `false` if it already
    /// existed.
    pub(crate) async fn store(&self, mut connection: impl WriteConnection) -> sqlx::Result<bool> {
        let sender_uuid = self.sender.uuid();
        let sender_domain = self.sender.domain();
        let rows = query!(
            "INSERT INTO reaction (
                reaction_mimi_id,
                target_mimi_id,
                chat_id,
                sender_user_uuid,
                sender_user_domain,
                emoji,
                created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT DO NOTHING",
            &self.reaction_mimi_id,
            &self.target_mimi_id,
            self.chat_id,
            sender_uuid,
            sender_domain,
            self.emoji,
            self.created_at,
        )
        .execute(connection.as_mut())
        .await?
        .rows_affected();
        Ok(rows > 0)
    }

    /// Look up the MimiId of a reaction by its identifying tuple.
    pub(crate) async fn load_mimi_id(
        mut connection: impl ReadConnection,
        target_mimi_id: &MimiId,
        sender: &UserId,
        emoji: &str,
    ) -> sqlx::Result<Option<MimiId>> {
        let sender_uuid = sender.uuid();
        let sender_domain = sender.domain();
        let res = query!(
            r#"SELECT reaction_mimi_id AS "reaction_mimi_id: MimiId"
            FROM reaction
            WHERE target_mimi_id = ?
                AND sender_user_uuid = ?
                AND sender_user_domain = ?
                AND emoji = ?"#,
            target_mimi_id,
            sender_uuid,
            sender_domain,
            emoji,
        )
        .fetch_optional(connection.as_mut())
        .await?;
        Ok(res.map(|row| row.reaction_mimi_id))
    }

    /// Delete a reaction by its own MimiId, returning the `target_mimi_id` of
    /// the removed row (or `None` if no such reaction existed).
    pub(crate) async fn delete_by_mimi_id(
        mut connection: impl WriteConnection,
        reaction_mimi_id: &MimiId,
    ) -> sqlx::Result<Option<MimiId>> {
        let res = query!(
            r#"DELETE FROM reaction WHERE reaction_mimi_id = ?
            RETURNING target_mimi_id AS "target_mimi_id: MimiId""#,
            reaction_mimi_id,
        )
        .fetch_optional(connection.as_mut())
        .await?;
        Ok(res.map(|row| row.target_mimi_id))
    }

    /// Delete all reactions on a message, including those targeting versions
    /// superseded through its edit history. Must run while the edit history
    /// still exists.
    pub(crate) async fn delete_by_message_versions(
        mut connection: impl WriteConnection,
        message_id: MessageId,
        current_mimi_id: Option<&MimiId>,
    ) -> sqlx::Result<()> {
        query!(
            "DELETE FROM reaction WHERE target_mimi_id IN (
                SELECT mimi_id FROM message_edit WHERE message_id = ?
                UNION ALL
                SELECT ?
            )",
            message_id,
            current_mimi_id,
        )
        .execute(connection.as_mut())
        .await?;
        Ok(())
    }

    pub(crate) async fn exists_by_mimi_id(
        mut connection: impl ReadConnection,
        reaction_mimi_id: &MimiId,
    ) -> sqlx::Result<bool> {
        let exists = query_scalar!(
            "SELECT EXISTS(SELECT 1 FROM reaction WHERE reaction_mimi_id = ?)",
            reaction_mimi_id,
        )
        .fetch_one(connection.as_mut())
        .await?
            != 0;
        Ok(exists)
    }

    /// Load all newest reactions on the messages of the own user in a chat, together with their
    /// target message.
    ///
    /// Only reactions with a `created_at > since` are loaded (all of them if since is `None`),
    /// capped at `limit`, newest first, excluding own reactions.
    pub(crate) async fn load_own_message_reactions_since(
        mut connection: impl ReadConnection,
        chat_id: ChatId,
        own_user: &UserId,
        since: Option<DateTime<Utc>>,
        limit: u32,
    ) -> sqlx::Result<Vec<(Reaction, ChatMessage)>> {
        let own_uuid = own_user.uuid();
        let own_domain = own_user.domain();
        query_as!(
            SqlReactionWithTarget,
            r#"SELECT
                r.reaction_mimi_id AS "reaction_mimi_id: _",
                r.target_mimi_id AS "target_mimi_id: _",
                r.chat_id AS "chat_id: _",
                r.sender_user_uuid AS "sender_user_uuid: _",
                r.sender_user_domain AS "sender_user_domain: _",
                r.emoji,
                r.created_at AS "created_at: _",
                m.message_id AS "message_id: _",
                m.mimi_id AS "message_mimi_id: _",
                m.timestamp AS "timestamp: _",
                m.sender_user_uuid AS "message_sender_user_uuid: _",
                m.sender_user_domain AS "message_sender_user_domain: _",
                m.content AS "content: _",
                m.sent,
                m.status,
                m.edited_at AS "edited_at: _",
                b.user_uuid IS NOT NULL AS "is_blocked!: _",
                m.in_reply_to_mimi_id AS "in_reply_to_mimi_id: _"
            FROM reaction r
            INNER JOIN message m ON m.mimi_id = r.target_mimi_id
            LEFT JOIN blocked_contact b ON b.user_uuid = m.sender_user_uuid
                AND b.user_domain = m.sender_user_domain
            WHERE r.chat_id = ?1
                AND (?2 IS NULL OR r.created_at > ?2)
                AND m.sender_user_uuid = ?3
                AND m.sender_user_domain = ?4
                AND (r.sender_user_uuid != ?3 OR r.sender_user_domain != ?4)
            ORDER BY r.created_at DESC, r.reaction_mimi_id DESC
            LIMIT ?5"#,
            chat_id,
            since,
            own_uuid,
            own_domain,
            limit,
        )
        .fetch_all(connection.as_mut())
        .await
        .map(|rows| rows.into_iter().map(From::from).collect())
    }

    /// Load all reactions on a given message, oldest first.
    pub(crate) async fn load_by_target(
        mut connection: impl ReadConnection,
        target_mimi_id: &MimiId,
    ) -> sqlx::Result<Vec<Reaction>> {
        query_as!(
            SqlReaction,
            r#"SELECT
                reaction_mimi_id AS "reaction_mimi_id: _",
                target_mimi_id AS "target_mimi_id: _",
                chat_id AS "chat_id: _",
                sender_user_uuid AS "sender_user_uuid: _",
                sender_user_domain AS "sender_user_domain: _",
                emoji,
                created_at AS "created_at: _"
            FROM reaction
            WHERE target_mimi_id = ?
            ORDER BY created_at ASC, reaction_mimi_id ASC"#,
            target_mimi_id,
        )
        .fetch_all(connection.as_mut())
        .await
        .map(|rows| rows.into_iter().map(Reaction::from).collect())
    }

    /// Load the last reaction on a given message that `user_id` is party to:
    /// one on a message they sent, or one they made themselves.
    ///
    /// `target_is_own` says whether they sent the target message. The caller
    /// already holds it, so the target never has to be joined back in.
    pub(crate) async fn last_by_target_for_user(
        mut connection: impl ReadConnection,
        target_mimi_id: &MimiId,
        user_id: &UserId,
        target_is_own: bool,
    ) -> sqlx::Result<Option<Reaction>> {
        let user_uuid = user_id.uuid();
        let user_domain = user_id.domain();
        query_as!(
            SqlReaction,
            r#"SELECT
                reaction_mimi_id AS "reaction_mimi_id: _",
                target_mimi_id AS "target_mimi_id: _",
                chat_id AS "chat_id: _",
                sender_user_uuid AS "sender_user_uuid: _",
                sender_user_domain AS "sender_user_domain: _",
                emoji,
                created_at AS "created_at: _"
            FROM reaction
            WHERE target_mimi_id = ?1
                AND (?4 OR (sender_user_uuid = ?2 AND sender_user_domain = ?3))
            ORDER BY created_at DESC, reaction_mimi_id DESC
            LIMIT 1"#,
            target_mimi_id,
            user_uuid,
            user_domain,
            target_is_own,
        )
        .fetch_optional(connection.as_mut())
        .await
        .map(|row| row.map(Reaction::from))
    }
}

#[cfg(test)]
mod tests {
    use sqlx::SqlitePool;

    use crate::{
        chats::{
            messages::persistence::tests::test_chat_message_from, persistence::tests::test_chat,
        },
        db::access::DbAccess,
    };

    use super::*;

    fn user() -> UserId {
        UserId::random("localhost".parse().unwrap())
    }

    fn at(secs: i64) -> TimeStamp {
        TimeStamp::from(secs * 1_000_000_000)
    }

    #[sqlx::test]
    async fn last_reaction_covers_own_messages_and_own_reactions(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let mut connection = pool.write().await?;

        let own_user = user();
        let other = user();
        let third = user();

        let chat = test_chat();
        chat.store(&mut connection).await?;

        let own_message = test_chat_message_from(chat.id(), [1; 16], at(1), own_user.clone());
        let other_message = test_chat_message_from(chat.id(), [2; 16], at(2), other.clone());
        let third_message = test_chat_message_from(chat.id(), [3; 16], at(3), third.clone());
        for message in [&own_message, &other_message, &third_message] {
            message.store(&mut connection).await?;
        }
        let own_target = *own_message.message().mimi_id().unwrap();
        let other_target = *other_message.message().mimi_id().unwrap();
        let third_target = *third_message.message().mimi_id().unwrap();

        // Someone else on our message, ourselves on someone else's, and two
        // other people between themselves.
        for (id, target, reactor, secs) in [
            (1u8, own_target, &other, 10),
            (2, other_target, &own_user, 11),
            (3, third_target, &other, 12),
        ] {
            Reaction::new(
                MimiId::from_slice(&[id; 32]).unwrap(),
                target,
                chat.id(),
                reactor.clone(),
                format!("emoji-{id}"),
                at(secs),
            )
            .store(&mut connection)
            .await?;
        }

        let on_own =
            Reaction::last_by_target_for_user(&mut connection, &own_target, &own_user, true)
                .await?
                .unwrap();
        assert_eq!(on_own.sender, other);

        let by_own =
            Reaction::last_by_target_for_user(&mut connection, &other_target, &own_user, false)
                .await?
                .unwrap();
        assert_eq!(by_own.sender, own_user);

        let between_others =
            Reaction::last_by_target_for_user(&mut connection, &third_target, &own_user, false)
                .await?;
        assert!(between_others.is_none());

        Ok(())
    }

    #[sqlx::test]
    async fn last_reaction_is_the_newest_one(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let mut connection = pool.write().await?;

        let own_user = user();
        let other = user();

        let chat = test_chat();
        chat.store(&mut connection).await?;

        let message = test_chat_message_from(chat.id(), [1; 16], at(1), own_user.clone());
        message.store(&mut connection).await?;
        let target = *message.message().mimi_id().unwrap();

        for (id, secs) in [(1, 30), (2, 10), (3, 20)] {
            Reaction::new(
                MimiId::from_slice(&[id; 32]).unwrap(),
                target,
                chat.id(),
                other.clone(),
                format!("emoji-{id}"),
                at(secs),
            )
            .store(&mut connection)
            .await?;
        }

        let last = Reaction::last_by_target_for_user(&mut connection, &target, &own_user, true)
            .await?
            .unwrap();
        assert_eq!(last.emoji, "emoji-1");

        Ok(())
    }
}
