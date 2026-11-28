// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Group creation on the DS. APQ groups are created on a group creation
//! stream, classical groups with separate id reservation and creation
//! requests.

use airapiclient::{
    ApiClient,
    ds_api::{DsGroupCreator, DsGroupReservation, DsRequestError, ProvisionAttachmentResponse},
};
use aircommon::{
    credentials::keys::UserSigningKey, crypto::aead::keys::GroupStateEarKey,
    messages::client_ds_out::CreateGroupParamsOut,
};
use openmls::group::GroupId;

/// The ids and group profile upload the DS reserved for a new group.
#[derive(Debug)]
pub(crate) struct ReservedGroup {
    pub(crate) group_id: GroupId,
    /// Set for APQ groups.
    pub(crate) pq_group_id: Option<GroupId>,
    /// Set when a group profile upload was requested and the server has
    /// storage configured.
    pub(crate) group_profile_provisioning: Option<ProvisionAttachmentResponse>,
}

/// Creates a group with the ids of its [`ReservedGroup`].
#[derive(Debug)]
pub(crate) enum GroupCreator {
    /// The open creation stream of an APQ group. Dropping it closes the
    /// stream without creating the group.
    Stream(Box<DsGroupCreator>),
    /// Classical groups are created with a separate request.
    Request,
}

impl GroupCreator {
    /// Creates the group on the DS. `params` must use the ids of the
    /// reservation, and `signing_key` must be the key the reservation was
    /// made with.
    pub(crate) async fn create(
        self,
        api_client: &ApiClient,
        params: CreateGroupParamsOut,
        signing_key: &UserSigningKey,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<(), DsRequestError> {
        match self {
            Self::Stream(creator) => creator.create(params, group_state_ear_key).await,
            Self::Request => {
                api_client
                    .ds_create_group(params, signing_key, group_state_ear_key)
                    .await
            }
        }
    }
}

/// Reserves the ids of a new group, and optionally the upload of a group
/// profile of `group_profile_size` bytes. The group must be created before the
/// server's reservation deadline.
pub(crate) async fn reserve_group(
    api_client: &ApiClient,
    signing_key: &UserSigningKey,
    group_profile_size: Option<usize>,
    is_apq: bool,
) -> Result<(ReservedGroup, GroupCreator), DsRequestError> {
    if is_apq {
        let (reservation, creator) = api_client
            .ds_prepare_group_creation(signing_key, group_profile_size)
            .await?;
        let DsGroupReservation {
            group_id,
            pq_group_id,
            group_profile_provisioning,
        } = reservation;
        let reserved = ReservedGroup {
            group_id,
            pq_group_id: Some(pq_group_id),
            group_profile_provisioning,
        };
        Ok((reserved, GroupCreator::Stream(Box::new(creator))))
    } else {
        let request_pq_group_id = false;
        let (group_id, _, group_profile_provisioning) = api_client
            .ds_request_group_id(group_profile_size, request_pq_group_id)
            .await?;
        let reserved = ReservedGroup {
            group_id,
            pq_group_id: None,
            group_profile_provisioning,
        };
        Ok((reserved, GroupCreator::Request))
    }
}
