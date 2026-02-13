// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::ClientCredential,
    crypto::indexed_aead::{ciphertexts::IndexDecryptable, keys::UserProfileKey},
    messages::client_as_out::GetUserProfileResponse,
};
use serde::{Deserialize, Serialize};
use sqlx::SqliteExecutor;
use tls_codec::Serialize as _;
use tracing::{debug, error};

use crate::{
    clients::CoreUser,
    groups::ProfileInfo,
    job::operation::OperationId,
    key_stores::indexed_keys::StorableIndexedKey,
    user_profiles::{VerifiableUserProfile, process::ExistingUserProfile},
    utils::connection_ext::ConnectionExt,
};

use super::{
    Job, JobContext, JobError,
    operation::{Operation, OperationData, OperationKind},
};

impl CoreUser {
    /// Schedule a profile fetch operation
    ///
    /// This will be executed on the next run of the outbound service.
    pub(crate) async fn schedule_fetch_profile(
        executor: impl SqliteExecutor<'_>,
        profile_info: impl Into<ProfileInfo>,
    ) -> sqlx::Result<()> {
        let ProfileInfo {
            client_credential,
            user_profile_key,
        } = profile_info.into();
        let user_id = client_credential.identity();
        debug!(?user_id, "##### scheduling fetch profile");
        FetchProfileOperation::new(client_credential, user_profile_key)
            .enqueue(executor)
            .await
    }

    /// Immediately fetch profile from the server
    ///
    /// This will do a network request.
    pub(crate) async fn fetch_profile(
        &self,
        profile_info: impl Into<ProfileInfo>,
    ) -> anyhow::Result<()> {
        let ProfileInfo {
            client_credential,
            user_profile_key,
        } = profile_info.into();
        let job = FetchProfileOperation::new(client_credential, user_profile_key);
        self.execute_job(job).await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FetchProfileOperation {
    client_credential: ClientCredential,
    user_profile_key: UserProfileKey,
}

impl OperationData for FetchProfileOperation {
    fn kind() -> OperationKind {
        OperationKind::FetchProfile
    }

    fn generate_id(&self) -> OperationId {
        let mut bytes = Vec::new();
        bytes.push(Self::kind() as u8);
        let user_id = self.client_credential.identity();
        if let Err(error) = user_id.tls_serialize(&mut bytes) {
            error!(%error, "error white serializing user id");
        }
        OperationId(bytes)
    }
}

impl FetchProfileOperation {
    pub(crate) fn new(
        client_credential: ClientCredential,
        user_profile_key: UserProfileKey,
    ) -> Self {
        Self {
            client_credential,
            user_profile_key,
        }
    }

    pub(crate) async fn enqueue<'a>(self, executor: impl SqliteExecutor<'a>) -> sqlx::Result<()> {
        Operation::new(self).enqueue(executor).await
    }
}

impl Job for FetchProfileOperation {
    type Output = ();

    async fn execute_logic(self, context: &mut JobContext<'_>) -> Result<Self::Output, JobError> {
        let Self {
            client_credential,
            user_profile_key,
        } = self;

        let user_id = client_credential.identity();

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
            .with_transaction(async |txn| {
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
