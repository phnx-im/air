// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;

use aircommon::{
    codec::{BlobDecoded, BlobEncoded, PersistenceCodec},
    identifiers::{Fqdn, MimiId, UserId},
    time::TimeStamp,
};
use anyhow::bail;
use mimi_content::{MessageStatus, MimiContent};
use serde::{Deserialize, Serialize};
use sqlx::{SqliteConnection, query, query_as, query_scalar};
use tokio_stream::StreamExt;
use tracing::{error, warn};
use uuid::Uuid;

use crate::{
    ChatId, ChatMessage, ContentMessage, Message,
    chats::messages::InReplyToMessage,
    db_access::{ReadConnection, WriteConnection},
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

impl From<SqlChatMessage> for ChatMessage {
    fn from(
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
    ) -> Self {
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

        ChatMessage {
            message_id,
            chat_id,
            in_reply_to: in_reply_to_mimi_id.map(|id| (id, None)),
            timestamped_message,
            status,
        }
    }
}

impl ChatMessage {
    pub async fn load(
        mut connection: impl ReadConnection,
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
        .fetch_optional(connection.as_mut())
        .await?
        .map(ChatMessage::from)
        .with_loaded_in_reply_to(connection.as_mut())
        .await
    }

    pub(crate) async fn load_by_mimi_id(
        mut connection: impl ReadConnection,
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
        .fetch_optional(connection.as_mut())
        .await?
        .map(ChatMessage::from)
        .with_loaded_in_reply_to(connection.as_mut())
        .await
    }

    /// Decode a single row from the query stream, skipping rows that fail.
    fn decode_row(res: sqlx::Result<SqlChatMessage>) -> Option<sqlx::Result<ChatMessage>> {
        let message = res
            .inspect_err(|e| warn!("Error loading message: {e}"))
            .ok()?;
        Some(Ok(message.into()))
    }

    /// Trim the extra sentinel row used to detect more messages,
    /// optionally reverse, and return whether more messages exist.
    fn trim_sentinel(messages: &mut Vec<ChatMessage>, limit: u32, reverse: bool) -> bool {
        let has_more = messages.len() > limit as usize;
        messages.truncate(limit as usize);
        if reverse {
            messages.reverse();
        }
        has_more
    }

    pub(crate) async fn load_multiple(
        mut connection: impl ReadConnection,
        chat_id: ChatId,
        number_of_messages: u32,
    ) -> sqlx::Result<Vec<ChatMessage>> {
        let mut messages: Vec<ChatMessage> = query_as!(
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
            ORDER BY timestamp DESC, message_id DESC
            LIMIT ?"#,
            chat_id,
            number_of_messages,
        )
        .fetch(connection.as_mut())
        .filter_map(Self::decode_row)
        .collect::<sqlx::Result<Vec<_>>>()
        .await?;

        messages.reverse();
        let messages = messages
            .with_loaded_in_reply_to(connection.as_mut())
            .await?;
        Ok(messages)
    }

    /// Load messages before (older than) the given cursor, in ascending order.
    ///
    /// Uses a composite `(timestamp, message_id)` cursor to ensure stable
    /// pagination even when multiple messages share the same timestamp.
    ///
    /// Returns `(messages, has_older)` where `has_older` indicates more messages
    /// exist before the returned window.
    pub(crate) async fn load_before(
        mut connection: impl ReadConnection,
        chat_id: ChatId,
        before: TimeStamp,
        before_id: MessageId,
        limit: u32,
    ) -> sqlx::Result<(Vec<ChatMessage>, bool)> {
        let fetch_limit = limit + 1;
        let mut messages: Vec<ChatMessage> = query_as!(
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
            WHERE chat_id = ?1 AND (timestamp, message_id) < (?2, ?3)
            ORDER BY timestamp DESC, message_id DESC
            LIMIT ?4"#,
            chat_id,
            before,
            before_id,
            fetch_limit,
        )
        .fetch(connection.as_mut())
        .filter_map(Self::decode_row)
        .collect::<sqlx::Result<Vec<_>>>()
        .await?;

        let has_older = Self::trim_sentinel(&mut messages, limit, true);
        let messages = messages
            .with_loaded_in_reply_to(connection.as_mut())
            .await?;
        Ok((messages, has_older))
    }

    /// Load messages after (newer than) the given cursor, in ascending order.
    ///
    /// Uses a composite `(timestamp, message_id)` cursor to ensure stable
    /// pagination even when multiple messages share the same timestamp.
    ///
    /// Returns `(messages, has_newer)` where `has_newer` indicates more messages
    /// exist after the returned window.
    pub(crate) async fn load_after(
        mut connection: impl ReadConnection,
        chat_id: ChatId,
        after: TimeStamp,
        after_id: MessageId,
        limit: u32,
    ) -> sqlx::Result<(Vec<ChatMessage>, bool)> {
        let fetch_limit = limit + 1;
        let mut messages: Vec<ChatMessage> = query_as!(
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
            WHERE chat_id = ?1 AND (timestamp, message_id) > (?2, ?3)
            ORDER BY timestamp ASC, message_id ASC
            LIMIT ?4"#,
            chat_id,
            after,
            after_id,
            fetch_limit,
        )
        .fetch(connection.as_mut())
        .filter_map(Self::decode_row)
        .collect::<sqlx::Result<Vec<_>>>()
        .await?;

        let has_newer = Self::trim_sentinel(&mut messages, limit, false);
        let messages = messages
            .with_loaded_in_reply_to(connection.as_mut())
            .await?;
        Ok((messages, has_newer))
    }

    /// Load messages starting from (inclusive) an anchor, in ascending order.
    ///
    /// Uses a composite `(timestamp, message_id)` cursor with `>=` so the
    /// anchor message itself is included.
    ///
    /// Returns `(messages, has_newer)`.
    pub(crate) async fn load_starting_from(
        connection: &mut SqliteConnection,
        chat_id: ChatId,
        from: TimeStamp,
        from_id: MessageId,
        limit: u32,
    ) -> sqlx::Result<(Vec<ChatMessage>, bool)> {
        let fetch_limit = limit + 1;
        let mut messages: Vec<ChatMessage> = query_as!(
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
            WHERE chat_id = ?1 AND (timestamp, message_id) >= (?2, ?3)
            ORDER BY timestamp ASC, message_id ASC
            LIMIT ?4"#,
            chat_id,
            from,
            from_id,
            fetch_limit,
        )
        .fetch(&mut *connection)
        .filter_map(Self::decode_row)
        .collect::<sqlx::Result<Vec<_>>>()
        .await?;

        let has_newer = Self::trim_sentinel(&mut messages, limit, false);
        let messages = messages.with_loaded_in_reply_to(connection).await?;
        Ok((messages, has_newer))
    }

    /// Load messages around an anchor, in ascending order.
    ///
    /// Uses a composite `(timestamp, message_id)` cursor. The anchor message
    /// itself is included in the backward half (uses `<=`).
    ///
    /// Returns `(messages, has_older, has_newer)`.
    pub(crate) async fn load_around(
        connection: &mut SqliteConnection,
        chat_id: ChatId,
        anchor: TimeStamp,
        anchor_id: MessageId,
        half_limit: u32,
    ) -> sqlx::Result<(Vec<ChatMessage>, bool, bool)> {
        let fetch_half = half_limit + 1;

        // Backward half: includes the anchor message
        let mut backward: Vec<ChatMessage> = query_as!(
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
            WHERE chat_id = ?1 AND (timestamp, message_id) <= (?2, ?3)
            ORDER BY timestamp DESC, message_id DESC
            LIMIT ?4"#,
            chat_id,
            anchor,
            anchor_id,
            fetch_half,
        )
        .fetch(&mut *connection)
        .filter_map(Self::decode_row)
        .collect::<sqlx::Result<Vec<_>>>()
        .await?;

        let has_older = Self::trim_sentinel(&mut backward, half_limit, true);

        // Forward half: messages after the anchor
        let mut forward: Vec<ChatMessage> = query_as!(
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
            WHERE chat_id = ?1 AND (timestamp, message_id) > (?2, ?3)
            ORDER BY timestamp ASC, message_id ASC
            LIMIT ?4"#,
            chat_id,
            anchor,
            anchor_id,
            fetch_half,
        )
        .fetch(&mut *connection)
        .filter_map(Self::decode_row)
        .collect::<sqlx::Result<Vec<_>>>()
        .await?;

        let has_newer = Self::trim_sentinel(&mut forward, half_limit, false);

        backward.append(&mut forward);
        let messages = backward.with_loaded_in_reply_to(connection).await?;
        Ok((messages, has_older, has_newer))
    }

    /// Load the first unread content message in a chat after `last_read`.
    ///
    /// Only considers messages with a sender (excludes system/event messages).
    pub(crate) async fn first_unread_message(
        connection: &mut SqliteConnection,
        chat_id: ChatId,
        last_read: TimeStamp,
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
            WHERE chat_id = ?1
                AND timestamp > ?2
                AND sender_user_uuid IS NOT NULL
            ORDER BY timestamp ASC, message_id ASC
            LIMIT 1"#,
            chat_id,
            last_read,
        )
        .fetch_optional(&mut *connection)
        .await?
        .map(ChatMessage::from)
        .with_loaded_in_reply_to(connection)
        .await
    }

    /// Augments a chat message when it is a reply with the data from the referenced message
    async fn augment_in_reply_to(&mut self, connection: &mut SqliteConnection) -> sqlx::Result<()> {
        if let Some((mimi_id, message)) = self.in_reply_to.as_mut() {
            *message = InReplyToMessage::load(connection, mimi_id).await?;
        }

        Ok(())
    }

    pub(crate) async fn store(&self, mut connection: impl WriteConnection) -> anyhow::Result<()> {
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
        .execute(connection.as_mut())
        .await?;

        connection
            .notifier()
            .add(self.message_id)
            .update(self.chat_id);
        Ok(())
    }

    pub(crate) async fn update(&self, mut connection: impl WriteConnection) -> anyhow::Result<()> {
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
        .execute(connection.as_mut())
        .await?;

        connection.notifier().update(self.id());
        connection.notifier().update(self.chat_id);
        Ok(())
    }

    /// Delete a message from the database.
    ///
    /// This removes the message row entirely. This will also remove associated
    /// edit history and status records via foreign key cascade.
    pub(crate) async fn delete(
        mut connection: impl WriteConnection,
        message_id: MessageId,
    ) -> sqlx::Result<()> {
        let chat_id = query_as!(
            ChatId,
            "DELETE FROM message WHERE message_id = ? RETURNING chat_id AS 'uuid: _'",
            message_id
        )
        .fetch_optional(connection.as_mut())
        .await?;

        let notifier = connection.notifier();
        if let Some(chat_id) = chat_id {
            notifier.remove(message_id);
            notifier.update(chat_id);
        }
        Ok(())
    }

    /// Set the message's sent status in the database and update the message's timestamp.
    pub(super) async fn update_sent_status(
        mut connection: impl WriteConnection,
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
        .execute(connection.as_mut())
        .await?;
        if res.rows_affected() == 1 {
            connection.notifier().update(message_id);
        }
        Ok(())
    }

    /// Get the last message in the chat.
    pub(crate) async fn last_message(
        mut connection: impl ReadConnection,
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
        .fetch_optional(connection.as_mut())
        .await?
        .map(ChatMessage::from)
        .with_loaded_in_reply_to(connection.as_mut())
        .await
    }

    /// Get the last content message in the chat which is owned by the given user.
    pub(crate) async fn last_content_message_by_user(
        mut connection: impl ReadConnection,
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
        .fetch_optional(connection.as_mut())
        .await?
        .map(ChatMessage::from)
        .with_loaded_in_reply_to(connection.as_mut())
        .await
    }

    pub(crate) async fn prev_message(
        mut connection: impl ReadConnection,
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
        .fetch_optional(connection.as_mut())
        .await?
        .map(ChatMessage::from)
        .with_loaded_in_reply_to(connection.as_mut())
        .await
    }

    pub(crate) async fn next_message(
        mut connection: impl ReadConnection,
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
        .fetch_optional(connection.as_mut())
        .await?
        .map(ChatMessage::from)
        .with_loaded_in_reply_to(connection.as_mut())
        .await
    }

    pub(crate) async fn redact_all_in_reply_to_mimi_ids(
        mut connection: impl WriteConnection,
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
        .fetch_all(connection.as_mut())
        .await
    }

    pub(crate) async fn load_message_ids_in_reply_to_mimi_id(
        mut connection: impl ReadConnection,
        mimi_id: &MimiId,
    ) -> sqlx::Result<Vec<MessageId>> {
        query_as!(
            MessageId,
            r#"SELECT message_id AS 'uuid: _' FROM message WHERE in_reply_to_mimi_id = ?"#,
            mimi_id
        )
        .fetch_all(connection.as_mut())
        .await
    }
}

#[derive(Debug)]
struct SqlInReplyToMessage {
    message_id: MessageId,
    sender_user_uuid: Option<Uuid>,
    sender_user_domain: Option<Fqdn>,
    content: Option<BlobDecoded<VersionedMessage>>,
}

impl From<SqlInReplyToMessage> for Option<InReplyToMessage> {
    fn from(
        SqlInReplyToMessage {
            message_id,
            sender_user_uuid,
            sender_user_domain,
            content,
        }: SqlInReplyToMessage,
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
    async fn with_loaded_in_reply_to(self, c: &mut SqliteConnection) -> sqlx::Result<Self>;
}

impl SqlChatMessageExt for &mut ChatMessage {
    async fn with_loaded_in_reply_to(
        self,
        connection: &mut SqliteConnection,
    ) -> sqlx::Result<Self> {
        if let Some((in_reply_to_mimi_id, in_reply_to_message)) = self.in_reply_to.as_mut() {
            *in_reply_to_message = InReplyToMessage::load(connection, in_reply_to_mimi_id).await?;
        }

        Ok(self)
    }
}

impl SqlChatMessageExt for Vec<ChatMessage> {
    async fn with_loaded_in_reply_to(
        mut self,
        connection: &mut SqliteConnection,
    ) -> sqlx::Result<Self> {
        for message in &mut self {
            if let Err(error) = message.augment_in_reply_to(connection).await {
                error!(%error, "failed to load reply for message");
            }
        }
        Ok(self)
    }
}

impl SqlChatMessageExt for Option<ChatMessage> {
    async fn with_loaded_in_reply_to(
        mut self,
        connection: &mut SqliteConnection,
    ) -> sqlx::Result<Self> {
        if let Some(chat_message) = self.as_mut() {
            chat_message.augment_in_reply_to(connection).await?;
        }

        Ok(self)
    }
}

impl InReplyToMessage {
    pub(crate) async fn load(
        connection: &mut SqliteConnection,
        mimi_id: &MimiId,
    ) -> sqlx::Result<Option<Self>> {
        let mimi_id = mimi_id.as_slice();

        // Try to load the message that was replied to
        let in_reply_to_message = if let Some(in_reply_to_message) = query_as!(
            SqlInReplyToMessage,
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
        .fetch_optional(&mut *connection)
        .await?
        {
            in_reply_to_message.into()
        }
        // If we didn't find it, try to load a message edit (previous version)
        else if let Some(in_reply_to_message) = query_as!(
            SqlInReplyToMessage,
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
        .fetch_optional(&mut *connection)
        .await?
        {
            in_reply_to_message.into()
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
        test_chat_message_at(chat_id, salt, Utc::now().into())
    }

    pub(crate) fn test_chat_message_at(
        chat_id: ChatId,
        salt: [u8; 16],
        timestamp: TimeStamp,
    ) -> ChatMessage {
        let chat_message_id = MessageId::random();
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
        ChatMessage::delete(txn.as_mut(), &mut store_notifier, message.id()).await?;

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
        ChatMessage::delete(txn.as_mut(), &mut store_notifier, message.id()).await?;

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
        ChatMessage::delete(txn.as_mut(), &mut store_notifier, message.id()).await?;

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
        ChatMessage::delete(txn.as_mut(), &mut store_notifier, message_b.id()).await?;

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
        let result = ChatMessage::delete(txn.as_mut(), &mut store_notifier, fake_message_id).await;

        // Should succeed without error (no-op)
        assert!(result.is_ok());

        Ok(())
    }

    /// Helper to create a message with a specific timestamp (in seconds).
    fn message_at(chat_id: ChatId, secs: i64) -> ChatMessage {
        let sender = &*TEST_SENDER;
        let salt = secs.to_le_bytes();
        let mut salt_16 = [0u8; 16];
        salt_16[..8].copy_from_slice(&salt);
        ChatMessage::new_for_test(
            chat_id,
            MessageId::random(),
            TimeStamp::from(secs * 1_000_000_000),
            ContentMessage::new(
                sender.clone(),
                true,
                MimiContent::simple_markdown_message(format!("msg at {secs}"), salt_16),
                &GroupId::from_slice(&[0]),
            ),
        )
    }

    static TEST_SENDER: LazyLock<UserId> =
        LazyLock::new(|| UserId::random("localhost".parse().unwrap()));

    #[sqlx::test]
    async fn load_before(pool: SqlitePool) -> anyhow::Result<()> {
        let mut notifier = StoreNotifier::noop();
        let mut txn = pool.begin().await?;

        let chat = test_chat();
        chat.store(txn.as_mut(), &mut notifier).await?;

        // Store 5 messages at t=10,20,30,40,50
        let msgs: Vec<_> = [10, 20, 30, 40, 50]
            .into_iter()
            .map(|t| message_at(chat.id(), t))
            .collect();
        for m in &msgs {
            m.store(txn.as_mut(), &mut notifier).await?;
        }

        // Load 2 messages before t=35 -> should get t=20, t=30
        let (loaded, has_older) = ChatMessage::load_before(
            &mut txn,
            chat.id(),
            TimeStamp::from(35_000_000_000_i64),
            MessageId::new(Uuid::max()),
            2,
        )
        .await?;
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].id(), msgs[1].id()); // t=20
        assert_eq!(loaded[1].id(), msgs[2].id()); // t=30
        assert!(has_older); // t=10 exists

        // Load 10 messages before t=35 -> should get t=10, t=20, t=30 with has_older=false
        let (loaded, has_older) = ChatMessage::load_before(
            &mut txn,
            chat.id(),
            TimeStamp::from(35_000_000_000_i64),
            MessageId::new(Uuid::max()),
            10,
        )
        .await?;
        assert_eq!(loaded.len(), 3);
        assert!(!has_older);

        Ok(())
    }

    #[sqlx::test]
    async fn load_after(pool: SqlitePool) -> anyhow::Result<()> {
        let mut notifier = StoreNotifier::noop();
        let mut txn = pool.begin().await?;

        let chat = test_chat();
        chat.store(txn.as_mut(), &mut notifier).await?;

        let msgs: Vec<_> = [10, 20, 30, 40, 50]
            .into_iter()
            .map(|t| message_at(chat.id(), t))
            .collect();
        for m in &msgs {
            m.store(txn.as_mut(), &mut notifier).await?;
        }

        // Load 2 messages after t=25 -> should get t=30, t=40
        let (loaded, has_newer) = ChatMessage::load_after(
            &mut txn,
            chat.id(),
            TimeStamp::from(25_000_000_000_i64),
            MessageId::new(Uuid::nil()),
            2,
        )
        .await?;
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].id(), msgs[2].id()); // t=30
        assert_eq!(loaded[1].id(), msgs[3].id()); // t=40
        assert!(has_newer); // t=50 exists

        // Load 10 messages after t=25 -> should get t=30, t=40, t=50 with has_newer=false
        let (loaded, has_newer) = ChatMessage::load_after(
            &mut txn,
            chat.id(),
            TimeStamp::from(25_000_000_000_i64),
            MessageId::new(Uuid::nil()),
            10,
        )
        .await?;
        assert_eq!(loaded.len(), 3);
        assert!(!has_newer);

        Ok(())
    }

    #[sqlx::test]
    async fn load_around(pool: SqlitePool) -> anyhow::Result<()> {
        let mut notifier = StoreNotifier::noop();
        let mut txn = pool.begin().await?;

        let chat = test_chat();
        chat.store(txn.as_mut(), &mut notifier).await?;

        let msgs: Vec<_> = [10, 20, 30, 40, 50]
            .into_iter()
            .map(|t| message_at(chat.id(), t))
            .collect();
        for m in &msgs {
            m.store(txn.as_mut(), &mut notifier).await?;
        }

        // Load around t=30 with half_limit=2
        // Backward (<=30): t=30, t=20 (limit 2), has_older because t=10 exists
        // Forward (>30): t=40, t=50 (limit 2), has_newer=false
        let (loaded, has_older, has_newer) = ChatMessage::load_around(
            &mut txn,
            chat.id(),
            TimeStamp::from(30_000_000_000_i64),
            msgs[2].id(),
            2,
        )
        .await?;
        assert_eq!(loaded.len(), 4); // t=20, t=30, t=40, t=50
        assert_eq!(loaded[0].id(), msgs[1].id()); // t=20
        assert_eq!(loaded[1].id(), msgs[2].id()); // t=30 (anchor)
        assert_eq!(loaded[2].id(), msgs[3].id()); // t=40
        assert_eq!(loaded[3].id(), msgs[4].id()); // t=50
        assert!(has_older); // t=10 exists
        assert!(!has_newer);

        // Load around t=30 with half_limit=10 -> all 5 messages, no more
        let (loaded, has_older, has_newer) = ChatMessage::load_around(
            &mut txn,
            chat.id(),
            TimeStamp::from(30_000_000_000_i64),
            msgs[2].id(),
            10,
        )
        .await?;
        assert_eq!(loaded.len(), 5);
        assert!(!has_older);
        assert!(!has_newer);

        Ok(())
    }

    #[sqlx::test]
    async fn first_unread_message(pool: SqlitePool) -> anyhow::Result<()> {
        let mut notifier = StoreNotifier::noop();
        let mut txn = pool.begin().await?;

        let chat = test_chat();
        chat.store(txn.as_mut(), &mut notifier).await?;

        let msgs: Vec<_> = [10, 20, 30, 40, 50]
            .into_iter()
            .map(|t| message_at(chat.id(), t))
            .collect();
        for m in &msgs {
            m.store(txn.as_mut(), &mut notifier).await?;
        }

        // last_read at t=25 -> first unread is t=30
        let first = ChatMessage::first_unread_message(
            &mut txn,
            chat.id(),
            TimeStamp::from(25_000_000_000_i64),
        )
        .await?;
        assert_eq!(first.as_ref().map(|m| m.id()), Some(msgs[2].id()));

        // last_read at t=50 -> no unread
        let first = ChatMessage::first_unread_message(
            &mut txn,
            chat.id(),
            TimeStamp::from(50_000_000_000_i64),
        )
        .await?;
        assert!(first.is_none());

        Ok(())
    }
}
