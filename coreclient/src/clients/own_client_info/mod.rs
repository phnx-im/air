// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::keys::{ClientSigningKey, LeafSigningKey, SelfGroupSigningKey},
    identifiers::{QsClientId, QsUserId, UserId},
};
use anyhow::Context;
use openmls::group::GroupId;
use uuid::Uuid;

use crate::db::access::ReadConnection;

mod persistence;

/// The purpose of this struct is to be stored in the local DB for use as
/// reference for other tables.
#[derive(Debug, Clone)]
pub(crate) struct OwnClientInfo {
    pub(crate) qs_user_id: QsUserId,
    pub(crate) qs_client_id: QsClientId,
    pub(crate) user_id: UserId,
    /// Identifies this client (device), e.g. in self-group leaf credentials. Unlike `user_id`, it
    /// is unique per client: each linked device mints its own.
    pub(crate) client_id: Uuid,
    pub(crate) self_group_id: Option<GroupId>,
    pub(crate) self_group_signing_key: Option<SelfGroupSigningKey>,
}

impl OwnClientInfo {
    /// The signing key for the local client's leaf in `group_id`.
    ///
    /// The self-group leaf is signed with the per-device self-group key. All other groups use the
    /// shared user-level client signing key.
    pub(crate) async fn signer_for_group(
        connection: impl ReadConnection,
        group_id: &GroupId,
        user_signer: &ClientSigningKey,
    ) -> anyhow::Result<LeafSigningKey> {
        let info = Self::load(connection).await?;
        if info.self_group_id.as_ref() == Some(group_id) {
            let signing_key = info
                .self_group_signing_key
                .context("self-group signer was not initialized")?;
            Ok(LeafSigningKey::SelfGroup(signing_key))
        } else {
            Ok(LeafSigningKey::User(user_signer.clone()))
        }
    }
}
