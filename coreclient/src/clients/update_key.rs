// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{codec::PersistenceCodec, identifiers::UserId, time::TimeStamp};
use sqlx::SqliteConnection;
use update_key_flow::UpdateKeyData;

use crate::{
    Chat, ChatAttributes, ChatId, ChatMessage, SystemMessage,
    chats::messages::TimestampedMessage,
    groups::GroupData,
    store::StoreNotifier,
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
    pub(crate) async fn update_key(
        &self,
        chat_id: ChatId,
        new_chat_attributes: impl Into<Option<&ChatAttributes>>,
    ) -> anyhow::Result<Vec<ChatMessage>> {
        // Phase 1: Load the chat and the group
        let mut connection = self.pool().acquire().await?;
        let new_chat_attributes = new_chat_attributes.into();
        let update = connection
            .with_transaction(async |txn| {
                UpdateKeyData::lock(txn, chat_id, self.signing_key(), new_chat_attributes).await
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

pub(crate) async fn update_chat_attributes(
    connection: &mut SqliteConnection,
    notifier: &mut StoreNotifier,
    chat: &mut Chat,
    sender_id: UserId,
    group_data: GroupData,
    ds_timestamp: TimeStamp,
) -> anyhow::Result<Vec<TimestampedMessage>> {
    let mut group_messages = Vec::new();
    let new_chat_attributes: ChatAttributes = PersistenceCodec::from_slice(group_data.bytes())?;
    let new_title = new_chat_attributes.title;
    let old_title = chat.attributes.title.clone();
    if chat.attributes.title != new_title {
        chat.set_title(&mut *connection, notifier, new_title.clone())
            .await?;
        let system_message = SystemMessage::ChangeTitle {
            user_id: sender_id.clone(),
            old_title,
            new_title,
        };
        let group_message = TimestampedMessage::system_message(system_message, ds_timestamp);
        group_messages.push(group_message);
    }
    if chat.attributes.picture != new_chat_attributes.picture {
        chat.set_picture(connection, notifier, new_chat_attributes.picture)
            .await?;
        let system_message = SystemMessage::ChangePicture(sender_id);
        let group_message = TimestampedMessage::system_message(system_message, ds_timestamp);
        group_messages.push(group_message);
    }

    Ok(group_messages)
}

mod update_key_flow {
    use aircommon::{
        codec::PersistenceCodec, credentials::keys::ClientSigningKey, identifiers::UserId,
        messages::client_ds_out::UpdateParamsOut, time::TimeStamp,
    };
    use anyhow::Context;
    use sqlx::SqliteTransaction;

    use crate::{
        Chat, ChatAttributes, ChatId, ChatMessage,
        clients::{CoreUser, api_clients::ApiClients, update_key::update_chat_attributes},
        groups::{Group, GroupData},
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
            new_chat_attributes: Option<&ChatAttributes>,
        ) -> anyhow::Result<Self> {
            let chat = Chat::load(txn.as_mut(), &chat_id)
                .await?
                .with_context(|| format!("Can't find chat with id {chat_id}"))?;
            let group_id = chat.group_id();
            let mut group = Group::load_clean(txn, group_id)
                .await?
                .with_context(|| format!("Can't find group with id {group_id:?}"))?;
            let group_data = match new_chat_attributes {
                Some(attrs) => Some(GroupData::from(PersistenceCodec::to_vec(attrs)?)),
                None => None,
            };
            let params = group.update(txn, signer, group_data).await?;
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
                chat,
                ds_timestamp,
                own_id: signer.credential().identity().clone(),
            })
        }
    }

    pub(super) struct UpdatedKey {
        group: Group,
        chat: Chat,
        ds_timestamp: TimeStamp,
        own_id: UserId,
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
                mut chat,
                ds_timestamp,
                own_id,
            } = self;
            let (mut group_messages, group_data) = group
                .merge_pending_commit(&mut *connection, None, ds_timestamp)
                .await?;

            if let Some(group_data) = group_data {
                let attribute_messages = update_chat_attributes(
                    connection,
                    notifier,
                    &mut chat,
                    own_id,
                    group_data,
                    ds_timestamp,
                )
                .await?;
                group_messages.extend(attribute_messages);
            }

            group.store_update(&mut *connection).await?;
            CoreUser::store_new_messages(&mut *connection, notifier, chat_id, group_messages).await
        }
    }
}
