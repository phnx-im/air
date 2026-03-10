// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;

use aircommon::{
    codec::{self, BlobDecoded, BlobEncoded, PersistenceCodec},
    identifiers::{Fqdn, MimiId, UserId},
    time::TimeStamp,
};
use anyhow::bail;
use mimi_content::{MessageStatus, MimiContent};
use serde::{Deserialize, Serialize};
use sqlx::{SqliteExecutor, SqliteTransaction, query, query_as, query_scalar};
use tokio_stream::StreamExt;
use tracing::{error, warn};
use uuid::Uuid;

use crate::{
    ChatId, ChatMessage, ContentMessage, Message, chats::messages::InReplyToMessage,
    store::StoreNotifier,
};

use super::{ErrorMessage, EventMessage};

const UNKNOWN_MESSAGE_VERSION: u16 = 0;
const CURRENT_MESSAGE_VERSION: u16 = 1;

#[derive(Serialize, Deserialize)]
pub struct VersionedMessage {
    #[serde(default = "VersionedMessage::unknown_message_version")]
    pub(crate) version: u16,
    // We store the message as bytes, because deserialization depends on
    // other parameters.
    // TODO: Do not use cbor unsigned int array here
    #[serde(default)]
    pub(crate) content: Vec<u8>,
}

impl fmt::Debug for VersionedMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VersionedMessage")
            .field("version", &self.version)
            .field("content_len", &self.content.len())
            .finish_non_exhaustive()
    }
}

impl VersionedMessage {
    const fn unknown_message_version() -> u16 {
        UNKNOWN_MESSAGE_VERSION
    }
}

impl VersionedMessage {
    fn to_event_message(&self) -> anyhow::Result<EventMessage> {
        match self.version {
            CURRENT_MESSAGE_VERSION => {
                Ok(PersistenceCodec::from_slice::<EventMessage>(&self.content)?)
            }
            other => bail!("unknown event message version: {other}"),
        }
    }

    pub(crate) fn to_mimi_content(&self) -> anyhow::Result<MimiContent> {
        match self.version {
            CURRENT_MESSAGE_VERSION => {
                Ok(PersistenceCodec::from_slice::<MimiContent>(&self.content)?)
            }
            other => bail!("unknown mimi content message version: {other}"),
        }
    }

    fn from_event_message(
        event: &EventMessage,
    ) -> Result<VersionedMessage, aircommon::codec::Error> {
        Ok(VersionedMessage {
            version: CURRENT_MESSAGE_VERSION,
            content: PersistenceCodec::to_vec(&event)?,
        })
    }

    pub(crate) fn from_mimi_content(
        content: &MimiContent,
    ) -> Result<VersionedMessage, aircommon::codec::Error> {
        Ok(VersionedMessage {
            version: CURRENT_MESSAGE_VERSION,
            content: PersistenceCodec::to_vec(&content)?,
        })
    }
}

use super::{MessageId, TimestampedMessage};

struct SqlChatMessage {
    message_id: MessageId,
    mimi_id: Option<MimiId>,
    chat_id: ChatId,
    timestamp: TimeStamp,
    sender_user_uuid: Option<Uuid>,
    sender_user_domain: Option<Fqdn>,
    content: BlobDecoded<VersionedMessage>,
    sent: bool,
    status: i64,
    edited_at: Option<TimeStamp>,
    is_blocked: bool,
    in_reply_to_mimi_id: Option<MimiId>,
}

#[derive(thiserror::Error, Debug)]
enum VersionedMessageError {
    #[error(transparent)]
    Codec(#[from] codec::Error),
}

impl From<VersionedMessageError> for sqlx::Error {
    fn from(value: VersionedMessageError) -> Self {
        sqlx::Error::Decode(Box::new(value))
    }
}

impl TryFrom<SqlChatMessage> for ChatMessage {
    type Error = VersionedMessageError;

    fn try_from(
        SqlChatMessage {
            message_id,
            mimi_id,
            chat_id,
            timestamp,
            sender_user_uuid,
            sender_user_domain,
            content,
            sent,
            status,
            edited_at,
            is_blocked,
            in_reply_to_mimi_id,
        }: SqlChatMessage,
    ) -> Result<Self, Self::Error> {
        let message = match (sender_user_uuid, sender_user_domain) {
            // user message
            (Some(sender_user_uuid), Some(sender_user_domain)) => {
                let sender = UserId::new(sender_user_uuid, sender_user_domain);
                content
                    .into_inner()
                    .to_mimi_content()
                    .map(|content| {
                        Message::Content(Box::new(ContentMessage {
                            sender,
                            sent,
                            content,
                            mimi_id,
                            edited_at,
                        }))
                    })
                    .unwrap_or_else(|e| {
                        warn!("Message parsing failed: {e}");
                        Message::Event(EventMessage::Error(ErrorMessage::new(
                            "Message parsing failed".to_owned(),
                        )))
                    })
            }
            // system message
            _ => Message::Event(content.into_inner().to_event_message().unwrap_or_else(|e| {
                warn!("Event parsing failed: {e}");
                EventMessage::Error(ErrorMessage::new("Event parsing failed".to_owned()))
            })),
        };

        let timestamped_message = TimestampedMessage { timestamp, message };
        let status = if is_blocked {
            MessageStatus::Hidden
        } else {
            u8::try_from(status)
                .map(MessageStatus::from_repr)
                .unwrap_or(MessageStatus::Unread)
        };

        Ok(ChatMessage {
            message_id,
            chat_id,
            in_reply_to: in_reply_to_mimi_id.map(|id| (id, None)),
            timestamped_message,
            status,
        })
    }
}

impl ChatMessage {
    pub async fn load(
        txn: &mut SqliteTransaction<'_>,
        message_id: MessageId,
    ) -> sqlx::Result<Option<Self>> {
        query_as!(
            SqlChatMessage,
            r#"SELECT
                message_id AS "message_id: _",
                mimi_id AS "mimi_id: _",
                chat_id AS "chat_id: _",
                timestamp AS "timestamp: _",
                sender_user_uuid AS "sender_user_uuid: _",
                sender_user_domain AS "sender_user_domain: _",
                content AS "content: _",
                sent,
                status,
                edited_at AS "edited_at: _",
                b.user_uuid IS NOT NULL AS "is_blocked!: _",
                in_reply_to_mimi_id AS "in_reply_to_mimi_id: _"
            FROM message
            LEFT JOIN blocked_contact b ON b.user_uuid = sender_user_uuid
                AND b.user_domain = sender_user_domain
            WHERE message_id = ?
            "#,
            message_id,
        )
        .fetch_optional(txn.as_mut())
        .await?
        .map(TryFrom::try_from)
        .transpose()?
        .try_load_replied_message(txn)
        .await
    }

    pub(crate) async fn load_by_mimi_id(
        txn: &mut SqliteTransaction<'_>,
        mimi_id: &MimiId,
    ) -> sqlx::Result<Option<Self>> {
        query_as!(
            SqlChatMessage,
            r#"SELECT
                message_id AS "message_id: _",
                mimi_id AS "mimi_id: _",
                chat_id AS "chat_id: _",
                timestamp AS "timestamp: _",
                sender_user_uuid AS "sender_user_uuid: _",
                sender_user_domain AS "sender_user_domain: _",
                content AS "content: _",
                sent,
                status,
                edited_at AS "edited_at: _",
                b.user_uuid IS NOT NULL AS "is_blocked!: _",
                in_reply_to_mimi_id AS "in_reply_to_mimi_id: _"
            FROM message
            LEFT JOIN blocked_contact b ON b.user_uuid = sender_user_uuid
                AND b.user_domain = sender_user_domain
            WHERE mimi_id = ?
            "#,
            mimi_id,
        )
        .fetch_optional(txn.as_mut())
        .await?
        .map(TryFrom::try_from)
        .transpose()?
        .try_load_replied_message(txn)
        .await
    }

    pub(crate) async fn load_multiple(
        txn: &mut SqliteTransaction<'_>,
        chat_id: ChatId,
        number_of_messages: u32,
    ) -> sqlx::Result<Vec<ChatMessage>> {
        let messages: sqlx::Result<Vec<ChatMessage>> = query_as!(
            SqlChatMessage,
            r#"
            SELECT
                message_id AS "message_id: _",
                mimi_id AS "mimi_id: _",
                chat_id AS "chat_id: _",
                timestamp AS "timestamp: _",
                sender_user_uuid AS "sender_user_uuid: _",
                sender_user_domain AS "sender_user_domain: _",
                content AS "content: _",
                sent,
                status,
                edited_at AS "edited_at: _",
                b.user_uuid IS NOT NULL AS "is_blocked!: _",
                in_reply_to_mimi_id AS "in_reply_to_mimi_id: _"
            FROM message
            LEFT JOIN blocked_contact b ON b.user_uuid = sender_user_uuid
                AND b.user_domain = sender_user_domain
            WHERE chat_id = ?
            ORDER BY timestamp DESC
            LIMIT ?"#,
            chat_id,
            number_of_messages,
        )
        .fetch(txn.as_mut())
        .filter_map(|res| {
            let message: sqlx::Result<ChatMessage> = res
                // skip messages that we can't decode, but don't fail loading the rest of the
                // messages
                .inspect_err(|e| warn!("Error loading message: {e}"))
                .ok()?
                .try_into()
                .map_err(From::from);
            Some(message)
        })
        .collect()
        .await;

        let mut messages = messages?;
        messages.reverse();

        for message in messages.iter_mut() {
            // we don't want to fail for all messages if one load operation fails
            if let Err(error) = message.try_load_replied_message(txn).await {
                error!(%error, "failed to load reply for message");
            }
        }

        Ok(messages)
    }

    /// Augments a chat message when it is a reply with the data from the referenced message
    async fn try_load_replied_message(
        &mut self,
        txn: &mut SqliteTransaction<'_>,
    ) -> sqlx::Result<()> {
        if let Some((mimi_id, message)) = self.in_reply_to.as_mut() {
            *message = InReplyToMessage::load(txn, mimi_id).await?;
        }

        Ok(())
    }

    pub(crate) async fn store(
        &self,
        executor: impl SqliteExecutor<'_>,
        notifier: &mut StoreNotifier,
    ) -> anyhow::Result<()> {
        let (sender_uuid, sender_domain, mimi_id) = match &self.timestamped_message.message {
            Message::Content(content_message) => (
                Some(content_message.sender.uuid()),
                Some(content_message.sender.domain()),
                Some(content_message.mimi_id()),
            ),
            Message::Event(_) => (None, None, None),
        };
        let content = match &self.timestamped_message.message {
            Message::Content(content_message) => {
                VersionedMessage::from_mimi_content(&content_message.content)?
            }
            Message::Event(event_message) => VersionedMessage::from_event_message(event_message)?,
        };
        let content = BlobEncoded(&content);
        let sent = match &self.timestamped_message.message {
            Message::Content(content_message) => content_message.sent,
            Message::Event(_) => true,
        };
        let in_reply_to_mimi_id = self
            .timestamped_message
            .message
            .mimi_content()
            .and_then(|content| content.in_reply_to.as_ref())
            .and_then(|bytes| {
                MimiId::from_slice(bytes)
                    .inspect_err(|error| {
                        error!(%error, "failed to decode in_reply_to MimiId");
                    })
                    .ok()
            });
        let in_reply_to_mimi_id = in_reply_to_mimi_id.as_ref();

        query!(
            "INSERT INTO message (
                message_id,
                mimi_id,
                chat_id,
                in_reply_to_mimi_id,
                timestamp,
                sender_user_uuid,
                sender_user_domain,
                content,
                sent
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            self.message_id,
            mimi_id,
            self.chat_id,
            in_reply_to_mimi_id,
            self.timestamped_message.timestamp,
            sender_uuid,
            sender_domain,
            content,
            sent,
        )
        .execute(executor)
        .await?;

        notifier.add(self.message_id).update(self.chat_id);
        Ok(())
    }

    pub(crate) async fn update(
        &self,
        executor: impl SqliteExecutor<'_>,
        notifier: &mut StoreNotifier,
    ) -> anyhow::Result<()> {
        let mimi_id = self.message().mimi_id();
        let content = match &self.timestamped_message.message {
            Message::Content(content_message) => {
                VersionedMessage::from_mimi_content(&content_message.content)?
            }
            Message::Event(event_message) => VersionedMessage::from_event_message(event_message)?,
        };
        let content = BlobEncoded(&content);
        let sent = match &self.timestamped_message.message {
            Message::Content(content_message) => content_message.sent,
            Message::Event(_) => true,
        };
        let edited_at = self.edited_at();
        let status = self.status().repr();
        let message_id = self.id();

        query!(
            "UPDATE message
            SET
                mimi_id = ?,
                timestamp = ?,
                content = ?,
                sent = ?,
                edited_at = ?,
                status = ?
            WHERE message_id = ?",
            mimi_id,
            self.timestamped_message.timestamp,
            content,
            sent,
            edited_at,
            status,
            message_id,
        )
        .execute(executor)
        .await?;

        notifier.update(self.id());
        notifier.update(self.chat_id);
        Ok(())
    }

    /// Delete a message from the database.
    ///
    /// This removes the message row entirely. This will also remove associated
    /// edit history and status records via foreign key cascade.
    pub(crate) async fn delete(
        executor: impl SqliteExecutor<'_>,
        notifier: &mut StoreNotifier,
        message_id: MessageId,
        chat_id: ChatId,
    ) -> sqlx::Result<()> {
        query!("DELETE FROM message WHERE message_id = ?", message_id)
            .execute(executor)
            .await?;
        notifier.remove(message_id);
        notifier.update(chat_id);
        Ok(())
    }

    /// Set the message's sent status in the database and update the message's timestamp.
    pub(super) async fn update_sent_status(
        executor: impl SqliteExecutor<'_>,
        notifier: &mut StoreNotifier,
        message_id: MessageId,
        timestamp: TimeStamp,
        sent: bool,
    ) -> sqlx::Result<()> {
        let res = query!(
            "UPDATE message SET timestamp = ?, sent = ? WHERE message_id = ?",
            timestamp,
            sent,
            message_id,
        )
        .execute(executor)
        .await?;
        if res.rows_affected() == 1 {
            notifier.update(message_id);
        }
        Ok(())
    }

    /// Get the last message in the chat.
    pub(crate) async fn last_message(
        txn: &mut SqliteTransaction<'_>,
        chat_id: ChatId,
    ) -> sqlx::Result<Option<Self>> {
        query_as!(
            SqlChatMessage,
            r#"SELECT
                message_id AS "message_id: _",
                mimi_id AS "mimi_id: _",
                chat_id AS "chat_id: _",
                timestamp AS "timestamp: _",
                sender_user_uuid AS "sender_user_uuid: _",
                sender_user_domain AS "sender_user_domain: _",
                content AS "content: _",
                sent,
                status,
                edited_at AS "edited_at: _",
                b.user_uuid IS NOT NULL AS "is_blocked!: _",
                in_reply_to_mimi_id AS "in_reply_to_mimi_id: _"
            FROM message
            LEFT JOIN blocked_contact b ON b.user_uuid = sender_user_uuid
                AND b.user_domain = sender_user_domain
            WHERE chat_id = ?
            ORDER BY timestamp DESC LIMIT 1"#,
            chat_id,
        )
        .fetch_optional(txn.as_mut())
        .await?
        .map(TryFrom::try_from)
        .transpose()?
        .try_load_replied_message(txn)
        .await
    }

    /// Get the last content message in the chat which is owned by the given user.
    pub(crate) async fn last_content_message_by_user(
        txn: &mut SqliteTransaction<'_>,
        chat_id: ChatId,
        user_id: &UserId,
    ) -> sqlx::Result<Option<Self>> {
        let user_uuid = user_id.uuid();
        let user_domain = user_id.domain();
        query_as!(
            SqlChatMessage,
            r#"SELECT
                message_id AS "message_id: _",
                chat_id AS "chat_id: _",
                mimi_id AS "mimi_id: _",
                timestamp AS "timestamp: _",
                sender_user_uuid AS "sender_user_uuid: _",
                sender_user_domain AS "sender_user_domain: _",
                content AS "content: _",
                sent,
                status,
                edited_at AS "edited_at: _",
                b.user_uuid IS NOT NULL AS "is_blocked!: _",
                in_reply_to_mimi_id AS "in_reply_to_mimi_id: _"
            FROM message
            LEFT JOIN blocked_contact b ON b.user_uuid = sender_user_uuid
                AND b.user_domain = sender_user_domain
            WHERE chat_id = ?
                AND sender_user_uuid = ?
                AND sender_user_domain = ?
            ORDER BY timestamp DESC LIMIT 1"#,
            chat_id,
            user_uuid,
            user_domain,
        )
        .fetch_optional(txn.as_mut())
        .await?
        .map(TryFrom::try_from)
        .transpose()?
        .try_load_replied_message(txn)
        .await
    }

    pub(crate) async fn prev_message(
        txn: &mut SqliteTransaction<'_>,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> sqlx::Result<Option<ChatMessage>> {
        query_as!(
            SqlChatMessage,
            r#"SELECT
                message_id AS "message_id: _",
                mimi_id AS "mimi_id: _",
                chat_id AS "chat_id: _",
                timestamp AS "timestamp: _",
                sender_user_uuid AS "sender_user_uuid: _",
                sender_user_domain AS "sender_user_domain: _",
                content AS "content: _",
                sent,
                status,
                edited_at AS "edited_at: _",
                b.user_uuid IS NOT NULL AS "is_blocked!: _",
                in_reply_to_mimi_id AS "in_reply_to_mimi_id: _"
            FROM message
            LEFT JOIN blocked_contact b ON b.user_uuid = sender_user_uuid
                AND b.user_domain = sender_user_domain
            WHERE chat_id = ?2
                AND message_id != ?1
                AND timestamp <= (SELECT timestamp FROM message WHERE message_id = ?1)
            ORDER BY timestamp DESC
            LIMIT 1"#,
            message_id,
            chat_id,
        )
        .fetch_optional(txn.as_mut())
        .await?
        .map(TryFrom::try_from)
        .transpose()?
        .try_load_replied_message(txn)
        .await
    }

    pub(crate) async fn next_message(
        txn: &mut SqliteTransaction<'_>,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> sqlx::Result<Option<ChatMessage>> {
        query_as!(
            SqlChatMessage,
            r#"SELECT
                message_id AS "message_id: _",
                mimi_id AS "mimi_id: _",
                chat_id AS "chat_id: _",
                timestamp AS "timestamp: _",
                sender_user_uuid AS "sender_user_uuid: _",
                sender_user_domain AS "sender_user_domain: _",
                content AS "content: _",
                sent,
                status,
                edited_at AS "edited_at: _",
                b.user_uuid IS NOT NULL AS "is_blocked!: _",
                in_reply_to_mimi_id AS "in_reply_to_mimi_id: _"
            FROM message
            LEFT JOIN blocked_contact b ON b.user_uuid = sender_user_uuid
                AND b.user_domain = sender_user_domain
            WHERE chat_id = ?2
                AND message_id != ?1
                AND timestamp >= (SELECT timestamp FROM message WHERE message_id = ?1)
            ORDER BY timestamp ASC
            LIMIT 1"#,
            message_id,
            chat_id,
        )
        .fetch_optional(txn.as_mut())
        .await?
        .map(TryFrom::try_from)
        .transpose()?
        .try_load_replied_message(txn)
        .await
    }

    pub(crate) async fn redact_all_in_reply_to_mimi_ids(
        executor: impl SqliteExecutor<'_>,
        original_message_id: &MessageId,
        original_mimi_id: &MimiId,
        replaces: &MimiId,
    ) -> sqlx::Result<Vec<MessageId>> {
        query_scalar!(
            r#"
            WITH target_mimi_ids AS(
                SELECT mimi_id FROM message_edit WHERE message_id = ?
                UNION ALL
                SELECT ?
            )
            UPDATE message
            SET in_reply_to_mimi_id = ?
            WHERE in_reply_to_mimi_id IN (SELECT * FROM target_mimi_ids)
            RETURNING message_id AS "message_id: _"
            "#,
            original_message_id,
            original_mimi_id,
            replaces
        )
        .fetch_all(executor)
        .await
    }
}

#[derive(Debug)]
struct SqlInReplyToReponse {
    message_id: MessageId,
    sender_user_uuid: Option<Uuid>,
    sender_user_domain: Option<Fqdn>,
    content: Option<BlobDecoded<VersionedMessage>>,
}

impl From<SqlInReplyToReponse> for Option<InReplyToMessage> {
    fn from(
        SqlInReplyToReponse {
            message_id,
            sender_user_uuid,
            sender_user_domain,
            content,
        }: SqlInReplyToReponse,
    ) -> Self {
        Some(InReplyToMessage {
            message_id,
            sender: UserId::new(sender_user_uuid?, sender_user_domain?),
            mimi_content: content.and_then(|BlobDecoded(c)| c.to_mimi_content().ok()),
        })
    }
}

/// Small trait to be able to augment the result of a sqlx::fetch operation on ChatMessage
trait SqlChatMessageExt
where
    Self: Sized,
{
    async fn try_load_replied_message(self, txn: &mut SqliteTransaction<'_>) -> sqlx::Result<Self>;
}

impl SqlChatMessageExt for &mut ChatMessage {
    async fn try_load_replied_message(self, txn: &mut SqliteTransaction<'_>) -> sqlx::Result<Self> {
        if let Some((in_reply_to_mimi_id, in_reply_to_message)) = self.in_reply_to.as_mut() {
            *in_reply_to_message = InReplyToMessage::load(txn, in_reply_to_mimi_id).await?;
        }

        Ok(self)
    }
}

impl SqlChatMessageExt for Option<ChatMessage> {
    async fn try_load_replied_message(
        mut self,
        txn: &mut SqliteTransaction<'_>,
    ) -> sqlx::Result<Self> {
        if let Some(chat_message) = self.as_mut() {
            chat_message.try_load_replied_message(txn).await?;
        }

        Ok(self)
    }
}

impl InReplyToMessage {
    pub(crate) async fn load(
        txn: &mut SqliteTransaction<'_>,
        mimi_id: &MimiId,
    ) -> sqlx::Result<Option<Self>> {
        let mimi_id = mimi_id.as_slice();

        // Try to load the message that was replied to
        let in_reply_to_message = if let Some(replied_to_message) = query_as!(
            SqlInReplyToReponse,
            r#"
            SELECT
                message_id AS "message_id: _",
                sender_user_uuid AS "sender_user_uuid: _",
                sender_user_domain AS "sender_user_domain: _",
                content AS "content: _"
            FROM message
            WHERE mimi_id = ?
            "#,
            mimi_id,
        )
        .fetch_optional(txn.as_mut())
        .await?
        {
            replied_to_message.into()
        }
        // If we didn't find it, try to load a message edit (previous version)
        else if let Some(replied_to_message_edit) = query_as!(
            SqlInReplyToReponse,
            r#"
            SELECT
                me.message_id AS "message_id: _",
                m.sender_user_uuid AS "sender_user_uuid: _",
                m.sender_user_domain AS "sender_user_domain: _",
                me.content AS "content: _"
            FROM message_edit me
            LEFT JOIN message m ON m.message_id = me.message_id
            WHERE me.mimi_id = ?
            "#,
            mimi_id,
        )
        .fetch_optional(txn.as_mut())
        .await?
        {
            replied_to_message_edit.into()
        } else {
            None
        };

        Ok(in_reply_to_message)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::LazyLock;

    use aircommon::{identifiers::UserId, time::TimeStamp};
    use chrono::Utc;
    use mimi_content::MimiContent;
    use openmls::group::GroupId;
    use sqlx::SqlitePool;

    use crate::{ContentMessage, Message, MessageId, chats::persistence::tests::test_chat};

    use super::*;

    pub(crate) fn test_chat_message(chat_id: ChatId) -> ChatMessage {
        test_chat_message_with_salt(chat_id, [0; 16])
    }

    pub(crate) fn test_chat_message_with_salt(chat_id: ChatId, salt: [u8; 16]) -> ChatMessage {
        let chat_message_id = MessageId::random();
        let timestamp = Utc::now().into();
        let message = Message::Content(Box::new(ContentMessage::new(
            UserId::random("localhost".parse().unwrap()),
            false,
            MimiContent::simple_markdown_message("Hello world!".to_string(), salt),
            &GroupId::from_slice(&[0]),
        )));
        let timestamped_message = TimestampedMessage { timestamp, message };
        ChatMessage {
            message_id: chat_message_id,
            chat_id,
            timestamped_message,
            status: MessageStatus::Unread,
            in_reply_to: None,
        }
    }

    #[sqlx::test]
    async fn store_load(pool: SqlitePool) -> anyhow::Result<()> {
        let mut store_notifier = StoreNotifier::noop();

        let chat = test_chat();
        chat.store(pool.acquire().await?.as_mut(), &mut store_notifier)
            .await?;

        let message = test_chat_message(chat.id());

        message.store(&pool, &mut store_notifier).await?;

        let mut txn = pool.begin().await?;
        let loaded = ChatMessage::load(&mut txn, message.id()).await?.unwrap();

        assert_eq!(loaded, message);

        Ok(())
    }

    #[sqlx::test]
    async fn prev_next_do_not_cross_chat_boundaries(pool: SqlitePool) -> anyhow::Result<()> {
        let mut store_notifier = StoreNotifier::noop();

        let mut connection = pool.acquire().await?;

        let chat_a = test_chat();
        chat_a
            .store(connection.as_mut(), &mut store_notifier)
            .await?;

        let chat_b = test_chat();
        chat_b
            .store(connection.as_mut(), &mut store_notifier)
            .await?;

        let group_id = GroupId::from_slice(&[0]);
        let sender = UserId::random("localhost".parse().unwrap());

        let message_a = ChatMessage::new_for_test(
            chat_a.id(),
            MessageId::random(),
            TimeStamp::from(1_000_000_000_i64),
            Message::Content(Box::new(ContentMessage::new(
                sender.clone(),
                true,
                MimiContent::simple_markdown_message("a".to_string(), [0; 16]),
                &group_id,
            ))),
        );
        message_a.store(&pool, &mut store_notifier).await?;

        let message_b = ChatMessage::new_for_test(
            chat_b.id(),
            MessageId::random(),
            TimeStamp::from(2_000_000_000_i64),
            ContentMessage::new(
                sender,
                true,
                MimiContent::simple_markdown_message("b".to_string(), [1; 16]),
                &group_id,
            ),
        );
        message_b.store(&pool, &mut store_notifier).await?;

        let mut txn = pool.begin().await?;

        let prev = ChatMessage::prev_message(&mut txn, chat_b.id(), message_b.id()).await?;
        assert!(
            prev.is_none(),
            "prev_message should ignore messages from other chats"
        );

        let next = ChatMessage::next_message(&mut txn, chat_a.id(), message_a.id()).await?;
        assert!(
            next.is_none(),
            "next_message should ignore messages from other chats"
        );

        Ok(())
    }

    #[sqlx::test]
    async fn store_load_multiple(pool: SqlitePool) -> anyhow::Result<()> {
        let mut store_notifier = StoreNotifier::noop();
        let mut txn = pool.begin().await?;

        let chat = test_chat();
        chat.store(txn.as_mut(), &mut store_notifier).await?;

        let message_a = test_chat_message(chat.id());
        let message_b = test_chat_message(chat.id());

        message_a.store(txn.as_mut(), &mut store_notifier).await?;
        message_b.store(txn.as_mut(), &mut store_notifier).await?;

        let loaded = ChatMessage::load_multiple(&mut txn, chat.id(), 2).await?;
        assert_eq!(loaded, [message_a, message_b.clone()]);

        let loaded = ChatMessage::load_multiple(&mut txn, chat.id(), 1).await?;
        assert_eq!(loaded, [message_b]);

        Ok(())
    }

    #[sqlx::test]
    async fn update_sent_status(pool: SqlitePool) -> anyhow::Result<()> {
        let mut store_notifier = StoreNotifier::noop();
        let mut txn = pool.begin().await?;

        let chat = test_chat();
        chat.store(&mut txn, &mut store_notifier).await?;

        let message = test_chat_message(chat.id());
        message.store(txn.as_mut(), &mut store_notifier).await?;

        let loaded = ChatMessage::load(&mut txn, message.id()).await?.unwrap();
        assert!(!loaded.is_sent());

        let sent_at: TimeStamp = Utc::now().into();
        ChatMessage::update_sent_status(
            txn.as_mut(),
            &mut store_notifier,
            loaded.id(),
            sent_at,
            true,
        )
        .await?;

        let loaded = ChatMessage::load(&mut txn, message.id()).await?.unwrap();
        assert_eq!(&loaded.timestamp(), sent_at.as_ref());
        assert!(loaded.is_sent());

        Ok(())
    }

    #[sqlx::test]
    async fn last_message(pool: SqlitePool) -> anyhow::Result<()> {
        let mut store_notifier = StoreNotifier::noop();

        let chat = test_chat();
        chat.store(pool.acquire().await?.as_mut(), &mut store_notifier)
            .await?;

        let message_a = test_chat_message(chat.id());
        let message_b = test_chat_message(chat.id());

        message_a.store(&pool, &mut store_notifier).await?;
        message_b.store(&pool, &mut store_notifier).await?;

        let mut txn = pool.begin().await?;

        let loaded = ChatMessage::last_message(&mut txn, chat.id()).await?;
        assert_eq!(loaded, Some(message_b));

        Ok(())
    }

    #[sqlx::test]
    async fn prev_message(pool: SqlitePool) -> anyhow::Result<()> {
        let mut store_notifier = StoreNotifier::noop();
        let mut txn = pool.begin().await?;

        let chat = test_chat();
        chat.store(&mut txn, &mut store_notifier).await?;

        let message_a = test_chat_message(chat.id());
        let message_b = test_chat_message(chat.id());

        message_a.store(txn.as_mut(), &mut store_notifier).await?;
        message_b.store(txn.as_mut(), &mut store_notifier).await?;

        let loaded = ChatMessage::prev_message(&mut txn, chat.id(), message_b.id()).await?;
        assert_eq!(loaded, Some(message_a));

        Ok(())
    }

    #[sqlx::test]
    async fn next_message(pool: SqlitePool) -> anyhow::Result<()> {
        let mut store_notifier = StoreNotifier::noop();
        let mut txn = pool.begin().await?;

        let chat = test_chat();
        chat.store(txn.as_mut(), &mut store_notifier).await?;

        let message_a = test_chat_message(chat.id());
        let message_b = test_chat_message(chat.id());

        message_a.store(txn.as_mut(), &mut store_notifier).await?;
        message_b.store(txn.as_mut(), &mut store_notifier).await?;

        let loaded = ChatMessage::next_message(&mut txn, chat.id(), message_a.id()).await?;
        assert_eq!(loaded, Some(message_b));

        Ok(())
    }

    static VERSIONED_MESSAGE: LazyLock<VersionedMessage> = LazyLock::new(|| {
        VersionedMessage::from_mimi_content(&MimiContent::simple_markdown_message(
            "Hello world!".to_string(),
            [0; 16], // simple salt for testing
        ))
        .unwrap()
    });

    #[test]
    fn versioned_message_serde_codec() {
        let bytes = PersistenceCodec::to_vec(&*VERSIONED_MESSAGE).unwrap();
        let diag = cbor_diag::parse_bytes(&bytes[1..]).unwrap().to_hex();
        insta::assert_snapshot!(diag);
    }

    #[test]
    fn versioned_message_serde_json() {
        insta::assert_json_snapshot!(&*VERSIONED_MESSAGE);
    }

    #[sqlx::test]
    async fn delete_message(pool: SqlitePool) -> anyhow::Result<()> {
        let mut store_notifier = StoreNotifier::noop();
        let mut txn = pool.begin().await?;

        let chat = test_chat();
        chat.store(txn.as_mut(), &mut store_notifier).await?;

        let message = test_chat_message(chat.id());
        message.store(txn.as_mut(), &mut store_notifier).await?;

        // Verify message exists
        let loaded = ChatMessage::load(&mut txn, message.id()).await?;
        assert!(loaded.is_some());

        // Delete message
        ChatMessage::delete(txn.as_mut(), &mut store_notifier, message.id(), chat.id()).await?;

        // Verify message is gone
        let loaded = ChatMessage::load(&mut txn, message.id()).await?;
        assert!(loaded.is_none());

        Ok(())
    }

    #[sqlx::test]
    async fn delete_message_cascade_edit_history(pool: SqlitePool) -> anyhow::Result<()> {
        use crate::chats::messages::edit::MessageEdit;
        use aircommon::identifiers::MimiId;

        let mut store_notifier = StoreNotifier::noop();
        let mut txn = pool.begin().await?;

        let chat = test_chat();
        chat.store(txn.as_mut(), &mut store_notifier).await?;

        let message = test_chat_message(chat.id());
        message.store(txn.as_mut(), &mut store_notifier).await?;

        // Create edit history entry
        let mimi_id = MimiId::from_slice(&[1u8; 32])?;
        let edit_content = MimiContent::simple_markdown_message("Edited!".to_string(), [1; 16]);
        let edit = MessageEdit::new(&mimi_id, message.id(), TimeStamp::now(), &edit_content);
        edit.store(txn.as_mut()).await?;

        // Verify edit history exists
        let found = MessageEdit::find_message_id(txn.as_mut(), &mimi_id).await?;
        assert_eq!(found, Some(message.id()));

        // Delete message - should cascade to edit history
        ChatMessage::delete(txn.as_mut(), &mut store_notifier, message.id(), chat.id()).await?;

        // Verify message is gone
        let loaded = ChatMessage::load(&mut txn, message.id()).await?;
        assert!(loaded.is_none());

        // Verify edit history is also gone (FK cascade)
        let found = MessageEdit::find_message_id(txn.as_mut(), &mimi_id).await?;
        assert!(found.is_none());

        Ok(())
    }

    #[sqlx::test]
    async fn delete_message_cascade_status_records(pool: SqlitePool) -> anyhow::Result<()> {
        use crate::chats::status::StatusRecord;
        use mimi_content::{MessageStatusReport, PerMessageStatus};
        use sqlx::query_scalar;

        let mut store_notifier = StoreNotifier::noop();
        let mut txn = pool.begin().await?;

        let chat = test_chat();
        chat.store(txn.as_mut(), &mut store_notifier).await?;

        let message = test_chat_message_with_salt(chat.id(), [0; 16]);
        message.store(txn.as_mut(), &mut store_notifier).await?;

        let mimi_id = message.message().mimi_id().unwrap();

        // Create status record
        let sender = UserId::random("localhost".parse().unwrap());
        let report = MessageStatusReport {
            statuses: vec![PerMessageStatus {
                mimi_id: mimi_id.as_ref().to_vec().into(),
                status: mimi_content::MessageStatus::Delivered,
            }],
        };
        StatusRecord::borrowed(&sender, report, TimeStamp::now())
            .store_report(&mut txn, &mut store_notifier)
            .await?;
        txn.commit().await?;

        let mut txn = pool.begin().await?;

        // Verify status record exists
        let count: i64 = query_scalar("SELECT COUNT(*) FROM message_status WHERE message_id = ?")
            .bind(message.id())
            .fetch_one(txn.as_mut())
            .await?;
        assert_eq!(count, 1);

        // Delete message - should cascade to status records
        ChatMessage::delete(txn.as_mut(), &mut store_notifier, message.id(), chat.id()).await?;

        // Verify message is gone
        let loaded = ChatMessage::load(&mut txn, message.id()).await?;
        assert!(loaded.is_none());

        // Verify status records are also gone (FK cascade)
        let count: i64 = query_scalar("SELECT COUNT(*) FROM message_status WHERE message_id = ?")
            .bind(message.id())
            .fetch_one(txn.as_mut())
            .await?;
        assert_eq!(count, 0);

        Ok(())
    }

    #[sqlx::test]
    async fn delete_preserves_other_messages(pool: SqlitePool) -> anyhow::Result<()> {
        let mut store_notifier = StoreNotifier::noop();
        let mut txn = pool.begin().await?;

        let chat = test_chat();
        chat.store(&mut txn, &mut store_notifier).await?;

        let message_a = test_chat_message_with_salt(chat.id(), [0; 16]);
        let message_b = test_chat_message_with_salt(chat.id(), [1; 16]);
        let message_c = test_chat_message_with_salt(chat.id(), [2; 16]);

        message_a.store(txn.as_mut(), &mut store_notifier).await?;
        message_b.store(txn.as_mut(), &mut store_notifier).await?;
        message_c.store(txn.as_mut(), &mut store_notifier).await?;

        // Delete only message_b
        ChatMessage::delete(txn.as_mut(), &mut store_notifier, message_b.id(), chat.id()).await?;

        // Verify message_b is gone
        let loaded_b = ChatMessage::load(&mut txn, message_b.id()).await?;
        assert!(loaded_b.is_none());

        // Verify message_a and message_c still exist
        let loaded_a = ChatMessage::load(&mut txn, message_a.id()).await?;
        let loaded_c = ChatMessage::load(&mut txn, message_c.id()).await?;
        assert_eq!(loaded_a, Some(message_a));
        assert_eq!(loaded_c, Some(message_c));

        Ok(())
    }

    #[sqlx::test]
    async fn delete_nonexistent_message(pool: SqlitePool) -> anyhow::Result<()> {
        let mut store_notifier = StoreNotifier::noop();
        let mut txn = pool.begin().await?;

        let chat = test_chat();
        chat.store(txn.as_mut(), &mut store_notifier).await?;

        // Try to delete a message that doesn't exist
        let fake_message_id = MessageId::random();
        let result = ChatMessage::delete(
            txn.as_mut(),
            &mut store_notifier,
            fake_message_id,
            chat.id(),
        )
        .await;

        // Should succeed without error (no-op)
        assert!(result.is_ok());

        Ok(())
    }
}
