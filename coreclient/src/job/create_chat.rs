// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::Infallible;

use aircommon::{
    credentials::keys::LeafSigningKey,
    crypto::{aead::keys::IdentityLinkWrapperKey, indexed_aead::keys::UserProfileKey},
    identifiers::QsReference,
    time::TimeStamp,
};
use airprotos::client::{
    group::{EncryptedGroupTitle, GroupData, GroupProfile},
    group_bootstrap::GroupBootstrapCarrier,
};
use anyhow::Context;
use tracing::{error, info};

use crate::{
    Chat, ChatAttributes, ChatId,
    db::access::WriteConnection,
    groups::{
        Group, NewGroupContext, pending_creation::PendingGroupCreation, self_group::SelfGroup,
    },
    job::{
        Job, JobContext, JobError,
        recoverable::{Recoverable, RecoverableCause},
    },
    key_stores::indexed_keys::StorableIndexedKey,
};

pub(crate) struct CreateChat {
    pub chat_attributes: ChatAttributes,
    pub client_reference: QsReference,
    pub is_apq: bool,
    /// Store the group profile in the group profile component instead of the
    /// group data extension.
    pub profile_component: bool,
}

type DomainError = Infallible;

impl Job for CreateChat {
    type Output = ChatId;

    type DomainError = Infallible;

    async fn execute_logic(
        self,
        context: &mut JobContext<'_, '_>,
    ) -> Result<ChatId, JobError<Self::DomainError>> {
        Box::pin(self.execute_internal(context)).await
    }
}

impl CreateChat {
    pub(crate) fn new(
        chat_attributes: ChatAttributes,
        client_reference: QsReference,
        is_apq: bool,
    ) -> Self {
        Self {
            chat_attributes,
            client_reference,
            is_apq,
            profile_component: false,
        }
    }

    #[cfg(any(test, feature = "test_utils"))]
    pub(crate) fn with_profile_component(mut self) -> Self {
        self.profile_component = true;
        self
    }

    async fn execute_internal(
        self,
        context: &mut JobContext<'_, '_>,
    ) -> Result<ChatId, JobError<DomainError>> {
        let Self {
            chat_attributes,
            client_reference,
            is_apq,
            profile_component,
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
        let (group_id, pq_group_id, group_profile_provisioning) = api_client
            .ds_request_group_id(Some(group_profile_bytes.len()), is_apq)
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
        let group_data = GroupData {
            encrypted_title: Some(encrypted_title),
            external_group_profile,
        };
        let context = if profile_component {
            NewGroupContext::Chat(group_data)
        } else {
            NewGroupContext::LegacyChat(group_data)
        };

        let own_user_id = key_store.signing_key.credential().user_id();

        // Create the group, pending until the DS confirms it. The chat only
        // appears on confirmation.
        let (group, partial_params, encrypted_user_profile_key, group_bootstrap) = db
            .write()
            .await?
            .with_transaction(async |txn| -> anyhow::Result<_> {
                let self_group = SelfGroup::load(&mut *txn)
                    .await?
                    .filter(SelfGroup::has_linked_devices);
                let vc_group_id = self_group.as_ref().map(|group| group.group_id());

                let (group, partial_params) = if is_apq {
                    Group::create_apq_group(
                        &mut *txn,
                        &LeafSigningKey::User(key_store.signing_key.clone()),
                        own_user_id.clone(),
                        identity_link_wrapper_key,
                        group_id,
                        pq_group_id.context("Missing PQ group ID")?,
                        context,
                        vc_group_id,
                        None,
                    )?
                } else {
                    Group::create_group(
                        &mut *txn,
                        &key_store.signing_key,
                        identity_link_wrapper_key,
                        group_id,
                        context,
                        vc_group_id,
                    )?
                };

                let user_profile_key = UserProfileKey::load_own(&mut *txn).await?;
                let encrypted_user_profile_key =
                    user_profile_key.encrypt(group.identity_link_wrapper_key(), own_user_id)?;

                group.store(&mut *txn).await?;
                PendingGroupCreation::new(partial_params.group_id.clone(), chat_attributes)
                    .store(&mut *txn)
                    .await?;

                // Group chats carry no connection context. The sibling takes
                // the chat attributes from the group data extension.
                let connection = None;
                let group_bootstrap = match &self_group {
                    Some(self_group) => Some(self_group.seal_group_bootstrap_param(
                        txn,
                        &group,
                        GroupBootstrapCarrier::CreationEcho,
                        connection,
                    )?),
                    None => None,
                };

                Ok((
                    group,
                    partial_params,
                    encrypted_user_profile_key,
                    group_bootstrap,
                ))
            })
            .await?;

        let mut params = partial_params.into_params(client_reference, encrypted_user_profile_key);
        params.group_bootstrap = group_bootstrap;
        let group_id = group.group_id();
        if let Err(error) = api_client
            .ds_create_group(params, &key_store.signing_key, group.group_state_ear_key())
            .await
        {
            let error = JobError::from(error);
            if outcome_is_uncertain(&error) {
                // The DS may have created the group. Its echo confirms it, or
                // a later queue drain without the echo discards it.
                info!(?group_id, %error, "Keeping the group creation pending");
            } else {
                db.write()
                    .await?
                    .with_transaction(async |txn| {
                        PendingGroupCreation::discard(txn, group_id).await
                    })
                    .await?;
            }
            return Err(error);
        }

        // The echo may have confirmed the creation already
        db.write()
            .await?
            .with_transaction(async |txn| -> Result<_, JobError<_>> {
                // FIXME: Use the DS timestamp here <https://github.com/phnx-im/air/issues/853>
                if PendingGroupCreation::confirm(txn, group_id, own_user_id, TimeStamp::now())
                    .await?
                    .is_none()
                    && Chat::load_by_group_id(&mut *txn, group_id).await?.is_none()
                {
                    return Err(JobError::fatal(anyhow::anyhow!(
                        "the group was discarded before the DS confirmed its creation"
                    )));
                }
                Ok(())
            })
            .await?;

        Ok(ChatId::try_from(group_id)?)
    }
}

/// Whether the DS may have created the group despite the error.
fn outcome_is_uncertain<E>(error: &JobError<E>) -> bool {
    matches!(
        error,
        JobError::NetworkError
            | JobError::Recoverable(Recoverable {
                cause: RecoverableCause::Server,
                ..
            })
    )
}
