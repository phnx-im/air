// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Fetching operations for user and group profiles.

use std::convert::Infallible;

use aircommon::{
    credentials::ClientCredential,
    crypto::indexed_aead::{ciphertexts::IndexDecryptable, keys::UserProfileKey},
    identifiers::{AttachmentId, UserId},
    messages::client_as_out::GetUserProfileResponse,
    time::TimeStamp,
};
use airprotos::{
    client::group::{ExternalGroupProfile, GroupProfile},
    delivery_service::v1::StorageObjectType,
};
use anyhow::Context;
use openmls::group::GroupId;
use serde::{Deserialize, Serialize};
use sqlx::SqliteExecutor;
use tls_codec::Serialize as _;
use tracing::{debug, error, info};

use crate::{
    Chat, ChatAttributes, ChatStatus,
    clients::{CoreUser, update_key::update_chat_attributes},
    groups::{Group, ProfileInfo},
    job::operation::OperationId,
    key_stores::indexed_keys::StorableIndexedKey,
    user_profiles::{VerifiableUserProfile, process::ExistingUserProfile},
    utils::connection_ext::ConnectionExt,
};

use super::{
    Job, JobContext, JobError,
    operation::{OperationData, OperationKind},
};

impl CoreUser {
    /// Schedule a user profile fetch operation.
    ///
    /// This will be executed on the next run of the outbound service.
    pub(crate) async fn schedule_fetch_user_profile(
        executor: impl SqliteExecutor<'_>,
        profile_info: impl Into<ProfileInfo>,
    ) -> sqlx::Result<()> {
        let ProfileInfo {
            client_credential,
            user_profile_key,
        } = profile_info.into();
        FetchUserProfileOperation::new(client_credential, user_profile_key)
            .into_operation()
            .enqueue(executor)
            .await
    }

    /// Immediately fetch user profile from the server.
    ///
    /// This will do a network request.
    pub(crate) async fn fetch_user_profile(
        &self,
        profile_info: impl Into<ProfileInfo>,
    ) -> anyhow::Result<()> {
        let ProfileInfo {
            client_credential,
            user_profile_key,
        } = profile_info.into();
        let job = FetchUserProfileOperation::new(client_credential, user_profile_key);
        Ok(self.execute_job(job).await?)
    }

    /// Schedule a group profile fetch operation.
    ///
    /// This will be executed on the next run of the outbound service.
    pub(crate) async fn schedule_fetch_group_profile(
        executor: impl SqliteExecutor<'_>,
        group_id: GroupId,
        sender_id: UserId,
        uploaded_at: TimeStamp,
        external_group_profile: ExternalGroupProfile,
    ) -> sqlx::Result<()> {
        FetchGroupProfileOperation {
            group_id,
            sender_id,
            uploaded_at,
            external_group_profile,
        }
        .into_operation()
        .enqueue(executor)
        .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FetchUserProfileOperation {
    client_credential: ClientCredential,
    user_profile_key: UserProfileKey,
}

impl FetchUserProfileOperation {
    pub(crate) fn new(
        client_credential: ClientCredential,
        user_profile_key: UserProfileKey,
    ) -> Self {
        Self {
            client_credential,
            user_profile_key,
        }
    }
}

impl OperationData for FetchUserProfileOperation {
    fn kind() -> OperationKind {
        OperationKind::FetchUserProfile
    }

    fn generate_id(&self) -> OperationId {
        let mut bytes = Vec::new();
        bytes.push(Self::kind() as u8);
        let user_id = self.client_credential.user_id();
        if let Err(error) = user_id.tls_serialize(&mut bytes) {
            error!(%error, "error white serializing user id");
        }
        OperationId(bytes)
    }
}

impl Job for FetchUserProfileOperation {
    type Output = ();

    type DomainError = Infallible;

    async fn execute_logic(
        self,
        context: &mut JobContext<'_>,
    ) -> Result<Self::Output, JobError<Self::DomainError>> {
        let Self {
            client_credential,
            user_profile_key,
        } = self;

        let user_id = client_credential.user_id();

        // Phase 1: Check if the profile in the DB is up to date.
        let existing_user_profile = ExistingUserProfile::load(&context.pool, user_id).await?;
        if existing_user_profile.matches_index(user_profile_key.index()) {
            return Ok(());
        }

        // Phase 2: Fetch the user profile from the server
        let api_client = context.api_clients.get(user_id.domain())?;
        let GetUserProfileResponse {
            encrypted_user_profile,
        } = api_client
            .as_get_user_profile(user_id.clone(), user_profile_key.index().clone())
            .await?;

        // Phase 3: Decrypt and process the user profile
        let verifiable_user_profile =
            VerifiableUserProfile::decrypt_with_index(&user_profile_key, &encrypted_user_profile)
                .map_err(JobError::fatal)?;
        let persistable_user_profile = existing_user_profile
            .process_decrypted_user_profile(verifiable_user_profile, &client_credential)
            .map_err(JobError::fatal)?;

        // Phase 4: Store the user profile and key in the database
        context
            .pool
            .with_transaction(async |txn| -> sqlx::Result<_> {
                user_profile_key.store(txn.as_mut()).await?;
                persistable_user_profile
                    .persist(txn.as_mut(), context.notifier)
                    .await?;
                if let Some(old_user_profile_index) = persistable_user_profile.old_profile_index() {
                    // Delete the old user profile key
                    UserProfileKey::delete(txn.as_mut(), old_user_profile_index).await?;
                }
                Ok(())
            })
            .await?;

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FetchGroupProfileOperation {
    group_id: GroupId,
    sender_id: UserId,
    uploaded_at: TimeStamp,
    external_group_profile: ExternalGroupProfile,
}

impl OperationData for FetchGroupProfileOperation {
    fn kind() -> OperationKind {
        OperationKind::FetchGroupProfile
    }

    fn generate_id(&self) -> OperationId {
        let mut bytes = Vec::new();
        bytes.push(Self::kind() as u8);
        bytes.extend(self.group_id.as_slice());
        OperationId(bytes)
    }
}

impl Job for FetchGroupProfileOperation {
    type Output = ();

    type DomainError = Infallible;

    async fn execute_logic(
        self,
        context: &mut JobContext<'_>,
    ) -> Result<Self::Output, JobError<Self::DomainError>> {
        let Self {
            group_id,
            sender_id,
            uploaded_at,
            external_group_profile,
        } = self;

        info!(
            ?group_id,
            object_id = %external_group_profile.object_id,
            ?uploaded_at,
            "Fetching group profile"
        );

        // Load chat and group
        let Some((mut chat, group)) = context
            .pool
            .with_transaction(async |txn| -> anyhow::Result<_> {
                let chat = Chat::load_by_group_id(txn.as_mut(), &group_id)
                    .await?
                    .context("Missing chat")?;
                if let ChatStatus::Blocked = chat.status() {
                    return Ok(None);
                }
                let group = Group::load_verified(txn.as_mut(), &group_id)
                    .await?
                    .context("Missing group")?;
                Ok(Some((chat, group)))
            })
            .await?
        else {
            return Ok(()); // blocked chat
        };

        // Fetch group profile from the object storage
        let api_client = context.api_clients.get(&chat.owner_domain())?;
        let attachment_id = AttachmentId::new(external_group_profile.object_id);
        let url = api_client
            .ds_get_attachment_url(
                &context.key_store.signing_key,
                group.group_state_ear_key(),
                &group_id,
                group.own_index(),
                attachment_id,
                StorageObjectType::GroupProfile,
            )
            .await?;
        let bytes = context
            .http_client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;

        // Decrypt and validate group profile
        let group_profile = GroupProfile::decrypt(
            group.identity_link_wrapper_key(),
            &external_group_profile,
            bytes.into(),
        )
        .map_err(JobError::fatal)?;

        debug!(
            ?group_id,
            ?external_group_profile,
            "Fetched and decrypted group profile"
        );

        // Update chat attributes and store new messages
        context
            .pool
            .with_transaction(async |txn| -> anyhow::Result<_> {
                let mut messages = Vec::new();

                let chat_attributes = ChatAttributes::new(
                    group_profile.title,
                    group_profile.picture.map(|p| p.into()),
                );
                update_chat_attributes(
                    txn.as_mut(),
                    context.notifier,
                    &mut chat,
                    sender_id,
                    chat_attributes,
                    uploaded_at,
                    &mut messages,
                )
                .await?;

                CoreUser::store_new_messages(txn, context.notifier, chat.id(), messages).await?;

                debug!(?group_id, chat_id = %chat.id(), "Updated chat attributes");

                Ok(())
            })
            .await?;

        Ok(())
    }
}
