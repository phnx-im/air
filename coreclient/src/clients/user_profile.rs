// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airapiclient::{ApiClient, ClassifyRequestError, RequestErrorKind};
use aircommon::{
    crypto::indexed_aead::{ciphertexts::IndexEncryptable, keys::UserProfileKey},
    messages::client_ds::UserProfileKeyUpdateParams,
};
use anyhow::Context;
use tracing::warn;

use crate::{
    Chat,
    clients::own_client_info::OwnClientInfo,
    groups::{Group, handle_group_gone_on_ds},
    key_stores::indexed_keys::StorableIndexedKey,
    user_profiles::{IndexedUserProfile, UserProfile, update::UserProfileUpdate},
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
            .db()
            .with_write_transaction(async |txn| -> anyhow::Result<_> {
                let current_profile = IndexedUserProfile::load(&mut *txn, self.user_id())
                    .await?
                    .context("Failed to load own user profile")?;

                let user_profile = UserProfileUpdate::update_own_profile(
                    current_profile,
                    user_profile_content,
                    user_profile_key.index().clone(),
                    &self.inner.key_store.signing_key,
                )?
                .store(&mut *txn)
                .await?;

                user_profile_key.store_own(txn).await?;
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
        self.send_user_profile_key_to_groups(&api_client, &user_profile_key)
            .await?;

        // Phase 5: Merge the user profile on the server
        api_client
            .as_merge_user_profile(self.user_id().clone(), &self.inner.key_store.signing_key)
            .await?;

        Ok(())
    }

    /// Sends the new user profile key to the groups of all chats, except
    /// inactive and blocked ones.
    async fn send_user_profile_key_to_groups(
        &self,
        api_client: &ApiClient,
        user_profile_key: &UserProfileKey,
    ) -> anyhow::Result<()> {
        let own_user_id = self.user_id();
        let chat_ids = Chat::load_ids_for_profile_update(self.db().read().await?).await?;

        for chat_id in chat_ids {
            let group = Group::load_ref_with_chat_id(self.db().read().await?, chat_id)
                .await?
                .context("Failed to load group")?;
            let signer = OwnClientInfo::signer_for_group(
                self.db().read().await?,
                &group.group_id,
                self.signing_key(),
            )
            .await?;

            let user_profile_key =
                user_profile_key.encrypt(&group.identity_link_wrapper_key, own_user_id)?;
            let params = UserProfileKeyUpdateParams {
                group_id: group.group_id.clone(),
                sender_index: group.own_index,
                user_profile_key,
            };

            match api_client
                .ds_user_profile_key_update(params, &signer, &group.group_state_ear_key)
                .await
            {
                Ok(()) => {}
                Err(error) if error.is_not_found() => {
                    warn!(%chat_id, %error, "Group gone on DS, skipping profile key update");
                    self.db()
                        .with_write_transaction(async |txn| {
                            handle_group_gone_on_ds(txn, &group.group_id, &error).await
                        })
                        .await?;
                }
                Err(error) if matches!(error.kind(), RequestErrorKind::Rejected) => {
                    warn!(%chat_id, %error, "DS rejected profile key update, skipping group");
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
}
