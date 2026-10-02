// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::keys::{LeafSigningKey, SelfGroupSigningKey, UserSigningKey},
    identifiers::{QsClientId, QsUserId, UserId},
};
use anyhow::Context;
use openmls::group::GroupId;
use uuid::Uuid;

use crate::db::access::{ReadConnection, WriteConnection};

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
    /// shared user-level signing key.
    pub(crate) async fn signer_for_group(
        connection: impl ReadConnection,
        group_id: &GroupId,
        user_signer: &UserSigningKey,
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

    /// Un-assigns the self-group this client had.
    pub(crate) async fn clear_self_group(mut connection: impl WriteConnection) -> sqlx::Result<()> {
        sqlx::query!(
            "UPDATE own_client_info SET self_group_id = NULL, self_group_signing_key = NULL",
        )
        .execute(connection.as_mut())
        .await?;
        Ok(())
    }
}

#[cfg(test)]
impl OwnClientInfo {
    /// Stores a random `own_client_info` row, as linked to the self group
    /// `self_group_id` if given.
    pub(crate) async fn store_for_test(
        connection: impl crate::db::access::WriteConnection,
        self_group_id: Option<GroupId>,
    ) -> anyhow::Result<Self> {
        let info = Self {
            qs_user_id: QsUserId::random(),
            qs_client_id: QsClientId::random(&mut rand::rng()),
            user_id: UserId::random("example.com".parse()?),
            client_id: Uuid::new_v4(),
            self_group_id,
            self_group_signing_key: None,
        };
        info.store(connection).await?;
        Ok(info)
    }
}
