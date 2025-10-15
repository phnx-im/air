// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airapiclient::ApiClient;
use anyhow::Context;
use mimi_room_policy::RoleIndex;
use tracing::{error, info};

use crate::{
    UserHandleRecord, clients::CoreUser, delete_client_database, groups::Group, store::Store,
};

impl CoreUser {
    /// Deletes the account on the server and locally.
    ///
    /// 1. Delete QS queue (mandatory for success)
    /// 2. Delete usernames
    /// 3. Batch self-remove from groups as a single transaction
    /// 4. Delete AS identity
    ///
    /// Finally, the client database is deleted if a `db_path` is provided.
    pub async fn delete_account(&self, db_path: Option<&str>) -> anyhow::Result<()> {
        let client = self.api_client()?;

        let client_id = self.inner.qs_client_id;
        let qs_client_signing_key = &self.inner.key_store.qs_client_signing_key;

        client
            .qs_delete_client(client_id, qs_client_signing_key)
            .await?;

        // After the qs client is deleted, there is no way back and everything else after it is
        // best effort.

        self.delete_all_user_handles(&client).await;
        self.leave_all_chats(&client).await;

        self.delete_qs_identity(&client).await;
        self.delete_as_identity(&client).await;

        if let Some(db_path) = db_path {
            delete_client_database(db_path, self.user_id()).await?;
        }

        Ok(())
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
        let user_id = self.user_id();

        let chats = self.chats().await?;
        info!(num_chats = chats.len(), "Leaving all chats");

        let removals = self
            .with_transaction(async |txn| -> anyhow::Result<_> {
                let mut removals = Vec::with_capacity(chats.len());
                for chat in chats {
                    let group_id = chat.group_id();
                    let mut group = Group::load_clean(txn, group_id)
                        .await?
                        .with_context(|| format!("Can't find group with id {group_id:?}"))?;
                    group.room_state_change_role(user_id, user_id, RoleIndex::Outsider)?;
                    let params = group.stage_leave_group(txn.as_mut(), self.signing_key())?;
                    let ear_key = group.group_state_ear_key().clone();
                    removals.push((params, ear_key));
                }
                Ok(removals)
            })
            .await?;

        for (params, ear_key) in removals {
            api_client
                .ds_self_remove(params, self.signing_key(), &ear_key)
                .await?;
        }

        info!("Left all chats");
        Ok(())
    }

    async fn delete_all_user_handles(&self, api_client: &ApiClient) {
        if let Err(error) = self.try_delete_all_user_handles(api_client).await {
            error!(%error, "Error deleting all user handles");
        }
    }

    async fn try_delete_all_user_handles(&self, api_client: &ApiClient) -> anyhow::Result<()> {
        let user_handles = self.user_handles().await?;
        info!(
            num_handles = user_handles.len(),
            "Deleting all user handles"
        );

        let records = UserHandleRecord::load_all(self.pool()).await?;
        for record in records {
            api_client
                .as_delete_handle(record.hash, &record.signing_key)
                .await?;
        }

        info!("Deleted all user handles");
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
