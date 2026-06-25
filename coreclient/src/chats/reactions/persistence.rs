// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::{Fqdn, MimiId, UserId};
use sqlx::{query, query_as};
use uuid::Uuid;

use crate::{
    ChatId,
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
}
