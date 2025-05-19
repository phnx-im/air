// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use phnxtypes::messages::client_as_out::{
    GetUserProfileParams, GetUserProfileResponse, MergeUserProfileParamsTbs,
    StageUserProfileParamsTbs,
};
use tracing::error;

use crate::{
    auth_service::{AuthService, user_record::UserRecord},
    errors::auth_service::{GetUserProfileError, MergeUserProfileError, StageUserProfileError},
};

impl AuthService {
    pub(crate) async fn as_get_user_profile(
        &self,
        params: GetUserProfileParams,
    ) -> Result<GetUserProfileResponse, GetUserProfileError> {
        let GetUserProfileParams {
            user_id: client_id,
            key_index,
        } = params;

        let user_record = UserRecord::load(&self.db_pool, &client_id)
            .await?
            .ok_or(GetUserProfileError::UserNotFound)?;

        let user_profile = user_record
            .into_user_profile(&key_index)
            .ok_or(GetUserProfileError::NoCiphertextFound)?;

        let response = GetUserProfileResponse {
            encrypted_user_profile: user_profile,
        };

        Ok(response)
    }

    pub(crate) async fn as_stage_user_profile(
        &self,
        params: StageUserProfileParamsTbs,
    ) -> Result<(), StageUserProfileError> {
        let StageUserProfileParamsTbs {
            user_id: client_id,
            user_profile,
        } = params;

        let mut user_record = UserRecord::load(&self.db_pool, &client_id)
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
        params: MergeUserProfileParamsTbs,
    ) -> Result<(), MergeUserProfileError> {
        let MergeUserProfileParamsTbs { user_id: client_id } = params;

        let mut user_record = UserRecord::load(&self.db_pool, &client_id)
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
