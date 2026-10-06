// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airapiclient::ApiClient;
use airprotos::client::self_group::{AccountDeleted, SelfGroupMessage};
use anyhow::Context;
use mimi_room_policy::RoleIndex;
use tracing::{error, info, warn};

use crate::{
    UsernameRecord,
    clients::{CoreUser, own_client_info::OwnClientInfo},
    delete_client_database,
    groups::{Group, self_group::SelfGroup},
    job::{JobError, chat_operation::ChatOperation},
    privacy_pass,
};

impl CoreUser {
    /// Deletes the account on the server and locally.
    ///
    /// 1. Announce the deletion to linked devices (mandatory for success)
    /// 2. Delete QS queue (mandatory for success)
    /// 3. Delete usernames
    /// 4. Batch self-remove from all groups except the self group as a single transaction
    /// 5. Delete QS user
    /// 6. Delete AS identity
    ///
    /// Finally, the client database is deleted if a `db_path` is provided.
    pub async fn delete_account(&self, db_path: Option<&str>) -> anyhow::Result<()> {
        self.announce_deletion_to_siblings().await?;

        let client = self.api_client()?;

        let client_id = self.inner.qs_client_id;
        let qs_client_signing_key = &self.inner.key_store.qs_client_signing_key;

        client
            .qs_delete_client(client_id, qs_client_signing_key)
            .await?;

        // After the qs client is deleted, there is no way back and everything else after it is
        // best effort.

        self.delete_all_usernames(&client).await;
        self.leave_all_chats(&client).await;

        self.delete_qs_identity(&client).await;
        self.delete_as_identity(&client).await;

        if let Some(db_path) = db_path {
            delete_client_database(db_path, self.client_record_id()).await?;
        }

        Ok(())
    }

    /// Tells the linked devices through the self group that the account is
    /// being deleted, so that they reset themselves.
    async fn announce_deletion_to_siblings(&self) -> anyhow::Result<()> {
        const MAX_ATTEMPTS: usize = 3;

        if !SelfGroup::load_and_check_if_has_linked_device(self.db().read().await?).await? {
            return Ok(());
        }
        let Some(chat_id) = self.self_chat_id().await? else {
            return Ok(());
        };
        info!("Announcing the account deletion to linked devices");

        let mut attempt = 1;
        loop {
            let operation = ChatOperation::self_group_messages(
                chat_id,
                vec![SelfGroupMessage::AccountDeleted(AccountDeleted {})],
            );
            match self.execute_job(operation).await {
                Ok(_) => return Ok(()),
                // A sibling commit won the epoch. Catch up on it and try again.
                Err(JobError::Blocked) if attempt < MAX_ATTEMPTS => {
                    warn!(attempt, "Deletion announcement lost the epoch, retrying");
                    let processed = self.drain_and_process_qs_queue().await?;
                    if let Some(error) = processed.errors.first() {
                        warn!(%error, "Failed to process queued messages before retrying");
                    }
                    attempt += 1;
                }
                Err(error) => return Err(error.into()),
            }
        }
    }

    async fn delete_qs_identity(&self, client: &ApiClient) {
        if let Err(error) = client
            .qs_delete_user(
                self.inner.qs_user_id,
                &self.inner.key_store.qs_user_signing_key,
            )
            .await
        {
            error!(%error, "Error deleting QS user");
        } else {
            info!("Deleted QS user");
        }
    }

    async fn leave_all_chats(&self, api_client: &ApiClient) {
        if let Err(error) = self.try_leave_all_chats(api_client).await {
            error!(%error, "Error leaving all chats");
        }
    }

    async fn try_leave_all_chats(&self, api_client: &ApiClient) -> anyhow::Result<()> {
        let chat_ids = self.ordered_chat_ids().await?;
        info!(num_chats = chat_ids.len(), "Leaving all chats");

        let removals = self
            .db()
            .with_write_transaction(async |txn| -> anyhow::Result<_> {
                let mut removals = Vec::with_capacity(chat_ids.len());
                for chat_id in chat_ids {
                    let mut group = Group::load_with_chat_id_clean(&mut *txn, chat_id)
                        .await?
                        .with_context(|| format!("Can't find group with chat id {chat_id:?}"))?;
                    // There is not point in sending a SelfRemove into the self-group, because no
                    // client will ever commit it.
                    if group.is_self_group() {
                        continue;
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
                    removals.push((params, ear_key, signer));
                }
                Ok(removals)
            })
            .await?;

        for (params, ear_key, signer) in removals {
            match api_client.ds_self_remove(params, &signer, &ear_key).await {
                Ok(_) => {}
                Err(e) if e.is_not_found() => {
                    warn!("Group already gone from server; skipping");
                }
                Err(e) => return Err(e.into()),
            }
        }

        info!("Left all chats");
        Ok(())
    }

    async fn delete_all_usernames(&self, api_client: &ApiClient) {
        if let Err(error) = self.try_delete_all_usernames(api_client).await {
            error!(%error, "Error deleting all usernames");
        }
    }

    async fn try_delete_all_usernames(&self, api_client: &ApiClient) -> anyhow::Result<()> {
        let usernames = self.usernames().await?;
        info!(num_usernames = usernames.len(), "Deleting all usernames");

        let records = UsernameRecord::load_all(self.db().read().await?).await?;
        let domain = self.user_id().domain();
        for record in records {
            let (token_request, _token_state) =
                privacy_pass::prepare_delete_token_request(self.db().write().await?, domain)
                    .await?
                    .context("no VOPRF keys available for delete token request")?;
            api_client
                .as_delete_username(record.hash, &record.signing_key, token_request)
                .await?;
        }

        info!("Deleted all usernames");
        Ok(())
    }

    async fn delete_as_identity(&self, api_client: &ApiClient) {
        if let Err(error) = self.try_delete_as_identity(api_client).await {
            error!(%error, "Error deleting AS identity");
        }
    }

    async fn try_delete_as_identity(&self, api_client: &ApiClient) -> anyhow::Result<()> {
        let user_id = self.user_id();
        let signing_key = self.signing_key();
        api_client
            .as_delete_user(user_id.clone(), signing_key)
            .await?;
        info!("Deleted AS user");
        Ok(())
    }
}
