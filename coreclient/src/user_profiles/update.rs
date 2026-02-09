// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::keys::ClientSigningKey,
    crypto::{indexed_aead::keys::UserProfileKeyIndex, signatures::signable::Signable},
};

use super::{
    EncryptableUserProfile, IndexedUserProfile, SignedUserProfile, UserProfile,
    UserProfileValidationError,
};

#[derive(Debug)]
pub(crate) struct UserProfileUpdate(SignedUserProfile);

impl UserProfileUpdate {
    pub(crate) fn update_own_profile(
        mut current_profile: IndexedUserProfile,
        new_user_profile: UserProfile,
        key_index: UserProfileKeyIndex,
        signing_key: &ClientSigningKey,
    ) -> Result<UserProfileUpdate, UserProfileValidationError> {
        let expected_user_id = signing_key.credential().identity();
        let profile_user_id = new_user_profile.user_id;
        if &profile_user_id != expected_user_id {
            return Err(UserProfileValidationError::MismatchingUserId {
                expected: expected_user_id.clone(),
                actual: profile_user_id,
            });
        }
        current_profile.display_name = new_user_profile.display_name;
        current_profile.profile_picture = new_user_profile.profile_picture;
        current_profile.decryption_key_index = key_index;
        current_profile.epoch += 1;
        let verifiable_user_profile = current_profile.sign(signing_key)?;
        Ok(UserProfileUpdate(verifiable_user_profile))
    }

    pub(crate) async fn store(
        self,
        executor: impl sqlx::SqliteExecutor<'_>,
        notifier: &mut crate::store::StoreNotifier,
    ) -> sqlx::Result<EncryptableUserProfile> {
        self.0.tbs.update(executor, notifier).await?;
        Ok(EncryptableUserProfile(self.0))
    }

    #[cfg(test)]
    pub(crate) fn skip_storage(self) -> EncryptableUserProfile {
        EncryptableUserProfile(self.0)
    }
}
