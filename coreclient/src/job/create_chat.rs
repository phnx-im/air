// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::Infallible;

use aircommon::{
    crypto::{aead::keys::IdentityLinkWrapperKey, indexed_aead::keys::UserProfileKey},
    identifiers::QsReference,
    time::TimeStamp,
};
use airprotos::client::group::{EncryptedGroupTitle, GroupData, GroupProfile};
use anyhow::Context;
use tracing::error;

use crate::{
    Chat, ChatAttributes, ChatId, ChatMessage, SystemMessage,
    chats::GroupDataExt,
    db_access::WriteConnection,
    groups::Group,
    job::{Job, JobContext, JobError},
    key_stores::indexed_keys::StorableIndexedKey,
};

pub(crate) struct CreateChat {
    pub chat_attributes: ChatAttributes,
    pub client_reference: QsReference,
}

type DomainError = Infallible;

impl Job for CreateChat {
    type Output = ChatId;

    type DomainError = Infallible;

    async fn execute_logic(
        self,
        context: &mut JobContext<'_, '_>,
    ) -> Result<ChatId, JobError<Self::DomainError>> {
        self.execute_internal(context).await
    }
}

impl CreateChat {
    pub(crate) fn new(chat_attributes: ChatAttributes, client_reference: QsReference) -> Self {
        Self {
            chat_attributes,
            client_reference,
        }
    }

    async fn execute_internal(
        self,
        context: &mut JobContext<'_, '_>,
    ) -> Result<ChatId, JobError<DomainError>> {
        let Self {
            chat_attributes,
            client_reference,
        } = self;
        let JobContext {
            api_clients,
            db,
            key_store,
            http_client,
            ..
        } = context;

        // First encrypt the group profile to get its size
        let identity_link_wrapper_key = IdentityLinkWrapperKey::random()
            .context("Failed to generate identity link wrapper key")?;
        let encrypted_title =
            EncryptedGroupTitle::encrypt(&chat_attributes.title, &identity_link_wrapper_key)
                .context("Failed to encrypt group title")?;

        let group_profile = GroupProfile::new(
            chat_attributes.title.clone(),
            None,
            chat_attributes
                .picture
                .as_ref()
                .map(|p| p.as_slice().into()),
        );
        let (group_profile_bytes, group_profile_builder) = group_profile
            .encrypt(&identity_link_wrapper_key)
            .context("Failed to encrypt group profile")?;

        // If we can't get a new group ID, we can't create the chat. Getting a
        // new group ID is repeatable.
        let api_client = api_clients.default_client()?;
        let (group_id, group_profile_provisioning) = api_client
            .ds_request_group_id(Some(group_profile_bytes.len()))
            .await?;

        let external_group_profile = if let Some(provisioning) = group_profile_provisioning
            && let Some(object_id) = provisioning.object_id
        {
            let mut request = http_client.put(provisioning.upload_url);
            for header in provisioning.upload_headers {
                request = request.header(header.key, header.value);
            }
            request
                .body(group_profile_bytes)
                .send()
                .await
                .context("Failed to upload group profile")?
                .error_for_status()
                .context("Failed to upload group profile")?;

            Some(group_profile_builder.build(object_id.into()))
        } else {
            error!("Unexpected group profile provisioning response");
            None
        };

        // Encode the group data to be stored in the group context
        let group_data_bytes = GroupData {
            encrypted_title: Some(encrypted_title),
            external_group_profile,
            legacy_title: None,
        }
        .encode()?;

        let own_user_id = key_store.signing_key.credential().user_id();

        // Create the group. If the query to the DS fails later on, we just
        // clean up the group, so this is repeatable.
        let (group, chat, partial_params, encrypted_user_profile_key) = db
            .write()
            .await?
            .with_transaction(async |txn| -> anyhow::Result<_> {
                let (group, partial_params) = Group::create_group(
                    &mut *txn,
                    &key_store.signing_key,
                    identity_link_wrapper_key,
                    group_id,
                    group_data_bytes,
                )?;

                let user_profile_key = UserProfileKey::load_own(&mut *txn).await?;
                let encrypted_user_profile_key =
                    user_profile_key.encrypt(group.identity_link_wrapper_key(), own_user_id)?;

                group.store(&mut *txn).await?;

                let chat = Chat::new_group_chat(partial_params.group_id.clone(), chat_attributes);
                chat.store(&mut *txn).await?;
                Ok((group, chat, partial_params, encrypted_user_profile_key))
            })
            .await?;

        let params = partial_params.into_params(client_reference, encrypted_user_profile_key);
        if let Err(e) = api_client
            .ds_create_group(params, &key_store.signing_key, group.group_state_ear_key())
            .await
        {
            db.write()
                .await?
                .with_transaction(async |txn| -> Result<_, JobError<_>> {
                    Group::delete_from_db(&mut *txn, group.group_id()).await?;
                    Chat::delete(txn, chat.id()).await?;
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
        .store(db.write().await?)
        .await?;

        Ok(chat.id())
    }
}
