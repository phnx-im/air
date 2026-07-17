// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The rebuild set for a chat local notification.
//!
//! A chat notification is a pure function of local DB state.

use std::collections::HashSet;

use aircommon::identifiers::UserId;
use chrono::{DateTime, Utc};

use crate::{Chat, ChatId, ChatMessage, chats::reactions::Reaction, db::access::ReadConnection};

/// Number of entries a chat notification build set is capped at.
///
/// This matches `MessagingStyle` retention on Android.
pub(crate) const CHAT_NOTIFICATION_REBUILD_LIMIT: usize = 25;

/// A single reaction line in a chat notification rebuild set
#[derive(Debug, Clone, PartialEq)]
pub struct NotificationReaction {
    pub reactor: UserId,
    pub emoji: String,
    pub created_at: DateTime<Utc>,
    /// Target message, optional because it might have been deleted
    pub target: Option<ChatMessage>,
}

/// A single chronological entry in a chat notification rebuild set
#[derive(Debug, Clone, PartialEq)]
pub enum ChatNotificationEntry {
    Message(Box<ChatMessage>),
    Reaction(NotificationReaction),
}

impl ChatNotificationEntry {
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            ChatNotificationEntry::Message(message) => message.timestamp(),
            ChatNotificationEntry::Reaction(reaction) => reaction.created_at,
        }
    }
}

/// The rebuild set for one chat local notification
///
/// The entries are chronologically ordered (ascending), capped at
/// [`CHAT_NOTIFICATION_REBUILD_LIMIT`] newest entries across both messages and reactions.
#[derive(Debug, Default)]
pub struct ChatNotificationRebuildSet {
    pub entries: Vec<ChatNotificationEntry>,
}

impl ChatNotificationRebuildSet {
    pub fn participant_ids(&self) -> HashSet<UserId> {
        self.entries
            .iter()
            .filter_map(|entry| match entry {
                ChatNotificationEntry::Message(message) => message.message().sender().cloned(),
                ChatNotificationEntry::Reaction(reaction) => Some(reaction.reactor.clone()),
            })
            .collect()
    }
}

impl Chat {
    /// Loads the notification rebuild set for `chat_id`.
    ///
    /// Includes content messages newer than `max(last_read, notified_until)` (deleted messages are
    /// dropped) and reactions on messages from `own_user` newer than `notified_until`.
    ///
    /// Returns an empty set if the chat does not exist.
    pub(crate) async fn load_notification_rebuild_set(
        mut connection: impl ReadConnection,
        chat_id: ChatId,
        own_user: &UserId,
    ) -> sqlx::Result<ChatNotificationRebuildSet> {
        let Some((last_read, notified_until)) =
            Chat::load_watermark(&mut connection, chat_id).await?
        else {
            return Ok(Default::default());
        };

        // Content messages are gated by both watermarks: unread (relative to `last_read`) and not
        // yet notified (relative to `notified_until`).
        let messages_since = notified_until
            .map(|notified_until| notified_until.max(last_read))
            .unwrap_or(last_read);
        // Reactions are never marked as read, they are gated only by `notified_until`.
        let reactions_since = notified_until;

        let limit = CHAT_NOTIFICATION_REBUILD_LIMIT as u32;
        let messages =
            ChatMessage::load_newest_since(&mut connection, chat_id, messages_since, limit).await?;
        let reactions = Reaction::load_own_message_reactions_since(
            &mut connection,
            chat_id,
            own_user,
            reactions_since,
            limit,
        )
        .await?;

        let mut entries = Vec::with_capacity(messages.len() + reactions.len());
        entries.extend(
            messages
                .into_iter()
                // Technically it would be better to exclude these messages already in SQL, but
                // currently we cannot do this, because being deleted is a function of content.
                .filter(|message| !message.message().is_deleted())
                .map(Box::new)
                .map(ChatNotificationEntry::Message),
        );

        for reaction in reactions {
            let target =
                ChatMessage::load_by_mimi_id(&mut connection, &reaction.target_mimi_id).await?;
            entries.push(ChatNotificationEntry::Reaction(NotificationReaction {
                reactor: reaction.sender,
                emoji: reaction.emoji,
                created_at: reaction.created_at.into(),
                target: target.filter(|message| !message.message().is_deleted()),
            }))
        }

        entries.sort_unstable_by_key(|entry| entry.timestamp());
        let overflow = entries
            .len()
            .saturating_sub(CHAT_NOTIFICATION_REBUILD_LIMIT);
        entries.drain(..overflow);

        Ok(ChatNotificationRebuildSet { entries })
    }
}

#[cfg(test)]
mod tests {
    use aircommon::{identifiers::MimiId, time::TimeStamp};
    use mimi_content::{Disposition, MimiContent, NestedPart};
    use openmls::group::GroupId;
    use sqlx::SqlitePool;

    use crate::{
        ContentMessage, MessageId, chats::persistence::tests::test_chat, db::access::DbAccess,
    };

    use super::*;

    fn dt(secs: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(secs, 0).unwrap()
    }

    fn message_at(chat_id: ChatId, sender: UserId, group_id: &GroupId, secs: i64) -> ChatMessage {
        let mut salt = [0u8; 16];
        salt[..8].copy_from_slice(&secs.to_le_bytes());
        ChatMessage::new_for_test(
            chat_id,
            MessageId::random(),
            TimeStamp::from(secs * 1_000_000_000),
            ContentMessage::new(
                sender,
                true,
                MimiContent::simple_markdown_message(format!("msg at {secs}"), salt),
                group_id,
            ),
        )
    }

    fn deleted_message_at(
        chat_id: ChatId,
        sender: UserId,
        group_id: &GroupId,
        secs: i64,
    ) -> ChatMessage {
        let content = MimiContent {
            salt: vec![0; 16],
            nested_part: NestedPart::NullPart {
                disposition: Disposition::Render,
                language: String::new(),
            },
            ..Default::default()
        };
        ChatMessage::new_for_test(
            chat_id,
            MessageId::random(),
            TimeStamp::from(secs * 1_000_000_000),
            ContentMessage::new(sender, true, content, group_id),
        )
    }

    fn reaction_at(
        reaction_id: u8,
        target_mimi_id: MimiId,
        chat_id: ChatId,
        reactor: UserId,
        secs: i64,
    ) -> Reaction {
        // The emoji varies per reaction id: `(target, sender, emoji)` is uniquely constrained, so
        // distinct rows on the same target need distinct emoji.
        Reaction::new(
            MimiId::from_slice(&[reaction_id; 32]).unwrap(),
            target_mimi_id,
            chat_id,
            reactor,
            format!("emoji-{reaction_id}"),
            TimeStamp::from(secs * 1_000_000_000),
        )
    }

    #[sqlx::test]
    async fn watermark_filters_by_last_read_and_notified_until(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let mut connection = pool.write().await?;

        let sender = UserId::random("localhost".parse().unwrap());
        let own_user = UserId::random("localhost".parse().unwrap());

        let mut chat = test_chat();
        chat.last_read = dt(10);
        chat.store(&mut connection).await?;
        Chat::set_notified_until(&mut connection, chat.id(), dt(20)).await?;

        // Older than last_read: excluded.
        message_at(chat.id(), sender.clone(), chat.group_id(), 5)
            .store(&mut connection)
            .await?;
        // Newer than last_read, but not newer than notified_until: excluded.
        message_at(chat.id(), sender.clone(), chat.group_id(), 15)
            .store(&mut connection)
            .await?;
        // Newer than both watermarks: included.
        let newest = message_at(chat.id(), sender.clone(), chat.group_id(), 30);
        newest.store(&mut connection).await?;

        let rebuild_set =
            Chat::load_notification_rebuild_set(&mut connection, chat.id(), &own_user).await?;

        assert_eq!(rebuild_set.entries.len(), 1);
        let ChatNotificationEntry::Message(message) = &rebuild_set.entries[0] else {
            panic!("expected a message entry");
        };
        assert_eq!(message.id(), newest.id());

        Ok(())
    }

    #[sqlx::test]
    async fn deleted_messages_are_dropped(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let mut connection = pool.write().await?;

        let sender = UserId::random("localhost".parse().unwrap());
        let own_user = UserId::random("localhost".parse().unwrap());

        let mut chat = test_chat();
        chat.last_read = dt(0);
        chat.store(&mut connection).await?;

        let kept = message_at(chat.id(), sender.clone(), chat.group_id(), 10);
        kept.store(&mut connection).await?;
        deleted_message_at(chat.id(), sender.clone(), chat.group_id(), 20)
            .store(&mut connection)
            .await?;

        let rebuild_set =
            Chat::load_notification_rebuild_set(&mut connection, chat.id(), &own_user).await?;

        assert_eq!(rebuild_set.entries.len(), 1);
        let ChatNotificationEntry::Message(message) = &rebuild_set.entries[0] else {
            panic!("expected a message entry");
        };
        assert_eq!(message.id(), kept.id());

        Ok(())
    }

    #[sqlx::test]
    async fn caps_at_limit_merged_chronologically(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let mut connection = pool.write().await?;

        let sender = UserId::random("localhost".parse().unwrap());
        let own_user = UserId::random("localhost".parse().unwrap());

        let mut chat = test_chat();
        chat.last_read = dt(-1000);
        chat.store(&mut connection).await?;

        // 20 messages at t=0,2,..,38 and 10 reactions at t=1,3,..,19, all on the own user's own
        // message so they qualify for notifying: more than CHAT_NOTIFICATION_REBUILD_LIMIT (25)
        // candidate entries in total, so the oldest ones must be dropped after the merge.
        let own_message = message_at(chat.id(), own_user.clone(), chat.group_id(), -1);
        own_message.store(&mut connection).await?;
        let target_mimi_id = *own_message.message().mimi_id().unwrap();

        for i in 0..20 {
            message_at(chat.id(), sender.clone(), chat.group_id(), i * 2)
                .store(&mut connection)
                .await?;
        }
        for i in 0..10 {
            reaction_at(
                i as u8,
                target_mimi_id,
                chat.id(),
                sender.clone(),
                i * 2 + 1,
            )
            .store(&mut connection)
            .await?;
        }

        let rebuild_set =
            Chat::load_notification_rebuild_set(&mut connection, chat.id(), &own_user).await?;

        assert_eq!(rebuild_set.entries.len(), CHAT_NOTIFICATION_REBUILD_LIMIT);
        // Ascending order.
        for pair in rebuild_set.entries.windows(2) {
            assert!(pair[0].timestamp() <= pair[1].timestamp());
        }
        // The 5 oldest candidates (t=0..4, i.e. the first 5 by our construction) were dropped; the
        // newest entry is at t=38.
        assert_eq!(rebuild_set.entries.last().unwrap().timestamp(), dt(38));
        assert!(
            rebuild_set
                .entries
                .iter()
                .all(|entry| entry.timestamp() >= dt(5))
        );

        Ok(())
    }

    #[sqlx::test]
    async fn reaction_on_own_message_included_others_excluded(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let mut connection = pool.write().await?;

        let own_user = UserId::random("localhost".parse().unwrap());
        let other_user = UserId::random("localhost".parse().unwrap());
        let reactor = UserId::random("localhost".parse().unwrap());

        let mut chat = test_chat();
        chat.last_read = dt(0);
        chat.store(&mut connection).await?;

        let own_message = message_at(chat.id(), own_user.clone(), chat.group_id(), 10);
        own_message.store(&mut connection).await?;
        let other_message = message_at(chat.id(), other_user.clone(), chat.group_id(), 11);
        other_message.store(&mut connection).await?;

        let own_target = *own_message.message().mimi_id().unwrap();
        let other_target = *other_message.message().mimi_id().unwrap();

        reaction_at(1, own_target, chat.id(), reactor.clone(), 20)
            .store(&mut connection)
            .await?;
        reaction_at(2, other_target, chat.id(), reactor.clone(), 21)
            .store(&mut connection)
            .await?;

        let rebuild_set =
            Chat::load_notification_rebuild_set(&mut connection, chat.id(), &own_user).await?;

        let reactions: Vec<_> = rebuild_set
            .entries
            .iter()
            .filter_map(|entry| match entry {
                ChatNotificationEntry::Reaction(reaction) => Some(reaction),
                ChatNotificationEntry::Message(_) => None,
            })
            .collect();
        assert_eq!(reactions.len(), 1);
        assert_eq!(reactions[0].created_at, dt(20));

        Ok(())
    }

    #[sqlx::test]
    async fn retracted_reaction_disappears(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let mut connection = pool.write().await?;

        let own_user = UserId::random("localhost".parse().unwrap());
        let reactor = UserId::random("localhost".parse().unwrap());

        let chat = test_chat();
        chat.store(&mut connection).await?;

        let own_message = message_at(chat.id(), own_user.clone(), chat.group_id(), 10);
        own_message.store(&mut connection).await?;
        let target_mimi_id = *own_message.message().mimi_id().unwrap();

        let reaction_mimi_id = MimiId::from_slice(&[7; 32]).unwrap();
        Reaction::new(
            reaction_mimi_id,
            target_mimi_id,
            chat.id(),
            reactor.clone(),
            "👍".to_string(),
            TimeStamp::from(20 * 1_000_000_000),
        )
        .store(&mut connection)
        .await?;

        let rebuild_set =
            Chat::load_notification_rebuild_set(&mut connection, chat.id(), &own_user).await?;
        assert_eq!(rebuild_set.entries.len(), 1);

        Reaction::delete_by_mimi_id(&mut connection, &reaction_mimi_id).await?;

        let rebuild_set =
            Chat::load_notification_rebuild_set(&mut connection, chat.id(), &own_user).await?;
        assert!(rebuild_set.entries.is_empty());

        Ok(())
    }
}
