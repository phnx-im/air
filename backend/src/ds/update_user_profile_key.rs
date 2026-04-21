// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{crypto::aead::keys::EncryptedUserProfileKey, time::TimeStamp};
use mls_assist::openmls::prelude::LeafNodeIndex;
use tonic::Status;

use super::group_state::DsGroupState;

impl DsGroupState {
    pub(super) fn update_user_profile_key(
        &mut self,
        sender_index: LeafNodeIndex,
        user_profile_key: EncryptedUserProfileKey,
    ) -> Result<(), UpdateUserProfileKeyError> {
        let client_profile = self
            .member_profiles
            .get_mut(&sender_index)
            .ok_or(UpdateUserProfileKeyError::UnknownSender)?;
        client_profile.encrypted_user_profile_key = user_profile_key;
        client_profile.activity_time = TimeStamp::now();
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum UpdateUserProfileKeyError {
    #[error("Unknown sender")]
    UnknownSender,
}

impl From<UpdateUserProfileKeyError> for Status {
    fn from(e: UpdateUserProfileKeyError) -> Self {
        let msg = e.to_string();
        match e {
            UpdateUserProfileKeyError::UnknownSender => Status::invalid_argument(msg),
        }
    }
}
