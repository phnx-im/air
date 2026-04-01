// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! This module contains the implementation of the delivery service.

use mimi_room_policy::VerifiedRoomState;
use mls_assist::{
    MlsAssistRustCrypto,
    openmls::{prelude::group_info::GroupInfo, treesync::RatchetTree},
};
use uuid::Uuid;

use aircommon::{
    codec::PersistenceCodec, crypto::ear::keys::EncryptedUserProfileKey,
    identifiers::QualifiedGroupId,
};

use super::Ds;

pub const USER_EXPIRATION_DAYS: i64 = 90;
pub(super) type Provider = MlsAssistRustCrypto<PersistenceCodec>;

impl Ds {
    pub(crate) async fn request_group_id(&self) -> QualifiedGroupId {
        // Generate UUIDs until we find one that is not yet reserved.
        let mut group_uuid = Uuid::new_v4();
        while !self.reserve_group_id(group_uuid).await {
            group_uuid = Uuid::new_v4();
        }
        QualifiedGroupId::new(group_uuid, self.own_domain.clone())
    }
}

#[derive(Debug)]
pub struct ExternalCommitInfo {
    pub group_info: GroupInfo,
    pub ratchet_tree: RatchetTree,
    pub encrypted_user_profile_keys: Vec<EncryptedUserProfileKey>,
    pub room_state: VerifiedRoomState,
    // Proposals that are valid in external commits
    pub proposals: Vec<Vec<u8>>,
}
