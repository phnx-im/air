// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Code migration storing the mentions of messages that clients received
//! before they tracked mentions.

use aircommon::{
    codec::PersistenceCodec,
    identifiers::{Fqdn, UserId},
};
use sqlx::query;
use tokio_stream::StreamExt;
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    MessageId, MimiContentExt,
    chats::messages::persistence::VersionedMessage,
    db::access::{ReadConnection, WriteConnection},
};

struct MentionBackfill {
    message_id: MessageId,
    mentions: Vec<UserId>,
    mentions_me: bool,
}

/// Stores all mentions sent in the past and whether they mention the local
/// user.
pub(crate) async fn backfill_mentions(mut connection: impl WriteConnection) -> anyhow::Result<()> {
    // A fresh database has no own client info yet, and no messages either.
    let own_user_id = query!(
        r#"SELECT
            user_uuid AS "user_uuid: Uuid",
            user_domain AS "user_domain: Fqdn"
        FROM own_client_info"#
    )
    .fetch_optional(connection.as_mut())
    .await?
    .map(|row| UserId::new(row.user_uuid, row.user_domain));

    let backfills = find_mentions(&mut connection, own_user_id.as_ref()).await?;
    info!(messages = backfills.len(), "Backfilling message mentions");
    for backfill in backfills {
        store_mentions(&mut connection, backfill).await?;
    }
    Ok(())
}

async fn find_mentions(
    mut connection: impl ReadConnection,
    own_user_id: Option<&UserId>,
) -> sqlx::Result<Vec<MentionBackfill>> {
    let mut rows = query!(
        r#"SELECT
            message_id AS "message_id: MessageId",
            sender_user_uuid AS "sender_user_uuid!: Uuid",
            sender_user_domain AS "sender_user_domain!: Fqdn",
            content
        FROM message
        WHERE sender_user_uuid IS NOT NULL"#
    )
    .fetch(connection.as_mut());

    let mut backfills = Vec::new();
    while let Some(row) = rows.next().await {
        let row = row?;
        // Decoding in the query would fail the whole migration on one bad row.
        let decoded = PersistenceCodec::from_slice::<VersionedMessage>(&row.content)
            .map_err(anyhow::Error::from)
            .and_then(|message| message.to_mimi_content());
        let content = match decoded {
            Ok(content) => content,
            Err(error) => {
                warn!(%error, message_id = %row.message_id, "Skipping an undecodable message");
                continue;
            }
        };
        let mut mentions = content.extract_mentioned_users();
        if mentions.is_empty() {
            continue;
        }
        mentions.sort_unstable();
        mentions.dedup();
        let sender = UserId::new(row.sender_user_uuid, row.sender_user_domain);
        let mentions_me = own_user_id.is_some_and(|own| sender != *own && mentions.contains(own));
        backfills.push(MentionBackfill {
            message_id: row.message_id,
            mentions,
            mentions_me,
        });
    }
    Ok(backfills)
}

async fn store_mentions(
    mut connection: impl WriteConnection,
    backfill: MentionBackfill,
) -> sqlx::Result<()> {
    let MentionBackfill {
        message_id,
        mentions,
        mentions_me,
    } = backfill;
    for (position, user) in mentions.into_iter().enumerate() {
        let position = position as i64;
        let (uuid, domain) = user.into_parts();
        query!(
            "INSERT INTO message_mention (
                message_id,
                position,
                user_uuid,
                user_domain
            )
            VALUES (?, ?, ?, ?)",
            message_id,
            position,
            uuid,
            domain,
        )
        .execute(connection.as_mut())
        .await?;
    }
    if mentions_me {
        query!(
            "UPDATE message SET mentions_me = TRUE WHERE message_id = ?",
            message_id,
        )
        .execute(connection.as_mut())
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use aircommon::{
        identifiers::{QsClientId, QsUserId},
        time::TimeStamp,
    };
    use mimi_content::MimiContent;
    use sqlx::SqlitePool;

    use crate::{
        ChatId, ChatMessage,
        chats::persistence::tests::test_chat,
        db::access::DbAccess,
        utils::persistence::{open_db_in_memory_before, run_client_migrations},
    };

    use super::*;

    const MENTIONS_MIGRATION: i64 = 20261123120000;

    fn user() -> UserId {
        UserId::random("localhost".parse().unwrap())
    }

    fn mention(user: &UserId) -> String {
        format!("[@name]({})", user.uri())
    }

    fn markdown(text: String) -> anyhow::Result<Vec<u8>> {
        let content = MimiContent::simple_markdown_message(text, [0; 16]);
        Ok(PersistenceCodec::to_vec(
            &VersionedMessage::from_mimi_content(&content)?,
        )?)
    }

    async fn open_db_with_chat() -> anyhow::Result<(SqlitePool, ChatId)> {
        let pool = open_db_in_memory_before(MENTIONS_MIGRATION).await?;
        let chat = test_chat();
        chat.store(DbAccess::for_tests(pool.clone()).write().await?)
            .await?;
        Ok((pool, chat.id()))
    }

    async fn store_own_user(pool: &SqlitePool, user_id: &UserId) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO own_client_info (qs_user_id, qs_client_id, user_uuid, user_domain)
            VALUES (?, ?, ?, ?)",
        )
        .bind(QsUserId::random())
        .bind(QsClientId::random(&mut rand::rng()))
        .bind(user_id.uuid())
        .bind(user_id.domain())
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Stores a message with the columns the schema has before
    /// `MENTIONS_MIGRATION`.
    async fn store_message(
        pool: &SqlitePool,
        chat_id: ChatId,
        sender: &UserId,
        content: &[u8],
    ) -> anyhow::Result<MessageId> {
        let message_id = MessageId::random();
        sqlx::query(
            "INSERT INTO message (
                message_id,
                chat_id,
                timestamp,
                sender_user_uuid,
                sender_user_domain,
                content,
                sent
            ) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(message_id)
        .bind(chat_id)
        .bind(TimeStamp::now())
        .bind(sender.uuid())
        .bind(sender.domain())
        .bind(content)
        .bind(true)
        .execute(pool)
        .await?;
        Ok(message_id)
    }

    fn sorted<const N: usize>(mut users: [UserId; N]) -> [UserId; N] {
        users.sort();
        users
    }

    async fn load(pool: &SqlitePool, message_id: MessageId) -> anyhow::Result<ChatMessage> {
        let db = DbAccess::for_tests(pool.clone());
        let message = ChatMessage::load(db.read().await?, message_id).await?;
        message.ok_or_else(|| anyhow::anyhow!("message {message_id} not found"))
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn backfills_mentions_of_stored_messages() -> anyhow::Result<()> {
        let (pool, chat_id) = open_db_with_chat().await?;
        let me = user();
        let alice = user();
        let bob = user();
        store_own_user(&pool, &me).await?;

        let text = format!("{} {} again {}", mention(&me), mention(&bob), mention(&me));
        let to_me = store_message(&pool, chat_id, &alice, &markdown(text)?).await?;
        let text = format!("{} {}", mention(&me), mention(&alice));
        let from_me = store_message(&pool, chat_id, &me, &markdown(text)?).await?;
        let text = format!("`{}` and [a link](https://air.ms)", mention(&me));
        let without = store_message(&pool, chat_id, &alice, &markdown(text)?).await?;
        store_message(&pool, chat_id, &alice, &[0xff]).await?;

        run_client_migrations(&DbAccess::for_tests(pool.clone())).await?;

        let message = load(&pool, to_me).await?;
        assert_eq!(message.mentions(), sorted([me.clone(), bob]));
        assert!(message.mentions_me());

        let message = load(&pool, from_me).await?;
        assert_eq!(message.mentions(), sorted([me, alice]));
        assert!(!message.mentions_me());

        let message = load(&pool, without).await?;
        assert_eq!(message.mentions(), []);
        assert!(!message.mentions_me());
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn backfills_mentions_without_own_user() -> anyhow::Result<()> {
        let (pool, chat_id) = open_db_with_chat().await?;
        let me = user();
        let alice = user();

        let message_id = store_message(&pool, chat_id, &alice, &markdown(mention(&me))?).await?;

        run_client_migrations(&DbAccess::for_tests(pool.clone())).await?;

        let message = load(&pool, message_id).await?;
        assert_eq!(message.mentions(), [me]);
        assert!(!message.mentions_me());
        Ok(())
    }
}
