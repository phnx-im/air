// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Keeps the app badge in sync with the number of unread messages.

use std::{pin::pin, sync::Arc, time::Duration};

use aircommon::identifiers::UserId;
use chrono::{DateTime, Utc};
use tokio_stream::{Stream, StreamExt};
use tracing::error;

use crate::{
    Chat,
    clients::CoreUser,
    db::{
        access::DbAccess,
        notification::{DbEntityId, DbNotification},
    },
};

const DEBOUNCE: Duration = Duration::from_millis(250);

/// We also count after this much time without a count. This picks up chat
/// mutes that ran out, and changes from writes that skipped their DB
/// notification.
const RECOUNT_INTERVAL: Duration = Duration::from_secs(30);

impl CoreUser {
    /// Reports the app badge count to `emit` now and again whenever it
    /// changes.
    pub async fn watch_badge_count<E>(
        &self,
        emit: impl FnMut(usize) -> Result<(), E>,
    ) -> Result<(), E> {
        watch_badge_count(self.db(), self.user_id(), RECOUNT_INTERVAL, emit).await
    }
}

struct BadgeCount {
    count: usize,
    next_mute_expiry: Option<DateTime<Utc>>,
}

impl BadgeCount {
    /// Whether a recount to `count` at `now` reveals a write that skipped its
    /// DB notification, given that no notification came since this count.
    ///
    /// A chat mute running out changes the count without a notification, so
    /// such a change is expected.
    fn missed_notification(&self, count: usize, now: DateTime<Utc>) -> bool {
        let mute_ran_out = self.next_mute_expiry.is_some_and(|expiry| expiry <= now);
        count != self.count && !mute_ran_out
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Trigger {
    Change,
    Interval,
}

async fn watch_badge_count<E>(
    db: &DbAccess,
    own_user: &UserId,
    recount_interval: Duration,
    mut emit: impl FnMut(usize) -> Result<(), E>,
) -> Result<(), E> {
    let mut notifications = pin!(db.notifier_tx.subscribe());
    let mut last: Option<BadgeCount> = None;
    let mut trigger = Trigger::Change;
    loop {
        last = match load(db, own_user).await {
            Ok(current) => {
                if trigger == Trigger::Interval
                    && let Some(last) = &last
                    && last.missed_notification(current.count, Utc::now())
                {
                    error!(
                        previous = last.count,
                        current = current.count,
                        "Badge count changed without a DB notification"
                    );
                }
                if last.as_ref().is_none_or(|last| last.count != current.count) {
                    emit(current.count)?;
                }
                Some(current)
            }
            Err(error) => {
                error!(%error, "Failed to load the badge count");
                // The change that triggered this count is consumed. We forget
                // the last count, so that the next one is emitted and not
                // reported as a missed notification.
                None
            }
        };

        let Some(next) = next_trigger(&mut notifications, recount_interval).await else {
            return Ok(());
        };
        trigger = next;
    }
}

async fn load(db: &DbAccess, own_user: &UserId) -> sqlx::Result<BadgeCount> {
    // Taken before counting, so that a mute running out during the count is
    // still returned as the next expiry.
    let now = Utc::now();
    db.with_read_transaction(async |txn| {
        let count = Chat::global_unread_message_count(&mut *txn, own_user).await?;
        let next_mute_expiry = Chat::next_mute_expiry(&mut *txn, now).await?;
        Ok(BadgeCount {
            count,
            next_mute_expiry,
        })
    })
    .await
}

/// Waits until the count is due again. Returns `None` once the notifications
/// closed.
async fn next_trigger(
    notifications: &mut (impl Stream<Item = Arc<DbNotification>> + Unpin),
    recount_interval: Duration,
) -> Option<Trigger> {
    tokio::select! {
        // A pending notification wins over the interval, so that its change is
        // not reported as a missed notification.
        biased;
        open = next_relevant_change(notifications) => {
            (open && drain_for(notifications).await).then_some(Trigger::Change)
        }
        () = tokio::time::sleep(recount_interval) => Some(Trigger::Interval),
    }
}

/// Waits for a notification that can change the count.
async fn next_relevant_change(
    notifications: &mut (impl Stream<Item = Arc<DbNotification>> + Unpin),
) -> bool {
    while let Some(notification) = notifications.next().await {
        if notification.ops.keys().any(affects_count) {
            return true;
        }
    }
    false
}

fn affects_count(id: &DbEntityId) -> bool {
    match id {
        DbEntityId::Chat(_) | DbEntityId::Message(_) => true,
        DbEntityId::User(_) | DbEntityId::Attachment(_) | DbEntityId::UserSetting(_) => false,
    }
}

/// Consumes notifications for the debounce window, so that they are folded
/// into the next count and do not pile up in the broadcast channel.
async fn drain_for(notifications: &mut (impl Stream<Item = Arc<DbNotification>> + Unpin)) -> bool {
    let mut deadline = pin!(tokio::time::sleep(DEBOUNCE));
    loop {
        tokio::select! {
            () = &mut deadline => return true,
            notification = notifications.next() => {
                if notification.is_none() {
                    return false;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration as ChronoDuration;
    use sqlx::SqlitePool;
    use tokio::{
        sync::mpsc::{self, UnboundedReceiver},
        task::JoinHandle,
        time::timeout,
    };

    use crate::{
        ChatId, ChatMessage, ChatMuted, MessageId,
        chats::{
            messages::persistence::tests::{test_chat_message, test_chat_message_from},
            persistence::tests::test_chat,
        },
        db::access::WriteConnection,
    };

    use super::*;

    /// Long enough for one debounce window plus a count.
    const SETTLE: Duration = Duration::from_millis(1000);
    const SHORT_INTERVAL: Duration = Duration::from_millis(100);

    struct Watcher {
        counts: UnboundedReceiver<usize>,
        task: JoinHandle<()>,
    }

    impl Watcher {
        fn spawn(db: &DbAccess, own_user: &UserId, recount_interval: Duration) -> Self {
            let (tx, counts) = mpsc::unbounded_channel();
            let db = db.clone();
            let own_user = own_user.clone();
            let task = tokio::spawn(async move {
                let emit = |count| tx.send(count);
                let _closed = watch_badge_count(&db, &own_user, recount_interval, emit).await;
            });
            Self { counts, task }
        }

        async fn next(&mut self) -> usize {
            timeout(SETTLE, self.counts.recv())
                .await
                .expect("no badge count within the timeout")
                .expect("watcher stopped")
        }

        async fn assert_silent(&mut self) {
            let next = timeout(SETTLE, self.counts.recv()).await;
            assert!(next.is_err(), "unexpected badge count {next:?}");
        }
    }

    impl Drop for Watcher {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    fn own_user() -> UserId {
        UserId::random("localhost".parse().unwrap())
    }

    async fn store_chat(db: &DbAccess) -> anyhow::Result<ChatId> {
        let chat = test_chat();
        db.with_write_transaction(async |txn| chat.store(txn).await)
            .await?;
        Ok(chat.id())
    }

    async fn store_message(db: &DbAccess, chat_id: ChatId) -> anyhow::Result<MessageId> {
        let message = test_chat_message(chat_id);
        db.with_write_transaction(async |txn| message.store(txn).await)
            .await?;
        Ok(message.id())
    }

    #[sqlx::test]
    async fn follows_new_and_read_messages(pool: SqlitePool) -> anyhow::Result<()> {
        let db = DbAccess::for_tests(pool);
        let own_user = own_user();
        let chat_id = store_chat(&db).await?;
        store_message(&db, chat_id).await?;

        let mut watcher = Watcher::spawn(&db, &own_user, RECOUNT_INTERVAL);
        assert_eq!(watcher.next().await, 1);

        store_message(&db, chat_id).await?;
        assert_eq!(watcher.next().await, 2);

        db.with_write_transaction(async |txn| {
            Chat::mark_as_read(txn, [(chat_id, Utc::now())]).await
        })
        .await?;
        assert_eq!(watcher.next().await, 0);
        Ok(())
    }

    #[sqlx::test]
    async fn recounts_after_a_message_only_change(pool: SqlitePool) -> anyhow::Result<()> {
        let db = DbAccess::for_tests(pool);
        let own_user = own_user();
        let chat_id = store_chat(&db).await?;
        let message_id = store_message(&db, chat_id).await?;

        let mut watcher = Watcher::spawn(&db, &own_user, RECOUNT_INTERVAL);
        assert_eq!(watcher.next().await, 1);

        db.with_write_transaction(async |txn| {
            ChatMessage::mark_deleted(&mut *txn, message_id).await?;
            txn.notifier().update(message_id);
            Ok::<_, sqlx::Error>(())
        })
        .await?;
        assert_eq!(watcher.next().await, 0);
        Ok(())
    }

    #[sqlx::test]
    async fn folds_a_burst_into_one_count(pool: SqlitePool) -> anyhow::Result<()> {
        let db = DbAccess::for_tests(pool);
        let own_user = own_user();
        let chat_id = store_chat(&db).await?;

        let mut watcher = Watcher::spawn(&db, &own_user, RECOUNT_INTERVAL);
        assert_eq!(watcher.next().await, 0);

        for _ in 0..5 {
            store_message(&db, chat_id).await?;
        }
        assert_eq!(watcher.next().await, 5);
        watcher.assert_silent().await;
        Ok(())
    }

    #[sqlx::test]
    async fn skips_changes_that_keep_the_count(pool: SqlitePool) -> anyhow::Result<()> {
        let db = DbAccess::for_tests(pool);
        let own_user = own_user();
        let chat_id = store_chat(&db).await?;

        let mut watcher = Watcher::spawn(&db, &own_user, RECOUNT_INTERVAL);
        assert_eq!(watcher.next().await, 0);

        let own_message =
            test_chat_message_from(chat_id, [0; 16], Utc::now().into(), own_user.clone());
        db.with_write_transaction(async |txn| own_message.store(txn).await)
            .await?;
        watcher.assert_silent().await;
        Ok(())
    }

    #[sqlx::test]
    async fn counts_a_chat_once_its_mute_runs_out(pool: SqlitePool) -> anyhow::Result<()> {
        let db = DbAccess::for_tests(pool);
        let own_user = own_user();
        let chat_id = store_chat(&db).await?;
        store_message(&db, chat_id).await?;
        let muted_until = Utc::now() + ChronoDuration::milliseconds(600);
        db.with_write_transaction(async |txn| {
            Chat::set_muted_until(txn, chat_id, Some(ChatMuted::Until(muted_until))).await
        })
        .await?;

        let mut watcher = Watcher::spawn(&db, &own_user, SHORT_INTERVAL);
        assert_eq!(watcher.next().await, 0);
        assert_eq!(watcher.next().await, 1);
        Ok(())
    }

    #[sqlx::test]
    async fn recounts_a_change_without_a_notification(pool: SqlitePool) -> anyhow::Result<()> {
        let db = DbAccess::for_tests(pool);
        let own_user = own_user();
        let chat_id = store_chat(&db).await?;
        store_message(&db, chat_id).await?;

        let mut watcher = Watcher::spawn(&db, &own_user, SHORT_INTERVAL);
        assert_eq!(watcher.next().await, 1);

        db.with_write_transaction(async |txn| {
            Chat::mark_as_read(txn, [(chat_id, Utc::now())]).await?;
            txn.notifier().clear();
            Ok::<_, sqlx::Error>(())
        })
        .await?;
        assert_eq!(watcher.next().await, 0);
        Ok(())
    }

    #[test]
    fn only_a_mute_running_out_explains_a_change() {
        let now = Utc::now();
        let last = |next_mute_expiry| BadgeCount {
            count: 1,
            next_mute_expiry,
        };
        let later = now + ChronoDuration::seconds(1);

        assert!(!last(None).missed_notification(1, now));
        assert!(last(None).missed_notification(2, now));
        assert!(last(Some(later)).missed_notification(2, now));
        assert!(!last(Some(now)).missed_notification(2, now));
    }

    #[sqlx::test]
    async fn stops_when_emit_fails(pool: SqlitePool) -> anyhow::Result<()> {
        let db = DbAccess::for_tests(pool);
        let own_user = own_user();

        let result = timeout(
            SETTLE,
            watch_badge_count(&db, &own_user, RECOUNT_INTERVAL, |_| Err("closed")),
        )
        .await?;
        assert_eq!(result, Err("closed"));
        Ok(())
    }
}
