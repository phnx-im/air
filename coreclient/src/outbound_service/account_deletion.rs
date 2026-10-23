// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airapiclient::{ApiClient, ClassifyRequestError, RequestErrorKind, ds_api::DsRequestError};
use aircommon::time::TimeStamp;
use chrono::Utc;
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
        recoverable::RecoverableCause,
    },
    privacy_pass,
};

use super::{
    OutboundServiceContext,
    error::OutboundServiceError,
    retry::{RetryDecision, RetryPolicy},
};

const OPERATION_ID: &[u8] = b"account_deletion";

/// The persisted deletion of the account, started on this device.
///
/// While it runs, it stores the next step, so that a run picks up where the
/// last one stopped, also after a restart.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum AccountDeletion {
    Running(AccountDeletionStep),
    /// The self group deletion failed before anything reached the DS, or the
    /// DS refused the first delete sent. Nothing changed.
    Aborted {
        error: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum AccountDeletionStep {
    /// Deletes the self group, which resets the linked devices.
    DeleteSelfGroup {
        /// Whether a self group delete was sent. The DS may have accepted it
        /// without a response arriving.
        delete_sent: bool,
    },
    DeleteQsClient,
    DeleteUsernames,
    LeaveChats,
    DeleteQsUser,
    DeleteAsUser,
}

impl AccountDeletionStep {
    fn next(self) -> Option<Self> {
        match self {
            Self::DeleteSelfGroup { .. } => Some(Self::DeleteQsClient),
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

/// What is left of the self group deletion before its commit is sent.
enum SelfGroupDelete {
    Done,
    Blocked,
    Send(Box<PendingChatOperation>),
}

/// The pending operation of the self group.
enum Staged {
    Delete(Box<PendingChatOperation>),
    /// Another self-group commit is pending.
    Other(Box<PendingChatOperation>),
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
    const FIRST_STEP: AccountDeletionStep =
        AccountDeletionStep::DeleteSelfGroup { delete_sent: false };

    /// Records that this device deletes the account.
    ///
    /// A running deletion keeps its progress and is due right away. An aborted
    /// one starts over.
    pub(crate) async fn start(txn: &mut WriteDbTransaction<'_>) -> anyhow::Result<()> {
        let Some(mut operation) = Self::load_operation(&mut *txn).await? else {
            Self::Running(Self::FIRST_STEP).store(&mut *txn).await?;
            return Ok(());
        };
        match operation.data {
            Self::Running(_) => operation.postpone(&mut *txn, Utc::now()).await?,
            Self::Aborted { .. } => {
                Self::Running(Self::FIRST_STEP).store(&mut *txn).await?;
            }
        }
        Ok(())
    }

    pub(crate) async fn load(connection: impl ReadConnection) -> anyhow::Result<Option<Self>> {
        let operation = Self::load_operation(connection).await?;
        Ok(operation.map(|operation| operation.data))
    }

    async fn load_operation(
        connection: impl ReadConnection,
    ) -> anyhow::Result<Option<Operation<Self>>> {
        Operation::load(connection, &OperationId(OPERATION_ID.to_vec())).await
    }

    /// Stores the deletion as due right away, without failed attempts.
    async fn store(self, connection: impl WriteConnection) -> sqlx::Result<Operation<Self>> {
        let operation = self.into_operation();
        operation.enqueue(connection).await?;
        Ok(operation)
    }

    async fn mark_self_group_delete_sent(connection: impl WriteConnection) -> sqlx::Result<()> {
        Self::Running(AccountDeletionStep::DeleteSelfGroup { delete_sent: true })
            .into_operation()
            .update_data(connection)
            .await
    }

    async fn abort(connection: impl WriteConnection, error: &anyhow::Error) -> sqlx::Result<()> {
        Self::Aborted {
            error: format!("{error:#}"),
        }
        .into_operation()
        .update_data(connection)
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
            Some(Self::Running(AccountDeletionStep::DeleteSelfGroup { .. })) => {
                info!("the self group is closed, continuing the account deletion");
                Self::Running(AccountDeletionStep::DeleteQsClient)
                    .store(&mut *txn)
                    .await?;
            }
            Some(Self::Running(_)) => {}
            Some(Self::Aborted { .. }) | None => OwnClientInfo::self_group_closed(txn).await?,
        }
        Ok(())
    }

    /// Removes the deletion and records that this device must reset itself.
    async fn complete(txn: &mut WriteDbTransaction<'_>) -> anyhow::Result<()> {
        Self::Running(Self::FIRST_STEP)
            .into_operation()
            .delete(&mut *txn)
            .await?;
        OwnClientInfo::mark_account_unlinked(&mut *txn, UnlinkReason::AccountDeleted).await?;
        Ok(())
    }
}

impl OutboundServiceContext {
    /// Runs the remaining steps of the account deletion, if this device
    /// deletes the account and the deletion is due. Returns whether this
    /// device deletes the account, which holds back all other work.
    ///
    /// Network errors and rate limiting stop the run without counting as a
    /// failed attempt.
    ///
    /// If the self group deletion fails before anything reached the DS, or the
    /// DS refuses the first delete sent, the deletion is aborted. Any other
    /// failure of the self group or QS client deletion keeps that step for a
    /// later run, with backoff. Once [`RetryPolicy::ACCOUNT_DELETION`] is used
    /// up, the deletion is given up and this device resets itself. The later
    /// steps are best effort, a failure is logged and skipped.
    pub(super) async fn run_account_deletion(&self) -> Result<bool, OutboundServiceError> {
        let Some(mut operation) = AccountDeletion::load_operation(self.db.read().await?)
            .await
            .map_err(OutboundServiceError::fatal)?
        else {
            return Ok(false);
        };
        let AccountDeletion::Running(mut step) = operation.data else {
            return Ok(false);
        };
        // A sibling that unlinked this device first won, and this device resets
        // itself anyway.
        let unlinked = OwnClientInfo::account_unlink_reason(self.db.read().await?)
            .await
            .map_err(OutboundServiceError::fatal)?
            .is_some();
        if unlinked || operation.scheduled_at > Utc::now() {
            return Ok(true);
        }

        let api_client = self.api_clients.default_client()?;
        loop {
            info!(?step, "Account deletion step");
            let result = match step {
                AccountDeletionStep::DeleteSelfGroup { delete_sent } => {
                    Box::pin(self.delete_self_group(delete_sent)).await
                }
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
            let failure = match result {
                Ok(StepOutcome::Done) => None,
                Ok(StepOutcome::Abort(error)) => {
                    error!(
                        %error,
                        "Self group deletion failed without a tombstone, aborting the account deletion"
                    );
                    AccountDeletion::abort(self.db.write().await?, &error).await?;
                    return Ok(false);
                }
                Ok(StepOutcome::Blocked) => Some((
                    RecoverableCause::WrongEpoch,
                    anyhow::anyhow!("a sibling's self group commit took the epoch"),
                )),
                Err(
                    error @ (OutboundServiceError::NetworkError
                    | OutboundServiceError::RateLimited { .. }),
                ) => return Err(error),
                Err(OutboundServiceError::Recoverable(error)) => Some((error.cause, error.into())),
                // The account is not deleted without this step, so it is
                // retried like after a server error.
                Err(OutboundServiceError::Fatal(error)) => Some((RecoverableCause::Server, error)),
            };
            if let Some((cause, error)) = failure {
                self.record_failed_attempt(&mut operation, step, cause, &error)
                    .await?;
                return Ok(true);
            }

            let Some(next_step) = step.next() else {
                self.db
                    .with_write_transaction(async |txn| AccountDeletion::complete(txn).await)
                    .await
                    .map_err(OutboundServiceError::fatal)?;
                info!("Deleted the account");
                return Ok(true);
            };
            operation = AccountDeletion::Running(next_step)
                .store(self.db.write().await?)
                .await?;
            step = next_step;
        }
    }

    /// Counts a failed attempt of `step` and defers the next one. Once
    /// [`RetryPolicy::ACCOUNT_DELETION`] is used up, gives up on the deletion
    /// and resets this device.
    async fn record_failed_attempt(
        &self,
        operation: &mut Operation<AccountDeletion>,
        step: AccountDeletionStep,
        cause: RecoverableCause,
        error: &anyhow::Error,
    ) -> Result<(), OutboundServiceError> {
        let attempts = u32::try_from(operation.retries).unwrap_or(u32::MAX);
        match RetryPolicy::ACCOUNT_DELETION.decide(cause, attempts) {
            RetryDecision::Retry => {
                warn!(?step, %error, "Account deletion step failed, retrying in the next run");
            }
            RetryDecision::Backoff { attempts, retry_in } => {
                warn!(
                    ?step,
                    attempts,
                    %error,
                    ?retry_in,
                    "Account deletion step failed, retrying later"
                );
                operation
                    .reschedule(self.db.write().await?, Utc::now() + retry_in)
                    .await?;
            }
            RetryDecision::GiveUp => {
                error!(
                    ?step,
                    %error,
                    "Account deletion keeps failing, giving up and resetting this device"
                );
                self.db
                    .with_write_transaction(async |txn| AccountDeletion::complete(txn).await)
                    .await
                    .map_err(OutboundServiceError::fatal)?;
            }
        }
        Ok(())
    }

    /// Deletes the self group if there are linked devices. The delete commit
    /// sets a tombstone, which tells them that the account is deleted.
    ///
    /// `sent_before` tells whether an earlier attempt sent a delete.
    async fn delete_self_group(
        &self,
        sent_before: bool,
    ) -> Result<StepOutcome, OutboundServiceError> {
        let operation = match self.prepare_self_group_delete(sent_before).await {
            Ok(SelfGroupDelete::Done) => return Ok(StepOutcome::Done),
            Ok(SelfGroupDelete::Blocked) => return Ok(StepOutcome::Blocked),
            Ok(SelfGroupDelete::Send(operation)) => *operation,
            // Nothing reached the DS yet, so nothing changed.
            Err(OutboundServiceError::Fatal(error)) if !sent_before => {
                return Ok(StepOutcome::Abort(error));
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
    /// a newly staged one. Records that it is sent.
    async fn prepare_self_group_delete(
        &self,
        sent_before: bool,
    ) -> Result<SelfGroupDelete, OutboundServiceError> {
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
            let staged = self
                .db
                .with_write_transaction(async |txn| -> anyhow::Result<_> {
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
                    // Recorded before the delete is sent, since the DS may
                    // accept it without a response arriving.
                    if !sent_before {
                        AccountDeletion::mark_self_group_delete_sent(&mut *txn).await?;
                    }
                    Ok(Staged::Delete(Box::new(operation)))
                })
                .await
                .map_err(OutboundServiceError::fatal)?;
            match staged {
                Staged::Delete(operation) => return Ok(SelfGroupDelete::Send(operation)),
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
    use std::assert_matches;

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

    async fn start(db: &DbAccess) -> anyhow::Result<()> {
        db.with_write_transaction(async |txn| AccountDeletion::start(txn).await)
            .await
    }

    async fn next_step(db: &DbAccess) -> anyhow::Result<Option<AccountDeletionStep>> {
        match AccountDeletion::load(db.read().await?).await? {
            Some(AccountDeletion::Running(step)) => Ok(Some(step)),
            Some(AccountDeletion::Aborted { error }) => anyhow::bail!("aborted: {error}"),
            None => Ok(None),
        }
    }

    async fn close_self_group(db: &DbAccess) -> anyhow::Result<()> {
        db.with_write_transaction(async |txn| AccountDeletion::self_group_closed(txn).await)
            .await
    }

    #[sqlx::test]
    async fn closed_self_group_continues_the_own_deletion(pool: SqlitePool) -> anyhow::Result<()> {
        let db = setup(pool).await?;
        start(&db).await?;

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
        AccountDeletion::Running(AccountDeletionStep::LeaveChats)
            .store(db.write().await?)
            .await?;
        close_self_group(&db).await?;
        assert_eq!(next_step(&db).await?, Some(AccountDeletionStep::LeaveChats));
        Ok(())
    }

    #[sqlx::test]
    async fn closed_self_group_after_an_aborted_deletion_is_a_sibling_deletion(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let db = setup(pool).await?;
        start(&db).await?;
        AccountDeletion::abort(db.write().await?, &anyhow::anyhow!("refused")).await?;

        close_self_group(&db).await?;

        assert_eq!(
            OwnClientInfo::account_unlink_reason(db.read().await?).await?,
            Some(UnlinkReason::AccountDeleted)
        );
        Ok(())
    }

    #[sqlx::test]
    async fn starting_again_keeps_the_progress_and_makes_it_due(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let db = setup(pool).await?;
        let mut operation = AccountDeletion::Running(AccountDeletionStep::DeleteQsClient)
            .store(db.write().await?)
            .await?;
        let later = Utc::now() + chrono::TimeDelta::minutes(10);
        operation.reschedule(db.write().await?, later).await?;

        start(&db).await?;

        let operation = AccountDeletion::load_operation(db.read().await?)
            .await?
            .expect("the deletion is running");
        assert_matches!(
            operation.data,
            AccountDeletion::Running(AccountDeletionStep::DeleteQsClient)
        );
        assert_eq!(operation.retries, 1);
        assert!(operation.scheduled_at <= Utc::now());
        Ok(())
    }

    #[sqlx::test]
    async fn starting_again_restarts_an_aborted_deletion(pool: SqlitePool) -> anyhow::Result<()> {
        let db = setup(pool).await?;
        start(&db).await?;
        AccountDeletion::mark_self_group_delete_sent(db.write().await?).await?;
        AccountDeletion::abort(db.write().await?, &anyhow::anyhow!("refused")).await?;

        start(&db).await?;

        assert_eq!(next_step(&db).await?, Some(AccountDeletion::FIRST_STEP));
        Ok(())
    }

    #[sqlx::test]
    async fn completing_resets_this_device(pool: SqlitePool) -> anyhow::Result<()> {
        let db = setup(pool).await?;
        start(&db).await?;

        db.with_write_transaction(async |txn| AccountDeletion::complete(txn).await)
            .await?;

        assert!(AccountDeletion::load(db.read().await?).await?.is_none());
        assert_eq!(
            OwnClientInfo::account_unlink_reason(db.read().await?).await?,
            Some(UnlinkReason::AccountDeleted)
        );
        Ok(())
    }
}
