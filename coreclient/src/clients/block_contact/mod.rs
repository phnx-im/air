// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::UserId;
use airprotos::client::self_group::BlockedContactEntry;
use chrono::{DateTime, Utc};

use crate::{clients::CoreUser, user_profiles::display_name::DisplayName};

use self::pending::BlockedState;

pub(crate) mod pending;
pub(crate) mod persistence;

impl CoreUser {
    /// Blocks a contact.
    pub async fn block_contact(&self, user_id: UserId) -> anyhow::Result<()> {
        let profile = self.user_profile(&user_id).await;
        self.record_blocked_state(BlockedState::Blocked(BlockedContact {
            user_id,
            last_display_name: profile.display_name.clone(),
            blocked_at: Utc::now(),
        }))
        .await
    }

    /// Unblocks a contact.
    pub async fn unblock_contact(&self, user_id: UserId) -> anyhow::Result<()> {
        self.record_blocked_state(BlockedState::Unblocked { user_id })
            .await
    }

    /// Applies a blocked-state change locally right away (optimistic) and
    /// parks to send to other devices via the self-group.
    async fn record_blocked_state(&self, intended: BlockedState) -> anyhow::Result<()> {
        let entry = BlockedContactEntry::from(&intended);
        self.db()
            .with_write_transaction(async |txn| {
                intended.apply(&mut *txn).await?;
                persistence::store_outgoing_entry(txn, &entry).await
            })
            .await?;

        self.outbound_service().notify_pending_chat_operations();

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BlockedContact {
    pub(crate) user_id: UserId,
    pub(crate) last_display_name: DisplayName,
    pub(crate) blocked_at: DateTime<Utc>,
}

#[cfg(test)]
impl BlockedContact {
    pub(crate) fn new(user_id: UserId) -> Self {
        Self {
            last_display_name: DisplayName::from_user_id(&user_id),
            user_id,
            blocked_at: Utc::now(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Blocked contact")]
pub struct BlockedContactError;
