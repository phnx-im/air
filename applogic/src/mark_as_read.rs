// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{cmp::Ordering, time::Duration};

use aircommon::identifiers::MimiId;
use aircoreclient::ChatId;
use aircoreclient::{MessageId, clients::CoreUser, store::Store};
use anyhow::Context;
use chrono::{DateTime, Utc};
use mimi_content::MessageStatus;
use tokio::{sync::watch, time::sleep};
use tracing::error;

use crate::api::user_settings_cubit::UserSettings;

#[cfg_attr(test, mockall::automock)]
pub(crate) trait MarkAsReadService {
    async fn mark_chat_as_read(
        &self,
        chat_id: ChatId,
        until: MessageId,
    ) -> anyhow::Result<(bool, Vec<(MessageId, MimiId)>)>;

    async fn enqueue_read_receipts(
        &self,
        chat_id: ChatId,
        statuses: Vec<(MessageId, MimiId)>,
    ) -> anyhow::Result<()>;

    async fn message_ordering(&self, a: MessageId, b: MessageId) -> anyhow::Result<Ordering>;
}

impl MarkAsReadService for CoreUser {
    async fn mark_chat_as_read(
        &self,
        chat_id: ChatId,
        until: MessageId,
    ) -> anyhow::Result<(bool, Vec<(MessageId, MimiId)>)> {
        <Self as Store>::mark_chat_as_read(self, chat_id, until).await
    }

    async fn enqueue_read_receipts(
        &self,
        chat_id: ChatId,
        statuses: Vec<(MessageId, MimiId)>,
    ) -> anyhow::Result<()> {
        let statuses = statuses
            .iter()
            .map(|(id, mimi_id)| (*id, mimi_id, MessageStatus::Read));
        self.outbound_service()
            .enqueue_receipts(chat_id, statuses)
            .await
    }

    async fn message_ordering(&self, a: MessageId, b: MessageId) -> anyhow::Result<Ordering> {
        let message_a = self.message(a).await?.context("no message")?;
        let message_b = self.message(b).await?.context("no message")?;
        // Tie break by message id
        Ok(message_a
            .timestamp()
            .cmp(&message_b.timestamp())
            .then(a.cmp(&b)))
    }
}

pub(crate) async fn mark_as_read(
    service: &impl MarkAsReadService,
    mark_as_read_tx: &watch::Sender<MarkAsReadState>,
    user_settings_rx: &watch::Receiver<UserSettings>,
    chat_id: ChatId,
    until_message_id: MessageId,
    until_timestamp: DateTime<Utc>,
    mark_as_read_debounce: Duration,
) -> anyhow::Result<()> {
    // Corner case: scheduled for a different message but with the same timestamp.
    let mut corner_case_id = None;
    let mut scheduled = mark_as_read_tx.send_if_modified(|state| match &state {
        MarkAsReadState::NotLoaded => {
            error!("Marking as read while chat is not loaded");
            false
        }
        MarkAsReadState::Marked { at }
        | MarkAsReadState::Scheduled {
            until_timestamp: at,
            until_message_id: _,
        } if *at < until_timestamp => {
            *state = MarkAsReadState::Scheduled {
                until_timestamp,
                until_message_id,
            };
            true
        }
        MarkAsReadState::Scheduled {
            until_timestamp: at,
            until_message_id: id,
        } if *id != until_message_id && *at == until_timestamp => {
            // Corner case: scheduled for a different message but with the same timestamp. Since
            // `until_timestamp` is a date time sent from Dart, it is truncated to microseconds,
            // and therefore we might have a different ordering in the database (nanoseconds
            // precision).
            corner_case_id = Some(*id);
            false
        }
        MarkAsReadState::Marked { .. } => {
            false // already marked as read
        }
        MarkAsReadState::Scheduled { .. } => {
            false // already scheduled at a later timestamp
        }
    });

    // Resolve equal timestamp corner case
    if let Some(scheduled_id) = corner_case_id
        && let Ordering::Less = service
            .message_ordering(scheduled_id, until_message_id)
            .await?
    {
        scheduled = true;
        mark_as_read_tx.send_modify(|state| {
            *state = MarkAsReadState::Scheduled {
                until_timestamp,
                until_message_id,
            }
        });
    }

    if !scheduled {
        return Ok(());
    }

    // debounce
    let mut rx = mark_as_read_tx.subscribe();
    tokio::select! {
        _ = rx.changed() => return Ok(()),
        _ = sleep(mark_as_read_debounce) => {},
    };

    // check if the scheduled state is still valid and if so, mark it as read
    let scheduled = mark_as_read_tx.send_if_modified(|state| match state {
        MarkAsReadState::Scheduled {
            until_message_id: scheduled_message_id,
            until_timestamp,
        } if *scheduled_message_id == until_message_id => {
            *state = MarkAsReadState::Marked {
                at: *until_timestamp,
            };
            true
        }
        _ => false,
    });
    if !scheduled {
        return Ok(());
    }

    let (_, read_message_ids) = service.mark_chat_as_read(chat_id, until_message_id).await?;

    let read_receipts_enabled = user_settings_rx.borrow().read_receipts;
    if read_receipts_enabled
        && let Err(error) = service
            .enqueue_read_receipts(chat_id, read_message_ids)
            .await
    {
        error!(%error, "Failed to enqueue read receipt");
    }

    Ok(())
}

#[derive(Debug, Default)]
pub(crate) enum MarkAsReadState {
    #[default]
    NotLoaded,
    /// Chat is marked as read until the given timestamp
    Marked { at: DateTime<Utc> },
    /// Chat is scheduled to be marked as read until the given timestamp and message id
    Scheduled {
        until_timestamp: DateTime<Utc>,
        until_message_id: MessageId,
    },
}

#[cfg(test)]
mod test {
    use uuid::Uuid;

    use super::*;

    #[tokio::test]
    async fn test_mark_as_read() {
        let mut service = MockMarkAsReadService::new();

        let (mark_as_read_tx, _) = watch::channel(MarkAsReadState::Marked {
            at: Utc::now() - Duration::from_secs(1),
        });
        let (user_settings_tx, user_settings_rx) = watch::channel(UserSettings {
            read_receipts: true,
            ..Default::default()
        });

        let chat_id = ChatId::new(Uuid::from_u128(1));
        let until_message_id = MessageId::new(Uuid::from_u128(2));
        let until_timestamp = Utc::now();
        let mark_as_read_debounce = Duration::ZERO;

        let mimi_id = MimiId::from_slice(&[0; 32]).unwrap();

        // Mark as read and enqueue receipts
        service
            .expect_mark_chat_as_read()
            .withf(move |cid, mid| *cid == chat_id && *mid == until_message_id)
            .returning(move |_, _| Ok((true, vec![(until_message_id, mimi_id)])))
            .times(1);

        service
            .expect_enqueue_read_receipts()
            .withf(move |cid, mids| *cid == chat_id && mids == &[(until_message_id, mimi_id)])
            .returning(|_, _| Ok(()))
            .times(1);

        mark_as_read(
            &service,
            &mark_as_read_tx,
            &user_settings_rx,
            chat_id,
            until_message_id,
            until_timestamp,
            mark_as_read_debounce,
        )
        .await
        .unwrap();

        service.checkpoint();

        // Mark as read and don't enqueue receipts because read receipts are disabled
        mark_as_read_tx.send_modify(|state| {
            *state = MarkAsReadState::Marked {
                at: until_timestamp - Duration::from_secs(1),
            };
        });
        user_settings_tx.send_modify(|settings| settings.read_receipts = false);

        service
            .expect_mark_chat_as_read()
            .withf(move |cid, mid| *cid == chat_id && *mid == until_message_id)
            .returning(move |_, _| Ok((true, vec![(until_message_id, mimi_id)])))
            .times(1);

        service.expect_enqueue_read_receipts().times(0);

        mark_as_read(
            &service,
            &mark_as_read_tx,
            &user_settings_rx,
            chat_id,
            until_message_id,
            until_timestamp,
            mark_as_read_debounce,
        )
        .await
        .unwrap();

        service.checkpoint();

        // Nothing to mark as read since the timestamp is older than the last read timestamp
        service.expect_mark_chat_as_read().times(0);
        service.expect_enqueue_read_receipts().times(0);

        mark_as_read(
            &service,
            &mark_as_read_tx,
            &user_settings_rx,
            chat_id,
            until_message_id,
            until_timestamp,
            mark_as_read_debounce,
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn mark_as_read_corner_case() {
        let mut service = MockMarkAsReadService::new();

        let scheduled_id = MessageId::new(Uuid::from_u128(1));
        let timestamp: DateTime<Utc> = "2023-01-01T00:00:00Z".parse().unwrap();

        let until_message_id = MessageId::new(Uuid::from_u128(2));

        let (mark_as_read_tx, _) = watch::channel(MarkAsReadState::Scheduled {
            until_timestamp: timestamp,
            until_message_id: scheduled_id,
        });
        let (_user_settings_tx, user_settings_rx) = watch::channel(UserSettings {
            read_receipts: false,
            ..Default::default()
        });

        let chat_id = ChatId::new(Uuid::from_u128(1));
        let mark_as_read_debounce = Duration::ZERO;

        let mimi_id = MimiId::from_slice(&[0; 32]).unwrap();

        // Corner case resolved as scheduled_id < until_message_id
        service
            .expect_message_ordering()
            .withf(move |a, b| *a == scheduled_id && *b == until_message_id)
            .returning(|_, _| Ok(Ordering::Less))
            .times(1);

        // Mark as read called with until_message_id
        service
            .expect_mark_chat_as_read()
            .withf(move |cid, mid| *cid == chat_id && *mid == until_message_id)
            .returning(move |_, _| Ok((true, vec![(until_message_id, mimi_id)])))
            .times(1);

        mark_as_read(
            &service,
            &mark_as_read_tx,
            &user_settings_rx,
            chat_id,
            until_message_id,
            timestamp,
            mark_as_read_debounce,
        )
        .await
        .unwrap();

        service.checkpoint();

        mark_as_read_tx.send_modify(|state| {
            *state = MarkAsReadState::Scheduled {
                until_timestamp: timestamp,
                until_message_id: scheduled_id,
            }
        });

        // Corner case resolved as scheduled_id > until_message_id
        service
            .expect_message_ordering()
            .withf(move |a, b| *a == scheduled_id && *b == until_message_id)
            .returning(|_, _| Ok(Ordering::Greater))
            .times(1);

        // Mark as read is not called
        service.expect_mark_chat_as_read().times(0);

        mark_as_read(
            &service,
            &mark_as_read_tx,
            &user_settings_rx,
            chat_id,
            until_message_id,
            timestamp,
            mark_as_read_debounce,
        )
        .await
        .unwrap();

        service.checkpoint();

        // Impossible case where scheduled_id == until_message_id as tie breaker
        service
            .expect_message_ordering()
            .withf(move |a, b| *a == scheduled_id && *b == until_message_id)
            .returning(|_, _| Ok(Ordering::Equal))
            .times(1);

        // Mark as read
        service.expect_message_ordering().times(0);

        mark_as_read(
            &service,
            &mark_as_read_tx,
            &user_settings_rx,
            chat_id,
            until_message_id,
            timestamp,
            mark_as_read_debounce,
        )
        .await
        .unwrap();

        service.checkpoint();
    }
}
