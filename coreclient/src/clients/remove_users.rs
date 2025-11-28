// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::UserId;
use remove_users_flow::RemoveUsersData;

use crate::{ChatId, ChatMessage, utils::connection_ext::StoreExt};

use super::CoreUser;

impl CoreUser {
    /// Remove users from the chat with the given [`ChatId`].
    ///
    /// Since this function causes the creation of an MLS commit, it can cause
    /// more than one effect on the group. As a result this function returns a
    /// vector of [`ChatMessage`]s that represents the changes to the
    /// group. Note that these returned message have already been persisted.
    pub(crate) async fn remove_users(
        &self,
        chat_id: ChatId,
        target_users: Vec<UserId>,
    ) -> anyhow::Result<Vec<ChatMessage>> {
        // Phase 1: Load the group and chat and prepare the commit.
        let remove = self
            .with_transaction(async |txn| {
                RemoveUsersData::stage_remove(
                    txn,
                    self.signing_key(),
                    chat_id,
                    self.user_id(),
                    target_users,
                )
                .await
            })
            .await?;

        // Phase 2: Send the commit to the DS
        let removed = remove
            .ds_group_operation(&self.inner.api_clients, self.signing_key())
            .await?;

        // Phase 3: Merge the commit into the group
        self.with_transaction_and_notifier(async |txn, notifier| {
            removed.accept(txn, notifier, chat_id).await
        })
        .await
    }
}

mod remove_users_flow {
    use aircommon::{
        credentials::keys::ClientSigningKey, identifiers::UserId,
        messages::client_ds_out::GroupOperationParamsOut, time::TimeStamp,
    };
    use anyhow::Context;
    use mimi_room_policy::RoleIndex;
    use sqlx::SqliteTransaction;

    use crate::{
        Chat, ChatId, ChatMessage,
        clients::{CoreUser, api_clients::ApiClients},
        groups::Group,
        store::StoreNotifier,
    };

    pub(super) struct RemoveUsersData {
        chat: Chat,
        group: Group,
        params: GroupOperationParamsOut,
    }

    impl RemoveUsersData {
        pub(super) async fn stage_remove(
            txn: &mut SqliteTransaction<'_>,
            signer: &ClientSigningKey,
            chat_id: ChatId,
            sender_id: &UserId,
            target_users: Vec<UserId>,
        ) -> anyhow::Result<Self> {
            let chat = Chat::load(txn.as_mut(), &chat_id)
                .await?
                .with_context(|| format!("Can't find chat with id {chat_id}"))?;
            let group_id = chat.group_id();
            let mut group = Group::load_clean(txn, group_id)
                .await?
                .with_context(|| format!("No group found for group ID {group_id:?}"))?;

            // Room policy checks
            for target in &target_users {
                group.room_state_change_role(sender_id, target, RoleIndex::Outsider)?;
            }

            let params = group
                .stage_remove(txn.as_mut(), signer, target_users)
                .await?;

            Ok(Self {
                chat,
                group,
                params,
            })
        }

        pub(super) async fn ds_group_operation(
            self,
            api_clients: &ApiClients,
            signer: &ClientSigningKey,
        ) -> anyhow::Result<RemovedUsers> {
            let Self {
                chat,
                group,
                params,
            } = self;

            let ds_timestamp = api_clients
                .get(&chat.owner_domain())?
                .ds_group_operation(params, signer, group.group_state_ear_key())
                .await?;
            Ok(RemovedUsers {
                group,
                ds_timestamp,
            })
        }
    }

    pub(super) struct RemovedUsers {
        group: Group,
        ds_timestamp: TimeStamp,
    }

    impl RemovedUsers {
        pub(super) async fn accept(
            self,
            txn: &mut sqlx::SqliteTransaction<'_>,
            notifier: &mut StoreNotifier,
            chat_id: ChatId,
        ) -> anyhow::Result<Vec<ChatMessage>> {
            let Self {
                mut group,
                ds_timestamp,
            } = self;

            let (group_messages, _) = group
                .merge_pending_commit(txn.as_mut(), None, ds_timestamp)
                .await?;
            group.store_update(txn.as_mut()).await?;
            CoreUser::store_new_messages(txn.as_mut(), notifier, chat_id, group_messages).await
        }
    }
}
