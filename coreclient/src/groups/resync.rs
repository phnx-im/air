// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::keys::ClientSigningKey,
    crypto::ear::keys::{GroupStateEarKey, IdentityLinkWrapperKey},
    messages::{client_ds::AadPayload, client_ds_out::ExternalCommitInfoIn},
};
use anyhow::Result;
use openmls::{group::GroupId, prelude::MlsMessageOut};
use sqlx::{Connection, SqliteConnection};

use crate::{clients::api_clients::ApiClients, groups::Group};

/// All the information required to resync into a group.
#[derive(Debug, Clone)]
pub(crate) struct ResyncInfo {
    pub(crate) group_id: GroupId,
    pub(crate) group_state_ear_key: GroupStateEarKey,
    pub(crate) identity_link_wrapper_key: IdentityLinkWrapperKey,
}

impl Group {
    /// Turn this group into a `ResyncInfo` struct and delete the original group
    /// from the DB.
    pub(crate) async fn prepare_for_resync(
        self,
        connection: &mut SqliteConnection,
    ) -> Result<ResyncInfo> {
        let resync_info = ResyncInfo {
            group_id: self.group_id,
            group_state_ear_key: self.group_state_ear_key,
            identity_link_wrapper_key: self.identity_link_wrapper_key,
        };

        // Delete the group from the DB.
        let mut txn = connection.begin().await?;
        Self::delete_from_db(&mut txn, &resync_info.group_id).await?;
        txn.commit().await?;

        Ok(resync_info)
    }
}

impl ResyncInfo {
    /// Resync using an external commit.
    pub(crate) async fn resync(
        self,
        connection: &mut SqliteConnection,
        api_clients: &ApiClients,
        external_commit_info: ExternalCommitInfoIn,
        signer: &ClientSigningKey,
    ) -> Result<(Group, MlsMessageOut, MlsMessageOut)> {
        let mut txn = connection.begin_with("BEGIN IMMEDIATE").await?;

        let aad = AadPayload::Resync.into();
        let (new_group, commit, group_info, _member_profile_info) = Group::join_group_externally(
            &mut txn,
            api_clients,
            external_commit_info,
            signer,
            self.group_state_ear_key,
            self.identity_link_wrapper_key,
            aad,
            None, // This is not in response to a connection offer.
        )
        .await?;

        new_group.store(&mut *txn).await?;

        txn.commit().await?;

        Ok((new_group, commit, group_info))
    }
}
