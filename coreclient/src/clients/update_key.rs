// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use update_key_flow::UpdateKeyData;

use crate::{
    ChatId, ChatMessage,
    utils::connection_ext::{ConnectionExt, StoreExt},
};

use super::CoreUser;

impl CoreUser {
    /// Update the user's key material in the chat with the given
    /// [`ChatId`].
    ///
    /// Since this function causes the creation of an MLS commit, it can cause
    /// more than one effect on the group. As a result this function returns a
    /// vector of [`ChatMessage`]s that represents the changes to the
    /// group. Note that these returned message have already been persisted.
    pub(crate) async fn update_key(&self, chat_id: ChatId) -> anyhow::Result<Vec<ChatMessage>> {
        // Phase 1: Load the chat and the group
        let mut connection = self.pool().acquire().await?;
        let update = connection
            .with_transaction(async |txn| {
                UpdateKeyData::lock(txn, chat_id, self.signing_key()).await
            })
            .await?;

        // Phase 2: Send the update to the DS
        let updated = update
            .ds_update(&self.inner.api_clients, self.signing_key())
            .await?;

        // Phase 3: Merge the commit into the group
        self.with_notifier(async |notifier| {
            connection
                .with_transaction(async |txn| {
                    updated.merge_pending_commit(txn, notifier, chat_id).await
                })
                .await
        })
        .await
    }
}

mod update_key_flow {
    use aircommon::{
        credentials::keys::ClientSigningKey, messages::client_ds_out::UpdateParamsOut,
        time::TimeStamp,
    };
    use anyhow::Context;
    use sqlx::SqliteTransaction;

    use crate::{
        Chat, ChatId, ChatMessage,
        clients::{CoreUser, api_clients::ApiClients},
        groups::Group,
    };

    pub(super) struct UpdateKeyData {
        chat: Chat,
        group: Group,
        params: UpdateParamsOut,
    }

    impl UpdateKeyData {
        pub(super) async fn lock(
            txn: &mut SqliteTransaction<'_>,
            chat_id: ChatId,
            signer: &ClientSigningKey,
        ) -> anyhow::Result<Self> {
            let chat = Chat::load(txn.as_mut(), &chat_id)
                .await?
                .with_context(|| format!("Can't find chat with id {chat_id}"))?;
            let group_id = chat.group_id();
            let mut group = Group::load_clean(txn, group_id)
                .await?
                .with_context(|| format!("Can't find group with id {group_id:?}"))?;
            let params = group.update(txn, signer).await?;
            Ok(Self {
                chat,
                group,
                params,
            })
        }

        pub(super) async fn ds_update(
            self,
            api_clients: &ApiClients,
            signer: &ClientSigningKey,
        ) -> anyhow::Result<UpdatedKey> {
            let Self {
                chat,
                group,
                params,
            } = self;
            let owner_domain = chat.owner_domain();
            let ds_timestamp = api_clients
                .get(&owner_domain)?
                .ds_update(params, signer, group.group_state_ear_key())
                .await?;
            Ok(UpdatedKey {
                group,
                ds_timestamp,
            })
        }
    }

    pub(super) struct UpdatedKey {
        group: Group,
        ds_timestamp: TimeStamp,
    }
    impl UpdatedKey {
        pub(crate) async fn merge_pending_commit(
            self,
            connection: &mut sqlx::SqliteConnection,
            notifier: &mut crate::store::StoreNotifier,
            chat_id: ChatId,
        ) -> anyhow::Result<Vec<ChatMessage>> {
            let Self {
                mut group,
                ds_timestamp,
            } = self;
            let group_messages = group
                .merge_pending_commit(&mut *connection, None, ds_timestamp)
                .await?;
            group.store_update(&mut *connection).await?;
            CoreUser::store_new_messages(&mut *connection, notifier, chat_id, group_messages).await
        }
    }
}
