// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    codec::PersistenceCodec, crypto::indexed_aead::keys::UserProfileKey, identifiers::QsReference,
    time::TimeStamp,
};

use crate::{
    Chat, ChatAttributes, ChatId, ChatMessage, SystemMessage,
    groups::Group,
    job::{Job, JobContext},
    key_stores::indexed_keys::StorableIndexedKey,
    utils::connection_ext::ConnectionExt as _,
};

pub(crate) struct CreateChat {
    pub chat_attributes: ChatAttributes,
    pub client_reference: QsReference,
}

impl Job for CreateChat {
    type Output = ChatId;

    async fn execute_logic(self, context: &mut JobContext<'_>) -> anyhow::Result<ChatId> {
        self.execute_internal(context).await
    }

    async fn execute_dependencies(&mut self, _context: &mut JobContext<'_>) -> anyhow::Result<()> {
        Ok(())
    }
}

impl CreateChat {
    pub(crate) fn new(chat_attributes: ChatAttributes, client_reference: QsReference) -> Self {
        Self {
            chat_attributes,
            client_reference,
        }
    }

    async fn execute_internal(self, context: &mut JobContext<'_>) -> anyhow::Result<ChatId> {
        let Self {
            chat_attributes,
            client_reference,
        } = self;
        let JobContext {
            api_clients,
            pool,
            notifier,
            key_store,
            ..
        } = context;
        // If we can't get a new group ID, we can't create the chat. Getting a
        // new group ID is repeatable.
        let group_id = api_clients.default_client()?.ds_request_group_id().await?;
        let own_user_id = key_store.signing_key.credential().identity();

        let group_data = PersistenceCodec::to_vec(&chat_attributes)?.into();

        let mut connection = pool.acquire().await?;

        // Create the group. If the query to the DS fails later on, we just
        // clean up the group, so this is repeatable.
        let (group, chat, partial_params, encrypted_user_profile_key) = connection
            .with_transaction(async |txn| {
                let (group, group_membership, partial_params) =
                    Group::create_group(txn, &key_store.signing_key, group_id, group_data)?;

                let user_profile_key = UserProfileKey::load_own(txn.as_mut()).await?;
                let encrypted_user_profile_key =
                    user_profile_key.encrypt(group.identity_link_wrapper_key(), own_user_id)?;

                group.store(txn.as_mut()).await?;
                group_membership.store(txn.as_mut()).await?;

                let chat = Chat::new_group_chat(partial_params.group_id.clone(), chat_attributes);
                chat.store(txn.as_mut(), notifier).await?;
                Ok((group, chat, partial_params, encrypted_user_profile_key))
            })
            .await?;

        let params = partial_params.into_params(client_reference, encrypted_user_profile_key);
        if let Err(e) = api_clients
            .default_client()?
            .ds_create_group(params, &key_store.signing_key, group.group_state_ear_key())
            .await
        {
            connection
                .with_transaction(async |txn| {
                    Group::delete_from_db(txn, group.group_id()).await?;
                    Chat::delete(txn.as_mut(), notifier, chat.id()).await?;
                    Ok(())
                })
                .await?;

            return Err(e.into());
        }

        // FIXME: Use the DS timestamp here <https://github.com/phnx-im/air/issues/853>
        ChatMessage::new_system_message(
            chat.id(),
            TimeStamp::now(),
            SystemMessage::CreateGroup(own_user_id.clone()),
        )
        .store(connection.as_mut(), notifier)
        .await?;

        Ok(chat.id())
    }
}
