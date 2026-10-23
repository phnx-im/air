// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::Duration;

use anyhow::{anyhow, bail};
use tokio::time::timeout;
use tracing::info;

use crate::{
    clients::{
        CoreUser,
        own_client_info::{OwnClientInfo, UnlinkReason},
    },
    outbound_service::{WaitForDoneFuture, account_deletion::AccountDeletion},
};

/// How long [`CoreUser::delete_account`] waits for the outbound service before
/// it reports the deletion as pending.
const ACCOUNT_DELETION_WAIT: Duration = Duration::from_secs(30);

/// Whether the account deletion finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountDeletionStatus {
    /// The account is deleted.
    Completed,
    /// The deletion is started and continues in the background, also after a
    /// restart.
    Pending,
}

impl CoreUser {
    /// Deletes the account on the server.
    ///
    /// 1. Delete the self group, so that linked devices reset themselves
    /// 2. Delete the QS client
    /// 3. Delete usernames
    /// 4. Self-remove from all groups except the self group
    /// 5. Delete the QS user
    /// 6. Delete the AS user
    ///
    /// The deletion is persisted before anything is sent and runs in the
    /// outbound service until it completes. This waits a bounded time for the
    /// outbound service, and returns [`AccountDeletionStatus::Pending`] if the
    /// deletion did not complete by then, or if the outbound service backs off
    /// or is stopped while the self group is not done. Calling this again runs
    /// a pending deletion right away.
    ///
    /// Once the deletion completes, the unlink reason is
    /// [`UnlinkReason::AccountDeleted`], and this device must reset itself.
    /// [`AccountDeletionStatus::Completed`] also covers a deletion that is
    /// given up after the self group or QS client deletion kept failing. This
    /// device must reset itself then as well.
    ///
    /// Fails if the deletion can't be started, if this device was unlinked, or
    /// if the self group deletion failed before anything reached the DS or the
    /// DS refused the first delete sent. The deletion is aborted then and
    /// nothing changed.
    pub async fn delete_account(&self) -> anyhow::Result<AccountDeletionStatus> {
        let unlink_reason = self
            .db()
            .with_write_transaction(async |txn| AccountDeletion::start(txn).await)
            .await?;
        match unlink_reason {
            Some(UnlinkReason::AccountDeleted) => return Ok(AccountDeletionStatus::Completed),
            Some(UnlinkReason::Unlinked) => bail!("this device was unlinked"),
            None => {}
        }

        let wait = async {
            self.continue_account_deletion().await?.await;
            // A run that was going on when notified may have missed the
            // deletion, the run after it doesn't.
            if self.account_deletion_status().await? == AccountDeletionStatus::Pending {
                self.continue_account_deletion().await?.await;
            }
            self.account_deletion_status().await
        };
        match timeout(ACCOUNT_DELETION_WAIT, wait).await {
            Ok(status) => status,
            Err(_elapsed) => {
                info!("Account deletion continues in the background");
                self.account_deletion_status().await
            }
        }
    }

    /// Wakes the outbound service to continue a started account deletion.
    ///
    /// Once the self group is done, this also starts a stopped outbound
    /// service. The QS client may be gone then, so processing the QS stream,
    /// which starts it otherwise, fails.
    pub(crate) async fn continue_account_deletion(&self) -> anyhow::Result<WaitForDoneFuture> {
        let past_self_group = match AccountDeletion::load(self.db().read().await?).await? {
            Some(AccountDeletion::Running(step)) => step.is_past_self_group(),
            Some(AccountDeletion::Aborted { .. }) | None => false,
        };
        Ok(if past_self_group {
            self.outbound_service().start()
        } else {
            self.outbound_service().notify_account_deletion()
        })
    }

    async fn account_deletion_status(&self) -> anyhow::Result<AccountDeletionStatus> {
        self.db()
            .with_read_transaction(async |txn| {
                match OwnClientInfo::account_unlink_reason(&mut *txn).await? {
                    Some(UnlinkReason::AccountDeleted) => {
                        return Ok(AccountDeletionStatus::Completed);
                    }
                    Some(UnlinkReason::Unlinked) => bail!("this device was unlinked"),
                    None => {}
                }
                match AccountDeletion::load(&mut *txn).await? {
                    Some(AccountDeletion::Running(_)) => Ok(AccountDeletionStatus::Pending),
                    Some(AccountDeletion::Aborted { error }) => {
                        Err(anyhow!(error).context("failed to delete the account"))
                    }
                    None => bail!("the account deletion was dropped"),
                }
            })
            .await
    }
}
