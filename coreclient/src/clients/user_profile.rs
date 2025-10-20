// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    crypto::indexed_aead::{
        ciphertexts::{IndexDecryptable, IndexEncryptable},
        keys::UserProfileKey,
    },
    messages::{client_as_out::GetUserProfileResponse, client_ds::UserProfileKeyUpdateParams},
};
use anyhow::Context;
use sqlx::SqliteConnection;

use crate::{
    ChatId,
    clients::block_contact::BlockedContact,
    groups::{Group, ProfileInfo},
    key_stores::indexed_keys::StorableIndexedKey,
    store::StoreNotifier,
    user_profiles::{
        IndexedUserProfile, UserProfile, VerifiableUserProfile, process::ExistingUserProfile,
        update::UserProfileUpdate,
    },
    utils::connection_ext::StoreExt,
};

use super::CoreUser;

impl CoreUser {
    pub async fn update_user_profile(
        &self,
        user_profile_content: UserProfile,
    ) -> anyhow::Result<()> {
        let user_profile_key = UserProfileKey::random(self.user_id())?;

        // Phase 1: Store the new user profile key in the database
        let encryptable_user_profile = self
            .with_transaction_and_notifier(async |txn, notifier| {
                let current_profile = IndexedUserProfile::load(txn.as_mut(), self.user_id())
                    .await?
                    .context("Failed to load own user profile")?;

                let user_profile = UserProfileUpdate::update_own_profile(
                    current_profile,
                    user_profile_content,
                    user_profile_key.index().clone(),
                    &self.inner.key_store.signing_key,
                )?
                .store(txn.as_mut(), notifier)
                .await?;

                user_profile_key.store_own(txn.as_mut()).await?;
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
                self.user_id().clone(),
                &self.inner.key_store.signing_key,
                encrypted_user_profile,
            )
            .await?;

        // Phase 4: Send a notification to all groups
        let own_user_id = self.user_id();
        let mut connection = self.pool().acquire().await?;
        let groups_ids = Group::load_all_group_ids(&mut connection).await?;
        for group_id in groups_ids {
            let group = Group::load(&mut connection, &group_id)
                .await?
                .context("Failed to load group")?;

            let chat_id = ChatId::try_from(&group_id).context("invalid group id")?;
            if BlockedContact::check_blocked_chat(&mut *connection, chat_id).await? {
                continue; // Skip blocked chats
            }

            let own_index = group.own_index();
            let user_profile_key =
                user_profile_key.encrypt(group.identity_link_wrapper_key(), own_user_id)?;
            let params = UserProfileKeyUpdateParams {
                group_id,
                sender_index: own_index,
                user_profile_key: user_profile_key.clone(),
            };
            api_client
                .ds_user_profile_key_update(params, self.signing_key(), group.group_state_ear_key())
                .await?;
        }

        // Phase 5: Merge the user profile on the server
        api_client
            .as_merge_user_profile(self.user_id().clone(), &self.inner.key_store.signing_key)
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
        let user_id = client_credential.identity();

        // Phase 1: Check if the profile in the DB is up to date.
        let existing_user_profile = ExistingUserProfile::load(&mut *connection, user_id).await?;
        if existing_user_profile.matches_index(user_profile_key.index()) {
            return Ok(());
        }

        // Phase 2: Fetch the user profile from the server
        let api_client = self.inner.api_clients.get(user_id.domain())?;

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
        if let Some(old_user_profile_index) = persistable_user_profile.old_profile_index() {
            // Delete the old user profile key
            UserProfileKey::delete(connection, old_user_profile_index).await?;
        }

        Ok(())
    }
}
