// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::keys::ClientSigningKey,
    crypto::{ear::keys::GroupStateEarKey, signatures::signable::Signable},
    identifiers::{AttachmentId, QsReference, QualifiedGroupId},
    messages::{
        client_ds::UserProfileKeyUpdateParams,
        client_ds_out::{
            CreateGroupParamsOut, DeleteGroupParamsOut, ExternalCommitInfoIn,
            GroupOperationParamsOut, SendMessageParamsOut, WelcomeInfoIn,
        },
    },
    time::TimeStamp,
};
use airprotos::{
    convert::{RefInto, TryRefInto},
    delivery_service::v1::{
        AddUsersInfo, ConnectionGroupInfoRequest, CreateGroupPayload, DeleteGroupPayload,
        ExternalCommitInfoRequest, GetAttachmentUrlPayload, GroupOperationPayload,
        JoinConnectionGroupRequest, ProvisionAttachmentPayload, ProvisionAttachmentResponse,
        RequestGroupIdRequest, ResyncPayload, SelfRemovePayload, SendMessagePayload,
        UpdateProfileKeyPayload, WelcomeInfoPayload,
        delivery_service_client::DeliveryServiceClient,
    },
    validation::MissingFieldExt,
};
use mimi_room_policy::VerifiedRoomState;
use mls_assist::{
    messages::AssistedMessageOut,
    openmls::{
        group::GroupEpoch,
        prelude::{GroupId, LeafNodeIndex},
    },
};
use tonic::transport::Channel;
use tracing::error;

use super::DsRequestError;

#[derive(Debug, Clone)]
pub(crate) struct DsGrpcClient {
    client: DeliveryServiceClient<Channel>,
}

impl DsGrpcClient {
    pub(crate) fn new(client: DeliveryServiceClient<Channel>) -> Self {
        Self { client }
    }

    pub(crate) async fn request_group_id(&self) -> Result<GroupId, DsRequestError> {
        let response = self
            .client
            .clone()
            .request_group_id(RequestGroupIdRequest {})
            .await?
            .into_inner();
        let qgid: QualifiedGroupId = response
            .group_id
            .ok_or_missing_field("group_id")
            .map_err(|error| {
                error!(%error, "unexpected response");
                DsRequestError::UnexpectedResponse
            })?
            .try_ref_into()
            .map_err(|error| {
                error!(%error, "unexpected response");
                DsRequestError::UnexpectedResponse
            })?;
        Ok(qgid.into())
    }

    pub(crate) async fn send_message(
        &self,
        params: SendMessageParamsOut,
        signing_key: &ClientSigningKey,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<TimeStamp, DsRequestError> {
        let payload = SendMessagePayload {
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            message: Some(params.message.try_ref_into()?),
            sender: Some(params.sender.into()),
            suppress_notifications: Some(params.suppress_notifications),
        };

        let request = payload.sign(signing_key)?;
        let response = self
            .client
            .clone()
            .send_message(request)
            .await?
            .into_inner();

        Ok(response
            .fanout_timestamp
            .ok_or(DsRequestError::UnexpectedResponse)?
            .into())
    }

    pub(crate) async fn create_group(
        &self,
        payload: CreateGroupParamsOut,
        signing_key: &ClientSigningKey,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<(), DsRequestError> {
        let qgid: QualifiedGroupId = payload.group_id.try_into()?;
        let payload = CreateGroupPayload {
            qgid: Some(qgid.ref_into()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            ratchet_tree: Some(payload.ratchet_tree.try_ref_into()?),
            encrypted_user_profile_key: Some(payload.encrypted_user_profile_key.into()),
            creator_client_reference: Some(payload.creator_client_reference.into()),
            group_info: Some(payload.group_info.try_ref_into()?),
            room_state: Some(payload.room_state.unverified().try_ref_into()?),
        };
        let request = payload.sign(signing_key)?;
        self.client.clone().create_group(request).await?;
        Ok(())
    }

    pub(crate) async fn delete_group(
        &self,
        params: DeleteGroupParamsOut,
        signing_key: &ClientSigningKey,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<TimeStamp, DsRequestError> {
        let payload = DeleteGroupPayload {
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            commit: Some(params.commit.try_ref_into()?),
        };
        let request = payload.sign(signing_key)?;
        let response = self
            .client
            .clone()
            .delete_group(request)
            .await?
            .into_inner();
        Ok(response
            .fanout_timestamp
            .ok_or(DsRequestError::UnexpectedResponse)?
            .into())
    }

    pub(crate) async fn group_operation(
        &self,
        payload: GroupOperationParamsOut,
        signing_key: &ClientSigningKey,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<TimeStamp, DsRequestError> {
        let add_users_info = payload
            .add_users_info_option
            .map(|add_user_infos| {
                Ok::<_, DsRequestError>(AddUsersInfo {
                    welcome: Some(add_user_infos.welcome.try_ref_into()?),
                    encrypted_welcome_attribution_info: add_user_infos
                        .encrypted_welcome_attribution_infos
                        .into_iter()
                        .map(From::from)
                        .collect(),
                })
            })
            .transpose()?;
        let payload = GroupOperationPayload {
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            commit: Some(payload.commit.try_ref_into()?),
            add_users_info,
        };
        let request = payload.sign(signing_key)?;
        let response = self
            .client
            .clone()
            .group_operation(request)
            .await?
            .into_inner();
        Ok(response
            .fanout_timestamp
            .ok_or(DsRequestError::UnexpectedResponse)?
            .into())
    }

    pub(crate) async fn connection_group_info(
        &self,
        group_id: GroupId,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<ExternalCommitInfoIn, DsRequestError> {
        let qgid: QualifiedGroupId = group_id.try_into()?;
        let request = ConnectionGroupInfoRequest {
            group_id: Some(qgid.ref_into()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
        };
        let response = self
            .client
            .clone()
            .connection_group_info(request)
            .await?
            .into_inner();
        Ok(ExternalCommitInfoIn {
            verifiable_group_info: response
                .group_info
                .ok_or(DsRequestError::UnexpectedResponse)?
                .try_ref_into()?,
            ratchet_tree_in: response
                .ratchet_tree
                .ok_or(DsRequestError::UnexpectedResponse)?
                .try_ref_into()?,
            encrypted_user_profile_keys: response
                .encrypted_user_profile_keys
                .into_iter()
                .map(TryFrom::try_from)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| DsRequestError::UnexpectedResponse)?,
            room_state: VerifiedRoomState::verify(
                response
                    .room_state
                    .ok_or(DsRequestError::UnexpectedResponse)?
                    .try_ref_into()?,
            )
            .map_err(|_| DsRequestError::UnexpectedResponse)?,
            proposals: response.proposals.into_iter().map(|m| m.tls).collect(),
        })
    }

    pub(crate) async fn update(
        &self,
        commit: AssistedMessageOut,
        signing_key: &ClientSigningKey,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<TimeStamp, DsRequestError> {
        let payload = GroupOperationPayload {
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            commit: Some(commit.try_ref_into()?),
            add_users_info: None,
        };
        let request = payload.sign(signing_key)?;
        let response = self
            .client
            .clone()
            .group_operation(request)
            .await?
            .into_inner();
        Ok(response
            .fanout_timestamp
            .ok_or(DsRequestError::UnexpectedResponse)?
            .into())
    }

    pub(crate) async fn join_connection_group(
        &self,
        external_commit: AssistedMessageOut,
        qs_client_reference: QsReference,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<TimeStamp, DsRequestError> {
        let request = JoinConnectionGroupRequest {
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            external_commit: Some(external_commit.try_ref_into()?),
            qs_client_reference: Some(qs_client_reference.into()),
        };
        let response = self
            .client
            .clone()
            .join_connection_group(request)
            .await?
            .into_inner();
        Ok(response
            .fanout_timestamp
            .ok_or(DsRequestError::UnexpectedResponse)?
            .into())
    }

    pub(crate) async fn self_remove(
        &self,
        remove_proposal: AssistedMessageOut,
        signing_key: &ClientSigningKey,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<TimeStamp, DsRequestError> {
        let payload = SelfRemovePayload {
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            remove_proposal: Some(remove_proposal.try_ref_into()?),
        };
        let request = payload.sign(signing_key)?;
        let response = self.client.clone().self_remove(request).await?.into_inner();
        Ok(response
            .fanout_timestamp
            .ok_or(DsRequestError::UnexpectedResponse)?
            .into())
    }

    pub(crate) async fn welcome_info(
        &self,
        group_id: GroupId,
        epoch: GroupEpoch,
        group_state_ear_key: &GroupStateEarKey,
        signing_key: &ClientSigningKey,
    ) -> Result<WelcomeInfoIn, DsRequestError> {
        let qgid: QualifiedGroupId = group_id.try_into()?;

        let payload = WelcomeInfoPayload {
            qgid: Some(qgid.ref_into()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            sender: Some(signing_key.credential().verifying_key().clone().into()),
            epoch: Some(epoch.into()),
        };

        let request = payload.sign(signing_key)?;
        let response = self
            .client
            .clone()
            .welcome_info(request)
            .await?
            .into_inner();
        Ok(WelcomeInfoIn {
            ratchet_tree: response
                .ratchet_tree
                .ok_or(DsRequestError::UnexpectedResponse)?
                .try_ref_into()?,
            encrypted_user_profile_keys: response
                .encrypted_user_profile_keys
                .into_iter()
                .map(TryFrom::try_from)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| DsRequestError::UnexpectedResponse)?,
            room_state: VerifiedRoomState::verify(
                response
                    .room_state
                    .ok_or(DsRequestError::UnexpectedResponse)?
                    .try_ref_into()?,
            )
            .map_err(|_| DsRequestError::UnexpectedResponse)?,
        })
    }

    pub(crate) async fn resync(
        &self,
        external_commit: AssistedMessageOut,
        signing_key: &ClientSigningKey,
        own_leaf_index: LeafNodeIndex,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<TimeStamp, DsRequestError> {
        let payload = ResyncPayload {
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            external_commit: Some(external_commit.try_ref_into()?),
            sender: Some(own_leaf_index.into()),
        };
        let request = payload.sign(signing_key)?;
        let response = self.client.clone().resync(request).await?.into_inner();
        Ok(response
            .fanout_timestamp
            .ok_or(DsRequestError::UnexpectedResponse)?
            .into())
    }

    pub(crate) async fn external_commit_info(
        &self,
        group_id: GroupId,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<ExternalCommitInfoIn, DsRequestError> {
        let qgid: QualifiedGroupId = group_id.try_into()?;
        let request = ExternalCommitInfoRequest {
            qgid: Some(qgid.ref_into()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
        };
        let response = self
            .client
            .clone()
            .external_commit_info(request)
            .await?
            .into_inner();
        Ok(ExternalCommitInfoIn {
            verifiable_group_info: response
                .group_info
                .ok_or(DsRequestError::UnexpectedResponse)?
                .try_ref_into()?,
            ratchet_tree_in: response
                .ratchet_tree
                .ok_or(DsRequestError::UnexpectedResponse)?
                .try_ref_into()?,
            encrypted_user_profile_keys: response
                .encrypted_user_profile_keys
                .into_iter()
                .map(TryFrom::try_from)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| DsRequestError::UnexpectedResponse)?,
            room_state: VerifiedRoomState::verify(
                response
                    .room_state
                    .ok_or(DsRequestError::UnexpectedResponse)?
                    .try_ref_into()?,
            )
            .map_err(|_| DsRequestError::UnexpectedResponse)?,
            proposals: response.proposals.into_iter().map(|m| m.tls).collect(),
        })
    }

    pub(crate) async fn user_profile_key_update(
        &self,
        params: UserProfileKeyUpdateParams,
        signing_key: &ClientSigningKey,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<(), DsRequestError> {
        let qgid: QualifiedGroupId = params.group_id.try_into()?;
        let payload = UpdateProfileKeyPayload {
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            group_id: Some(qgid.ref_into()),
            sender: Some(params.sender_index.into()),
            encrypted_user_profile_key: Some(params.user_profile_key.into()),
        };
        let request = payload.sign(signing_key)?;
        self.client.clone().update_profile_key(request).await?;
        Ok(())
    }

    pub(crate) async fn provision_attachment(
        &self,
        signing_key: &ClientSigningKey,
        group_state_ear_key: &GroupStateEarKey,
        group_id: &GroupId,
        sender_index: LeafNodeIndex,
        content_length: i64,
    ) -> Result<ProvisionAttachmentResponse, DsRequestError> {
        let qgid: QualifiedGroupId = group_id.try_into()?;
        let payload = ProvisionAttachmentPayload {
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            group_id: Some(qgid.ref_into()),
            sender: Some(sender_index.into()),
            use_post_policy: true,
            content_length,
        };
        let request = payload.sign(signing_key)?;
        let response = self
            .client
            .clone()
            .provision_attachment(request)
            .await?
            .into_inner();
        Ok(response)
    }

    pub(crate) async fn get_attachment_url(
        &self,
        signing_key: &ClientSigningKey,
        group_state_ear_key: &GroupStateEarKey,
        group_id: &GroupId,
        sender_index: LeafNodeIndex,
        attachment_id: AttachmentId,
    ) -> Result<String, DsRequestError> {
        let qgid: QualifiedGroupId = group_id.try_into()?;
        let payload = GetAttachmentUrlPayload {
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            group_id: Some(qgid.ref_into()),
            sender: Some(sender_index.into()),
            attachment_id: Some(attachment_id.uuid().into()),
        };
        let request = payload.sign(signing_key)?;
        let response = self
            .client
            .clone()
            .get_attachment_url(request)
            .await?
            .into_inner();
        Ok(response.download_url)
    }
}
