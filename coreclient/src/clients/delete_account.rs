// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::bail;
use tracing::{info, warn};

use crate::{
    clients::{
        CoreUser,
        own_client_info::{OwnClientInfo, UnlinkReason},
    },
    outbound_service::account_deletion::{AccountDeletion, AccountDeletionProgress},
};

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
    /// The deletion is persisted before anything is sent and continues in the
    /// outbound service until it completes. Once it completes, the unlink
    /// reason is [`UnlinkReason::AccountDeleted`], and this device must reset
    /// itself.
    ///
    /// [`AccountDeletionStatus::Completed`] also covers a deletion that is
    /// given up after the self group or QS client deletion kept failing. This
    /// device must reset itself then as well.
    ///
    /// Fails if the deletion can't be started, if this device was unlinked, or
    /// if the self group deletion fails before anything reached the DS or the
    /// DS refuses the first delete sent. The deletion is dropped then and
    /// nothing changed.
    pub async fn delete_account(&self) -> anyhow::Result<AccountDeletionStatus> {
        const MAX_ATTEMPTS: usize = 3;

        match OwnClientInfo::account_unlink_reason(self.db().read().await?).await? {
            Some(UnlinkReason::AccountDeleted) => return Ok(AccountDeletionStatus::Completed),
            Some(UnlinkReason::Unlinked) => bail!("this device was unlinked"),
            None => {}
        }
        AccountDeletion::start(self.db().write().await?).await?;

        let mut attempt = 1;
        loop {
            match self.outbound_service().run_account_deletion().await {
                Ok(AccountDeletionProgress::Completed | AccountDeletionProgress::GaveUp) => {
                    return Ok(AccountDeletionStatus::Completed);
                }
                Ok(AccountDeletionProgress::Aborted(error)) => {
                    return Err(error.context("failed to delete the account"));
                }
                // A sibling commit won the epoch. Catch up on it and try again.
                Ok(AccountDeletionProgress::Blocked) if attempt < MAX_ATTEMPTS => {
                    warn!(attempt, "Self group deletion lost the epoch, retrying");
                    match self.drain_and_process_qs_queue().await {
                        Ok(processed) => {
                            if let Some(error) = processed.errors.first() {
                                warn!(%error, "Failed to process queued messages before retrying");
                            }
                        }
                        Err(error) => {
                            warn!(%error, "Failed to fetch queued messages before retrying");
                            break;
                        }
                    }
                    attempt += 1;
                }
                // Another run of the deletion finished or dropped it first.
                Ok(AccountDeletionProgress::Idle) => return self.account_deletion_outcome().await,
                Ok(AccountDeletionProgress::Blocked) => break,
                Err(error) => {
                    warn!(%error, "Account deletion failed");
                    break;
                }
            }
        }
        info!("Account deletion continues in the background");
        self.outbound_service().notify_account_deletion();
        Ok(AccountDeletionStatus::Pending)
    }

    /// The outcome of a started deletion that this call didn't finish itself.
    async fn account_deletion_outcome(&self) -> anyhow::Result<AccountDeletionStatus> {
        match OwnClientInfo::account_unlink_reason(self.db().read().await?).await? {
            Some(UnlinkReason::AccountDeleted) => Ok(AccountDeletionStatus::Completed),
            Some(UnlinkReason::Unlinked) => bail!("this device was unlinked"),
            None if AccountDeletion::is_pending(self.db().read().await?).await? => {
                Ok(AccountDeletionStatus::Pending)
            }
            None => bail!("the account deletion was dropped"),
        }
    }
}
