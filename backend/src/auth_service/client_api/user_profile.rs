// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    crypto::indexed_aead::keys::UserProfileKeyIndex, identifiers::UserId,
    messages::client_as::EncryptedUserProfile,
};
use tracing::error;

use crate::{
    auth_service::{AuthService, user_record::UserRecord},
    errors::auth_service::{GetUserProfileError, MergeUserProfileError, StageUserProfileError},
};

impl AuthService {
    pub(crate) async fn as_get_user_profile(
        &self,
        user_id: UserId,
        key_index: UserProfileKeyIndex,
    ) -> Result<EncryptedUserProfile, GetUserProfileError> {
        let user_record = UserRecord::load(&self.db_pool, &user_id)
            .await?
            .ok_or(GetUserProfileError::UserNotFound)?;

        user_record
            .into_user_profile(&key_index)
            .ok_or(GetUserProfileError::NoCiphertextFound)
    }

    pub(crate) async fn as_stage_user_profile(
        &self,
        user_id: UserId,
        user_profile: EncryptedUserProfile,
    ) -> Result<(), StageUserProfileError> {
        let mut user_record = UserRecord::load(&self.db_pool, &user_id)
            .await?
            .ok_or(StageUserProfileError::UserNotFound)?;

        user_record.stage_user_profile(user_profile);

        user_record.update(&self.db_pool).await.map_err(|e| {
            error!("Error updating user record: {:?}", e);
            StageUserProfileError::StorageError
        })?;

        Ok(())
    }

    pub(crate) async fn as_merge_user_profile(
        &self,
        user_id: UserId,
    ) -> Result<(), MergeUserProfileError> {
        let mut user_record = UserRecord::load(&self.db_pool, &user_id)
            .await?
            .ok_or(MergeUserProfileError::UserNotFound)?;

        user_record
            .merge_user_profile()
            .map_err(|_| MergeUserProfileError::NoStagedUserProfile)?;

        user_record.update(&self.db_pool).await.map_err(|e| {
            error!("Error updating user record: {:?}", e);
            MergeUserProfileError::StorageError
        })?;

        Ok(())
    }
}
