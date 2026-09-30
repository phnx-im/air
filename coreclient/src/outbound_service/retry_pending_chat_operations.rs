// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::credentials::keys::SelfGroupSigningKey;
use airprotos::client::self_group::{BlockedContactsUpdate, SelfGroupMessage, SettingsUpdate};
use anyhow::Context as _;
use openmls::group::GroupId;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};
use uuid::Uuid;

use crate::{
    chats,
    clients::{
        block_contact,
        own_client_info::OwnClientInfo,
        user_settings::{SettingChanges, SettingsUpdateExt},
    },
    db::access::WriteDbTransaction,
    groups::{Group, VerifiedGroup},
    job::{JobError, pending_chat_operation::PendingChatOperation},
    outbound_service::{OutboundServiceContext, error::OutboundServiceRunError},
    privacy_pass,
};

impl OutboundServiceContext {
    pub(super) async fn send_pending_chat_operations(
        &self,
        run_token: &CancellationToken,
    ) -> Result<(), OutboundServiceRunError> {
        // Used to identify locked receipts by this task
        let task_id = Uuid::new_v4();
        loop {
            if run_token.is_cancelled() {
                return Ok(()); // the task is being stopped
            }

            // Turn pending self-group state into a commit when the self-group
            // is free. Runs every iteration: when an operation completes with
            // work left parked (the user re-toggled a setting while the commit
            // was in flight), the next iteration issues a fresh commit.
            if let Err(error) = self.ensure_self_group_messages_operation().await {
                error!(%error, "Failed to stage the parked self-group messages");
            }
            if let Err(error) = self.ensure_token_seed_operation().await {
                error!(%error, "Failed to stage pending token seeds");
            }

            let now = chrono::Utc::now();

            let pending_chat_operation = self
                .db
                .with_write_transaction(async |txn| {
                    PendingChatOperation::dequeue(txn, task_id, now).await
                })
                .await?;
            let Some(pending_chat_operation) = pending_chat_operation else {
                return Ok(());
            };

            let group_id = pending_chat_operation.group_id().clone();
            debug!(?group_id, "dequeued pending chat operation for retry");

            // The job manages its own retry count and deletion upon success.
            // We're just executing it here.
            match self.execute_job(pending_chat_operation).await {
                Err(JobError::NetworkError) => {
                    // If we're getting a network error, error out of the loop and wait for the next run.
                    return Err(OutboundServiceRunError::NetworkError);
                }
                Err(error @ (JobError::Fatal(_) | JobError::Domain(_))) => {
                    error!(%error, ?group_id, "Failed to execute pending chat operation");
                    // This job has a fatal error. Continue with the next one.
                    continue;
                }
                Err(JobError::Blocked | JobError::NotFound) => {
                    continue;
                }
                Ok(_) => (),
            }
        }
    }

    /// Stages one self-group commit for everything parked in the outbox.
    ///
    /// All kinds travel in the same commit, so a setting change and a block
    /// parked together reach the siblings in one round trip instead of taking
    /// turns at the self-group's single operation slot.
    async fn ensure_self_group_messages_operation(&self) -> anyhow::Result<()> {
        let Some((self_group_id, signer)) = self.self_group_signer().await? else {
            return Ok(());
        };

        self.db
            .with_write_transaction(async |txn| {
                let messages = drain_outbox(txn).await?;
                if messages.is_empty() {
                    return Ok(());
                }
                let Some(group) = free_self_group(txn, &self_group_id).await? else {
                    return Ok(());
                };

                PendingChatOperation::create_self_group_messages(txn, &signer, group, messages)
                    .await?;
                Ok(())
            })
            .await
    }

    /// Stages a self-group commit publishing the token seeds that still have to
    /// reach the siblings, if any.
    ///
    /// These are fresh proposals, and seeds that won a divergence and are
    /// re-broadcast so the sibling holding the losing seed converges. A device
    /// gets no tokens under a key until its seed is agreed, so this runs on
    /// every outbound wake until the commit lands.
    async fn ensure_token_seed_operation(&self) -> anyhow::Result<()> {
        let Some((self_group_id, signer)) = self.self_group_signer().await? else {
            return Ok(());
        };

        self.db
            .with_write_transaction(async |txn| {
                let seeds = privacy_pass::seeds_to_broadcast(&mut *txn).await?;
                if seeds.is_empty() {
                    return Ok(());
                }
                let Some(group) = free_self_group(txn, &self_group_id).await? else {
                    return Ok(());
                };

                PendingChatOperation::create_token_seeds(txn, &signer, group, seeds).await?;
                Ok(())
            })
            .await
    }

    /// The self group and the per-device key its commits are signed with.
    async fn self_group_signer(&self) -> anyhow::Result<Option<(GroupId, SelfGroupSigningKey)>> {
        let info = OwnClientInfo::load(self.db.read().await?).await?;
        let Some(self_group_id) = info.self_group_id else {
            return Ok(None);
        };
        let Some(signer) = info.self_group_signing_key else {
            return Ok(None);
        };
        Ok(Some((self_group_id, signer)))
    }
}

/// The parked changes, as the messages one commit carries.
async fn drain_outbox(txn: &mut WriteDbTransaction<'_>) -> anyhow::Result<Vec<SelfGroupMessage>> {
    let mut messages = Vec::new();

    if SettingChanges::has_pending(&mut *txn).await? {
        messages.push(SelfGroupMessage::SettingsUpdate(
            SettingsUpdate::collect(&mut *txn).await?,
        ));
    }

    let contacts = block_contact::persistence::staged_entries(&mut *txn).await?;
    if !contacts.is_empty() {
        messages.push(SelfGroupMessage::BlockedContactsUpdate(
            BlockedContactsUpdate { contacts },
        ));
    }

    messages.extend(
        chats::persistence::staged_deletions(&mut *txn)
            .await?
            .into_iter()
            .map(SelfGroupMessage::DeletedChat),
    );

    Ok(messages)
}

/// The self group, if a commit can be staged on it right now.
///
/// `None` when a self-group operation is already in flight (including one parked
/// on a wrong-epoch rejection, which waits for the winning commit to arrive
/// through the queue and delete it), or when the self group carries a pending
/// commit that no operation row belongs to.
async fn free_self_group(
    txn: &mut WriteDbTransaction<'_>,
    self_group_id: &GroupId,
) -> anyhow::Result<Option<VerifiedGroup>> {
    if PendingChatOperation::load_by_group_id(&mut *txn, self_group_id)
        .await?
        .is_some()
    {
        return Ok(None);
    }

    let group = Group::load_verified(&mut *txn, self_group_id)
        .await?
        .with_context(|| format!("Can't find self group with id {self_group_id:?}"))?;

    // A pending commit that no operation row accounts for is not ours to build
    // on. Defer, the next wake retries.
    if let Err(error) = group.ensure_clean() {
        debug!(%error, "Self group has a pending commit, deferring self-group state");
        return Ok(None);
    }

    Ok(Some(group))
}
