// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::ClientCredential,
    crypto::{indexed_aead::keys::UserProfileKeyIndex, signatures::signable::Verifiable as _},
    identifiers::UserId,
};

use crate::db_access::{ReadConnection, WriteConnection};

use super::{
    IndexedUserProfile, UnvalidatedUserProfile, UserProfileValidationError, VerifiableUserProfile,
};

pub(crate) struct ExistingUserProfile(Option<IndexedUserProfile>);

impl ExistingUserProfile {
    pub(crate) async fn load(
        mut connection: impl ReadConnection,
        user_id: &UserId,
    ) -> sqlx::Result<Self> {
        let existing_user_profile = IndexedUserProfile::load(connection, user_id).await?;
        Ok(Self(existing_user_profile))
    }

    pub(crate) fn process_decrypted_user_profile(
        self,
        user_profile: VerifiableUserProfile,
        credential: &ClientCredential,
    ) -> Result<PersistableUserProfile, UserProfileValidationError> {
        let unvalidated_user_profile: UnvalidatedUserProfile =
            user_profile.verify(credential.verifying_key())?;
        if let Some(existing_user_profile) = &self.0 {
            if existing_user_profile.user_id != unvalidated_user_profile.user_id {
                return Err(UserProfileValidationError::MismatchingUserId {
                    expected: existing_user_profile.user_id.clone(),
                    actual: unvalidated_user_profile.user_id,
                });
            }
            if existing_user_profile.epoch >= unvalidated_user_profile.epoch {
                return Err(UserProfileValidationError::OutdatedUserProfile {
                    user_id: existing_user_profile.user_id.clone(),
                    epoch: unvalidated_user_profile.epoch,
                });
            }
        }
        let user_profile = unvalidated_user_profile.validate_display_name();

        Ok(PersistableUserProfile {
            old_profile_index: self.0.map(|profile| profile.decryption_key_index),
            user_profile,
        })
    }

    /// Check if the user profile matches the given key index. Returns false if
    /// the user profile is not present.
    pub(crate) fn matches_index(&self, user_profile_key_index: &UserProfileKeyIndex) -> bool {
        self.0
            .as_ref()
            .map(|profile| profile.decryption_key_index == *user_profile_key_index)
            .unwrap_or(false)
    }
}

pub(crate) struct PersistableUserProfile {
    old_profile_index: Option<UserProfileKeyIndex>,
    user_profile: IndexedUserProfile,
}

impl PersistableUserProfile {
    pub(crate) async fn persist(&self, mut connection: impl WriteConnection) -> sqlx::Result<()> {
        if self.is_update() {
            self.user_profile.update(connection).await
        } else {
            self.user_profile.store(connection).await
        }
    }

    pub(crate) fn old_profile_index(&self) -> Option<&UserProfileKeyIndex> {
        self.old_profile_index.as_ref()
    }

    fn is_update(&self) -> bool {
        self.old_profile_index.is_some()
    }
}
