// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::Context;
use phnxtypes::{
    crypto::indexed_aead::{
        ciphertexts::{IndexDecryptable, IndexEncryptable},
        keys::UserProfileKey,
    },
    messages::{client_as_out::GetUserProfileResponse, client_ds::UserProfileKeyUpdateParams},
};
use sqlx::SqliteConnection;

use crate::{
    Contact,
    groups::{Group, ProfileInfo},
    key_stores::indexed_keys::StorableIndexedKey,
    store::StoreNotifier,
    user_profiles::{
        IndexedUserProfile, UserProfile, VerifiableUserProfile, process::ExistingUserProfile,
        update::UserProfileUpdate,
    },
};

use super::CoreUser;

impl CoreUser {
    pub async fn update_user_profile(
        &self,
        user_profile_content: UserProfile,
    ) -> anyhow::Result<()> {
        let user_profile_key = UserProfileKey::random(self.user_name())?;

        // Phase 1: Store the new user profile key in the database
        let encryptable_user_profile = self
            .with_transaction_and_notifier(async |txn, notifier| {
                let current_profile = IndexedUserProfile::load(&mut **txn, self.user_name())
                    .await?
                    .context("Failed to load own user profile")?;

                let user_profile = UserProfileUpdate::update_own_profile(
                    current_profile,
                    user_profile_content,
                    user_profile_key.index().clone(),
                    &self.inner.key_store.signing_key,
                )?
                .store(&mut **txn, notifier)
                .await?;

                user_profile_key.store_own(&mut *txn).await?;
                Ok(user_profile)
            })
            .await?;

        // Phase 2: Encrypt the user profile
        let encrypted_user_profile =
            encryptable_user_profile.encrypt_with_index(&user_profile_key)?;

        // Phase 3: Stage the updated profile on the server
        let api_client = self.inner.api_clients.default_client()?;

        api_client
            .as_stage_user_profile(
                self.as_client_id(),
                &self.inner.key_store.signing_key,
                encrypted_user_profile,
            )
            .await?;

        // Phase 4: Send a notification to all groups
        let own_user_name = self.user_name();
        let mut connection = self.pool().acquire().await?;
        let groups_ids = Group::load_all_group_ids(&mut connection).await?;
        for group_id in groups_ids {
            let group = Group::load(&mut connection, &group_id)
                .await?
                .context("Failed to load group")?;
            let own_index = group.own_index();
            let user_profile_key =
                user_profile_key.encrypt(group.identity_link_wrapper_key(), own_user_name)?;
            let params = UserProfileKeyUpdateParams {
                group_id,
                sender_index: own_index,
                user_profile_key: user_profile_key.clone(),
            };
            api_client
                .ds_user_profile_key_update(
                    params,
                    group.leaf_signer(),
                    group.group_state_ear_key(),
                )
                .await?;
        }

        // Phase 5: Merge the user profile on the server
        api_client
            .as_merge_user_profile(self.as_client_id(), &self.inner.key_store.signing_key)
            .await?;

        Ok(())
    }

    pub(crate) async fn fetch_and_store_user_profile(
        &self,
        connection: &mut SqliteConnection,
        notifier: &mut StoreNotifier,
        profile_info: impl Into<ProfileInfo>,
    ) -> anyhow::Result<()> {
        let ProfileInfo {
            user_profile_key,
            client_credential,
        } = profile_info.into();
        let user_name = client_credential.identity().user_name().clone();

        // Phase 1: Check if the profile in the DB is up to date.
        let existing_user_profile = ExistingUserProfile::load(&mut *connection, &user_name).await?;
        if existing_user_profile.matches_index(user_profile_key.index()) {
            return Ok(());
        }

        // Phase 2: Fetch the user profile from the server
        let api_client = self.inner.api_clients.get(user_name.domain())?;

        // TODO: Avoid network calls while in transaction
        let GetUserProfileResponse {
            encrypted_user_profile,
        } = api_client
            .as_get_user_profile(
                client_credential.identity().clone(),
                user_profile_key.index().clone(),
            )
            .await?;

        let verifiable_user_profile =
            VerifiableUserProfile::decrypt_with_index(&user_profile_key, &encrypted_user_profile)?;
        let persistable_user_profile = existing_user_profile
            .process_decrypted_user_profile(verifiable_user_profile, &client_credential)?;

        // Phase 3: Store the user profile and key in the database
        user_profile_key.store(&mut *connection).await?;
        persistable_user_profile
            .persist(&mut *connection, notifier)
            .await?;
        Contact::update_user_profile_key_index(
            &mut *connection,
            &user_name,
            user_profile_key.index(),
        )
        .await?;
        if let Some(old_user_profile_index) = persistable_user_profile.old_profile_index() {
            // Delete the old user profile key
            UserProfileKey::delete(connection, old_user_profile_index).await?;
        }

        Ok(())
    }
}
