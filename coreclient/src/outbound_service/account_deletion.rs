// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airapiclient::{ApiClient, ClassifyRequestError, RequestErrorKind, ds_api::DsRequestError};
use aircommon::time::TimeStamp;
use mimi_room_policy::RoleIndex;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

use crate::{
    Chat, UsernameRecord,
    clients::own_client_info::{OwnClientInfo, UnlinkReason},
    db::access::{ReadConnection, WriteConnection, WriteDbTransaction},
    groups::{Group, handle_group_not_found_on_ds, self_group::SelfGroup},
    job::{
        JobError,
        operation::{Operation, OperationData, OperationId, OperationKind},
        pending_chat_operation::PendingChatOperation,
    },
    privacy_pass,
};

use super::{OutboundServiceContext, error::OutboundServiceError};

const OPERATION_ID: &[u8] = b"account_deletion";

/// Failed attempts of the self group or QS client deletion before this device
/// gives up on the deletion and resets itself. Network errors and rate limiting
/// don't count.
const MAX_FAILED_ATTEMPTS: u32 = 10;

/// The persisted deletion of the account, started on this device.
///
/// It stores the next step to run, so that a run picks up where the last one
/// stopped, also after a restart.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AccountDeletion {
    next_step: AccountDeletionStep,
    /// Failed attempts of the next step.
    failed_attempts: u32,
    /// Whether a self group delete was sent. The DS may have accepted it
    /// without a response arriving.
    self_group_delete_sent: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum AccountDeletionStep {
    /// Deletes the self group, which resets the linked devices.
    DeleteSelfGroup,
    DeleteQsClient,
    DeleteUsernames,
    LeaveChats,
    DeleteQsUser,
    DeleteAsUser,
}

impl AccountDeletionStep {
    fn next(self) -> Option<Self> {
        match self {
            Self::DeleteSelfGroup => Some(Self::DeleteQsClient),
            Self::DeleteQsClient => Some(Self::DeleteUsernames),
            Self::DeleteUsernames => Some(Self::LeaveChats),
            Self::LeaveChats => Some(Self::DeleteQsUser),
            Self::DeleteQsUser => Some(Self::DeleteAsUser),
            Self::DeleteAsUser => None,
        }
    }
}

impl OperationData for AccountDeletion {
    fn kind() -> OperationKind {
        OperationKind::AccountDeletion
    }

    fn generate_id(&self) -> OperationId {
        OperationId(OPERATION_ID.to_vec())
    }
}

/// How far a run of the account deletion got.
#[derive(Debug)]
pub(crate) enum AccountDeletionProgress {
    /// The account is deleted and this device must reset itself.
    Completed,
    /// The self group or QS client deletion kept failing. The deletion is
    /// given up and this device must reset itself.
    GaveUp,
    /// The self group deletion failed before anything reached the DS, or the
    /// DS refused the first delete sent. The deletion is dropped and nothing
    /// changed.
    Aborted(anyhow::Error),
    /// A sibling's self-group commit took the epoch. Processing the queue
    /// unblocks the deletion.
    Blocked,
    /// There is no deletion to run.
    Idle,
}

/// What is left of the self group deletion before its commit is sent.
enum SelfGroupDelete {
    Done,
    Blocked,
    /// `sent_before` tells whether an earlier attempt may have reached the
    /// DS.
    Send {
        operation: Box<PendingChatOperation>,
        sent_before: bool,
    },
}

/// The self group delete found by [`OutboundServiceContext::prepare_self_group_delete`].
enum Staged {
    Delete {
        operation: Box<PendingChatOperation>,
        sent_before: bool,
    },
    /// Another self-group commit is pending.
    Other(Box<PendingChatOperation>),
    /// Another run moved the deletion past the self group, or dropped it.
    Moved,
}

/// Where a run goes after a step.
enum Advance {
    Next(AccountDeletionStep),
    Completed,
    /// Another run dropped the deletion.
    Gone,
}

/// What a step achieved.
enum StepOutcome {
    Done,
    Blocked,
    /// The self group deletion failed before anything reached the DS, or the
    /// DS refused the first delete sent.
    Abort(anyhow::Error),
}

impl AccountDeletion {
    /// Records that this device deletes the account, unless it already does.
    pub(crate) async fn start(connection: impl WriteConnection) -> sqlx::Result<()> {
        Self::at(AccountDeletionStep::DeleteSelfGroup)
            .into_operation()
            .enqueue_if_not_exists(connection)
            .await
    }

    fn at(next_step: AccountDeletionStep) -> Self {
        Self {
            next_step,
            failed_attempts: 0,
            self_group_delete_sent: false,
        }
    }

    pub(crate) async fn is_pending(connection: impl ReadConnection) -> sqlx::Result<bool> {
        Operation::<Self>::exists(connection, &OperationId(OPERATION_ID.to_vec())).await
    }

    async fn load(connection: impl ReadConnection) -> anyhow::Result<Option<Self>> {
        let operation =
            Operation::<Self>::load(connection, &OperationId(OPERATION_ID.to_vec())).await?;
        Ok(operation.map(|operation| operation.data))
    }

    async fn store(&self, connection: impl WriteConnection) -> sqlx::Result<()> {
        self.clone().into_operation().enqueue(connection).await
    }

    /// Records that a self group delete is sent. Returns whether one was sent
    /// before.
    async fn mark_self_group_delete_sent(txn: &mut WriteDbTransaction<'_>) -> anyhow::Result<bool> {
        let Some(mut deletion) = Self::load(&mut *txn).await? else {
            return Ok(false);
        };
        if deletion.self_group_delete_sent {
            return Ok(true);
        }
        deletion.self_group_delete_sent = true;
        deletion.store(&mut *txn).await?;
        Ok(false)
    }

    async fn remove(connection: impl WriteConnection) -> sqlx::Result<()> {
        Self::at(AccountDeletionStep::DeleteSelfGroup)
            .into_operation()
            .delete(connection)
            .await
    }

    /// Records that the own self group is closed, which only happens when the
    /// account is deleted.
    ///
    /// If this device deletes the account, the self group is done and the
    /// deletion goes on with the next step. Otherwise a sibling deleted the
    /// account, see [`OwnClientInfo::self_group_closed`].
    pub(crate) async fn self_group_closed(txn: &mut WriteDbTransaction<'_>) -> anyhow::Result<()> {
        match Self::load(&mut *txn).await? {
            Some(Self {
                next_step: AccountDeletionStep::DeleteSelfGroup,
                ..
            }) => {
                info!("the self group is closed, continuing the account deletion");
                Self::at(AccountDeletionStep::DeleteQsClient)
                    .store(&mut *txn)
                    .await?;
            }
            Some(_) => {}
            None => OwnClientInfo::self_group_closed(txn).await?,
        }
        Ok(())
    }

    /// Removes the deletion and records that this device must reset itself.
    async fn complete(txn: &mut WriteDbTransaction<'_>) -> anyhow::Result<()> {
        Self::remove(&mut *txn).await?;
        OwnClientInfo::mark_account_unlinked(&mut *txn, UnlinkReason::AccountDeleted).await?;
        Ok(())
    }
}

impl OutboundServiceContext {
    /// Runs the remaining steps of the account deletion, if this device
    /// deletes the account.
    ///
    /// Network errors and rate limiting stop the run without counting as a
    /// failed attempt.
    ///
    /// If the self group deletion fails before anything reached the DS, or the
    /// DS refuses the first delete sent, the deletion is dropped. Any other
    /// failure of the self group or QS client deletion keeps that step for the
    /// next run, and after [`MAX_FAILED_ATTEMPTS`] the deletion is given up and
    /// this device resets itself. The later steps are best effort, a failure
    /// is logged and skipped.
    pub(crate) async fn run_account_deletion(
        &self,
    ) -> Result<AccountDeletionProgress, OutboundServiceError> {
        // A sibling that unlinked this device first won, and this device resets
        // itself anyway.
        if OwnClientInfo::account_unlink_reason(self.db.read().await?)
            .await
            .map_err(OutboundServiceError::fatal)?
            .is_some()
        {
            return Ok(AccountDeletionProgress::Idle);
        }
        let Some(deletion) = AccountDeletion::load(self.db.read().await?)
            .await
            .map_err(OutboundServiceError::fatal)?
        else {
            return Ok(AccountDeletionProgress::Idle);
        };

        // Another run of the deletion, in the foreground or in another process,
        // can go on at the same time. The persisted state is therefore only
        // read and changed in transactions, never written back from a copy.
        let api_client = self.api_clients.default_client()?;
        let mut step = deletion.next_step;
        loop {
            info!(?step, "Account deletion step");
            let result = match step {
                AccountDeletionStep::DeleteSelfGroup => Box::pin(self.delete_self_group()).await,
                AccountDeletionStep::DeleteQsClient => self.delete_qs_client(&api_client).await,
                AccountDeletionStep::DeleteUsernames => {
                    best_effort(step, self.delete_usernames(&api_client).await)
                }
                AccountDeletionStep::LeaveChats => {
                    best_effort(step, self.leave_chats(&api_client).await)
                }
                AccountDeletionStep::DeleteQsUser => {
                    best_effort(step, self.delete_qs_user(&api_client).await)
                }
                AccountDeletionStep::DeleteAsUser => {
                    best_effort(step, self.delete_as_user(&api_client).await)
                }
            };
            match result {
                Ok(StepOutcome::Done) => {}
                Ok(StepOutcome::Abort(error)) => {
                    error!(
                        %error,
                        "Self group deletion failed without a tombstone, dropping the account deletion"
                    );
                    AccountDeletion::remove(self.db.write().await?).await?;
                    return Ok(AccountDeletionProgress::Aborted(error));
                }
                Ok(StepOutcome::Blocked) => {
                    if self.record_failed_attempt(step).await? {
                        return Ok(AccountDeletionProgress::GaveUp);
                    }
                    return Ok(AccountDeletionProgress::Blocked);
                }
                Err(
                    error @ (OutboundServiceError::NetworkError
                    | OutboundServiceError::RateLimited { .. }),
                ) => return Err(error),
                Err(
                    error @ (OutboundServiceError::Recoverable(_) | OutboundServiceError::Fatal(_)),
                ) => {
                    if self.record_failed_attempt(step).await? {
                        return Ok(AccountDeletionProgress::GaveUp);
                    }
                    return Err(error);
                }
            }

            step = match self.advance_account_deletion(step).await? {
                Advance::Next(next_step) => next_step,
                Advance::Completed => {
                    info!("Deleted the account");
                    return Ok(AccountDeletionProgress::Completed);
                }
                Advance::Gone => return Ok(AccountDeletionProgress::Idle),
            };
        }
    }

    /// Moves the deletion past `step`, unless another run already moved it.
    /// Completes the deletion after the last step.
    async fn advance_account_deletion(
        &self,
        step: AccountDeletionStep,
    ) -> Result<Advance, OutboundServiceError> {
        self.db
            .with_write_transaction(async |txn| -> anyhow::Result<_> {
                let Some(deletion) = AccountDeletion::load(&mut *txn).await? else {
                    let reason = OwnClientInfo::account_unlink_reason(&mut *txn).await?;
                    return Ok(match reason {
                        Some(UnlinkReason::AccountDeleted) => Advance::Completed,
                        Some(UnlinkReason::Unlinked) | None => Advance::Gone,
                    });
                };
                if deletion.next_step != step {
                    return Ok(Advance::Next(deletion.next_step));
                }
                match step.next() {
                    Some(next_step) => {
                        AccountDeletion::at(next_step).store(&mut *txn).await?;
                        Ok(Advance::Next(next_step))
                    }
                    None => {
                        AccountDeletion::complete(txn).await?;
                        Ok(Advance::Completed)
                    }
                }
            })
            .await
            .map_err(OutboundServiceError::fatal)
    }

    /// Counts a failed attempt of `step`, unless another run already moved the
    /// deletion past it. After [`MAX_FAILED_ATTEMPTS`], gives up on the
    /// deletion, resets this device and returns `true`.
    async fn record_failed_attempt(
        &self,
        step: AccountDeletionStep,
    ) -> Result<bool, OutboundServiceError> {
        self.db
            .with_write_transaction(async |txn| -> anyhow::Result<_> {
                let Some(mut deletion) = AccountDeletion::load(&mut *txn).await? else {
                    return Ok(false);
                };
                if deletion.next_step != step {
                    return Ok(false);
                }
                deletion.failed_attempts += 1;
                if deletion.failed_attempts < MAX_FAILED_ATTEMPTS {
                    deletion.store(&mut *txn).await?;
                    return Ok(false);
                }
                error!(
                    ?step,
                    failed_attempts = deletion.failed_attempts,
                    "Account deletion keeps failing, giving up and resetting this device"
                );
                AccountDeletion::complete(txn).await?;
                Ok(true)
            })
            .await
            .map_err(OutboundServiceError::fatal)
    }

    /// Deletes the self group if there are linked devices. The delete commit
    /// sets a tombstone, which tells them that the account is deleted.
    async fn delete_self_group(&self) -> Result<StepOutcome, OutboundServiceError> {
        let (operation, sent_before) = match self.prepare_self_group_delete().await {
            Ok(SelfGroupDelete::Done) => return Ok(StepOutcome::Done),
            Ok(SelfGroupDelete::Blocked) => return Ok(StepOutcome::Blocked),
            Ok(SelfGroupDelete::Send {
                operation,
                sent_before,
            }) => (*operation, sent_before),
            Err(OutboundServiceError::Fatal(error)) => {
                let sent = AccountDeletion::load(self.db.read().await?)
                    .await
                    .map_err(OutboundServiceError::fatal)?
                    .is_some_and(|deletion| deletion.self_group_delete_sent);
                // Nothing reached the DS yet, so nothing changed.
                if !sent {
                    return Ok(StepOutcome::Abort(error));
                }
                return Err(OutboundServiceError::Fatal(error));
            }
            Err(error) => return Err(error),
        };

        match self.execute_job(operation).await {
            Ok(_) | Err(JobError::GroupClosed | JobError::NotFound) => Ok(StepOutcome::Done),
            Err(JobError::Blocked) => Ok(StepOutcome::Blocked),
            // The DS refused the only delete ever sent, so nothing changed.
            // After an earlier send, a refusal proves nothing, because the DS
            // can refuse a request before it checks for the tombstone.
            Err(JobError::Fatal(error)) if !sent_before && is_ds_refusal(&error) => {
                Ok(StepOutcome::Abort(error))
            }
            Err(error) => Err(error.into()),
        }
    }

    /// Returns the self group delete to send: the one of an earlier attempt or
    /// of another run of the deletion, or a newly staged one. Records that it
    /// is sent.
    async fn prepare_self_group_delete(&self) -> Result<SelfGroupDelete, OutboundServiceError> {
        let Some(chat) = self.self_chat().await? else {
            return Ok(SelfGroupDelete::Done);
        };
        // An inactive self chat is either deleted by an earlier attempt or
        // found closed.
        if chat.status().is_inactive()
            || !SelfGroup::load_and_check_if_has_linked_device(self.db.read().await?).await?
        {
            return Ok(SelfGroupDelete::Done);
        }

        loop {
            // Looking up, staging and marking the delete as sent in one
            // transaction makes concurrent runs share a single delete.
            let staged = self
                .db
                .with_write_transaction(async |txn| -> anyhow::Result<_> {
                    if !AccountDeletion::load(&mut *txn)
                        .await?
                        .is_some_and(|deletion| {
                            deletion.next_step == AccountDeletionStep::DeleteSelfGroup
                        })
                    {
                        return Ok(Staged::Moved);
                    }
                    let operation = match PendingChatOperation::load(&mut *txn, &chat.id()).await? {
                        Some(operation) if operation.is_delete() => operation,
                        Some(operation) => return Ok(Staged::Other(Box::new(operation))),
                        None => {
                            PendingChatOperation::create_delete(
                                txn,
                                self.signing_key(),
                                chat.id(),
                                Some(TimeStamp::now()),
                            )
                            .await?
                        }
                    };
                    let sent_before = AccountDeletion::mark_self_group_delete_sent(txn).await?;
                    Ok(Staged::Delete {
                        operation: Box::new(operation),
                        sent_before,
                    })
                })
                .await
                .map_err(OutboundServiceError::fatal)?;
            match staged {
                Staged::Moved => return Ok(SelfGroupDelete::Done),
                Staged::Delete {
                    operation,
                    sent_before,
                } => {
                    return Ok(SelfGroupDelete::Send {
                        operation,
                        sent_before,
                    });
                }
                // Another self-group commit goes first, the delete needs a
                // group without a pending commit.
                Staged::Other(operation) => match self.execute_job(*operation).await {
                    Ok(_) => {}
                    Err(JobError::GroupClosed | JobError::NotFound) => {
                        return Ok(SelfGroupDelete::Done);
                    }
                    Err(JobError::Blocked) => return Ok(SelfGroupDelete::Blocked),
                    Err(error) => return Err(error.into()),
                },
            }
        }
    }

    async fn self_chat(&self) -> Result<Option<Chat>, OutboundServiceError> {
        Ok(self
            .db
            .with_read_transaction(async |txn| SelfGroup::load_chat(txn).await)
            .await?)
    }

    async fn delete_qs_client(
        &self,
        api_client: &ApiClient,
    ) -> Result<StepOutcome, OutboundServiceError> {
        match api_client
            .qs_delete_client(self.qs_client_id, &self.key_store.qs_client_signing_key)
            .await
        {
            Ok(()) => info!("Deleted QS client"),
            Err(error) if is_not_found(error.kind()) => info!("QS client already deleted"),
            Err(error) => return Err(error.into()),
        }
        Ok(StepOutcome::Done)
    }

    /// Deletes the usernames on the server and locally, so that a later run
    /// only deletes the rest.
    async fn delete_usernames(&self, api_client: &ApiClient) -> Result<(), OutboundServiceError> {
        let records = UsernameRecord::load_all(self.db.read().await?).await?;
        info!(num_usernames = records.len(), "Deleting all usernames");

        let domain = self.user_id().domain();
        for record in records {
            let (token_request, _token_state) =
                privacy_pass::prepare_delete_token_request(self.db.read().await?, domain)
                    .await
                    .map_err(OutboundServiceError::fatal)?
                    .ok_or_else(|| {
                        OutboundServiceError::fatal(anyhow::anyhow!(
                            "no VOPRF keys available for delete token request"
                        ))
                    })?;
            // A username that is gone already is reported as not found.
            api_client
                .as_delete_username(record.hash, &record.signing_key, token_request)
                .await
                .map_err(OutboundServiceError::fatal)?;
            UsernameRecord::delete(self.db.write().await?, &record.username).await?;
        }
        Ok(())
    }

    /// Leaves all chats but the self group. A chat is torn down locally once
    /// it is left, so that a later run only leaves the rest.
    async fn leave_chats(&self, api_client: &ApiClient) -> Result<(), OutboundServiceError> {
        let chat_ids = Chat::load_ordered_ids(self.db.read().await?).await?;
        info!(num_chats = chat_ids.len(), "Leaving all chats");

        for chat_id in chat_ids {
            let staged = self
                .db
                .with_write_transaction(async |txn| -> anyhow::Result<_> {
                    let Some(chat) = Chat::load(&mut *txn, &chat_id).await? else {
                        return Ok(None);
                    };
                    if chat.status().is_inactive() {
                        return Ok(None);
                    }
                    let Some(mut group) = Group::load(&mut *txn, chat.group_id()).await? else {
                        return Ok(None);
                    };
                    // Nobody would commit a self-remove from the self group: it
                    // is either deleted already or has no other member.
                    if group.is_self_group() {
                        return Ok(None);
                    }
                    // A pending operation is never sent once the account is
                    // being deleted, and it would block the leave.
                    if PendingChatOperation::is_pending_for_group(&mut *txn, group.group_id())
                        .await?
                    {
                        group.discard_pending_commit(&mut *txn).await?;
                        PendingChatOperation::delete(&mut *txn, group.group_id()).await?;
                    }
                    let signer = OwnClientInfo::signer_for_group(
                        &mut *txn,
                        group.group_id(),
                        self.signing_key(),
                    )
                    .await?;
                    let identity = signer.room_policy_identity();
                    group.room_state_change_role_identity(
                        &identity,
                        &identity,
                        RoleIndex::Outsider,
                    )?;
                    let params = group.stage_leave_group(&mut *txn, &signer)?;
                    let ear_key = group.group_state_ear_key().clone();
                    Ok(Some((group.group_id().clone(), params, ear_key, signer)))
                })
                .await
                .map_err(OutboundServiceError::fatal)?;
            let Some((group_id, params, ear_key, signer)) = staged else {
                continue;
            };

            match api_client.ds_self_remove(params, &signer, &ear_key).await {
                Ok(_) => {}
                // Also covers a closed group
                Err(error) if error.is_not_found() => {
                    warn!(?chat_id, "Group already gone from server, skipping");
                }
                Err(error) => match OutboundServiceError::from(error) {
                    error @ (OutboundServiceError::NetworkError
                    | OutboundServiceError::RateLimited { .. }) => return Err(error),
                    error => {
                        error!(%error, ?chat_id, "Failed to leave chat");
                        continue;
                    }
                },
            }
            self.db
                .with_write_transaction(async |txn| {
                    handle_group_not_found_on_ds(txn, &group_id).await
                })
                .await
                .map_err(OutboundServiceError::fatal)?;
        }
        info!("Left all chats");
        Ok(())
    }

    async fn delete_qs_user(&self, api_client: &ApiClient) -> Result<(), OutboundServiceError> {
        let qs_user_id = OwnClientInfo::load(self.db.read().await?).await?.qs_user_id;
        match api_client
            .qs_delete_user(qs_user_id, &self.key_store.qs_user_signing_key)
            .await
        {
            Ok(()) => info!("Deleted QS user"),
            Err(error) if is_not_found(error.kind()) => info!("QS user already deleted"),
            Err(error) => return Err(error.into()),
        }
        Ok(())
    }

    async fn delete_as_user(&self, api_client: &ApiClient) -> Result<(), OutboundServiceError> {
        match api_client
            .as_delete_user(self.user_id().clone(), self.signing_key())
            .await
        {
            Ok(()) => info!("Deleted AS user"),
            Err(error) if error.is_not_found() => info!("AS user already deleted"),
            Err(error) => return Err(OutboundServiceError::fatal(error)),
        }
        Ok(())
    }
}

/// Whether the DS refused a request for good, so it didn't apply it.
fn is_ds_refusal(error: &anyhow::Error) -> bool {
    error.chain().any(|error| {
        error.downcast_ref::<DsRequestError>().is_some_and(|error| {
            matches!(error, DsRequestError::Tonic(_))
                && matches!(error.kind(), RequestErrorKind::Rejected)
        })
    })
}

fn is_not_found(kind: RequestErrorKind) -> bool {
    matches!(kind, RequestErrorKind::NotFound)
}

/// Passes on network errors and rate limiting, which keep the step for the
/// next run, and logs and skips any other failure.
fn best_effort(
    step: AccountDeletionStep,
    result: Result<(), OutboundServiceError>,
) -> Result<StepOutcome, OutboundServiceError> {
    match result {
        Ok(()) => Ok(StepOutcome::Done),
        Err(
            error @ (OutboundServiceError::NetworkError | OutboundServiceError::RateLimited { .. }),
        ) => Err(error),
        Err(error @ (OutboundServiceError::Recoverable(_) | OutboundServiceError::Fatal(_))) => {
            error!(%error, ?step, "Account deletion step failed, skipping it");
            Ok(StepOutcome::Done)
        }
    }
}

#[cfg(test)]
mod tests {
    use aircommon::identifiers::{QsClientId, QsUserId, UserId};
    use sqlx::SqlitePool;
    use uuid::Uuid;

    use crate::db::access::DbAccess;

    use super::*;

    async fn setup(pool: SqlitePool) -> anyhow::Result<DbAccess> {
        let db = DbAccess::for_tests(pool);
        OwnClientInfo {
            qs_user_id: QsUserId::random(),
            qs_client_id: QsClientId::random(&mut rand::rng()),
            user_id: UserId::new(Uuid::new_v4(), "localhost".parse()?),
            client_id: Uuid::new_v4(),
            self_group_id: None,
            self_group_signing_key: None,
        }
        .store(db.write().await?)
        .await?;
        Ok(db)
    }

    async fn next_step(db: &DbAccess) -> anyhow::Result<Option<AccountDeletionStep>> {
        let deletion = AccountDeletion::load(db.read().await?).await?;
        Ok(deletion.map(|deletion| deletion.next_step))
    }

    async fn close_self_group(db: &DbAccess) -> anyhow::Result<()> {
        db.with_write_transaction(async |txn| AccountDeletion::self_group_closed(txn).await)
            .await
    }

    #[sqlx::test]
    async fn closed_self_group_continues_the_own_deletion(pool: SqlitePool) -> anyhow::Result<()> {
        let db = setup(pool).await?;
        AccountDeletion::start(db.write().await?).await?;

        close_self_group(&db).await?;

        assert_eq!(
            OwnClientInfo::account_unlink_reason(db.read().await?).await?,
            None
        );
        assert_eq!(
            next_step(&db).await?,
            Some(AccountDeletionStep::DeleteQsClient)
        );

        // A later step is not set back.
        AccountDeletion::at(AccountDeletionStep::LeaveChats)
            .store(db.write().await?)
            .await?;
        close_self_group(&db).await?;
        assert_eq!(next_step(&db).await?, Some(AccountDeletionStep::LeaveChats));
        Ok(())
    }

    #[sqlx::test]
    async fn starting_twice_keeps_the_progress(pool: SqlitePool) -> anyhow::Result<()> {
        let db = setup(pool).await?;
        AccountDeletion::start(db.write().await?).await?;
        AccountDeletion::at(AccountDeletionStep::DeleteQsUser)
            .store(db.write().await?)
            .await?;

        AccountDeletion::start(db.write().await?).await?;

        assert_eq!(
            next_step(&db).await?,
            Some(AccountDeletionStep::DeleteQsUser)
        );
        Ok(())
    }

    #[sqlx::test]
    async fn completing_resets_this_device(pool: SqlitePool) -> anyhow::Result<()> {
        let db = setup(pool).await?;
        AccountDeletion::start(db.write().await?).await?;

        db.with_write_transaction(async |txn| AccountDeletion::complete(txn).await)
            .await?;

        assert!(!AccountDeletion::is_pending(db.read().await?).await?);
        assert_eq!(
            OwnClientInfo::account_unlink_reason(db.read().await?).await?,
            Some(UnlinkReason::AccountDeleted)
        );
        Ok(())
    }
}
