// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! API client implementation for the DS

use aircommon::{
    LibraryError,
    credentials::keys::ClientSigningKey,
    crypto::{ear::keys::GroupStateEarKey, signatures::signable::Signable},
    identifiers::{AttachmentId, QsReference, QualifiedGroupId},
    messages::{
        client_ds::UserProfileKeyUpdateParams,
        client_ds_out::{
            CreateGroupParamsOut, DeleteGroupParamsOut, ExternalCommitInfoIn,
            GroupOperationParamsOut, SelfRemoveParamsOut, SendMessageParamsOut,
            TargetedMessageParamsOut, UpdateParamsOut, WelcomeInfoIn,
        },
    },
    time::TimeStamp,
};
pub use airprotos::delivery_service::v1::ProvisionAttachmentResponse;
use airprotos::{
    convert::{RefInto, TryRefInto},
    delivery_service::v1::{
        AddUsersInfo, ConnectionGroupInfoRequest, CreateGroupPayload, DeleteGroupPayload,
        ExternalCommitInfoRequest, GetAttachmentUrlPayload, GroupOperationPayload,
        JoinConnectionGroupRequest, ProvisionAttachmentPayload, RequestGroupIdRequest,
        ResyncPayload, SelfRemovePayload, SendMessagePayload, TargetedMessagePayload,
        UpdateProfileKeyPayload, WelcomeInfoPayload,
    },
    validation::MissingFieldExt,
};
use mimi_room_policy::VerifiedRoomState;
use mls_assist::{
    messages::AssistedMessageOut,
    openmls::prelude::{GroupEpoch, GroupId, LeafNodeIndex, MlsMessageOut},
};
use tracing::error;

use crate::ApiClient;

#[derive(Debug, thiserror::Error)]
pub enum DsRequestError {
    #[error("Library Error")]
    LibraryError,
    #[error(transparent)]
    Tonic(#[from] tonic::Status),
    #[error(transparent)]
    Tls(#[from] tls_codec::Error),
    #[error("We received an unexpected response type.")]
    UnexpectedResponse,
}

impl From<LibraryError> for DsRequestError {
    fn from(_: LibraryError) -> Self {
        Self::LibraryError
    }
}

impl ApiClient {
    /// Creates a new group on the DS.
    pub async fn ds_create_group(
        &self,
        payload: CreateGroupParamsOut,
        signing_key: &ClientSigningKey,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<(), DsRequestError> {
        let qgid: QualifiedGroupId = payload.group_id.try_into()?;
        let payload = CreateGroupPayload {
            client_metadata: Some(self.metadata().clone()),
            qgid: Some(qgid.ref_into()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            ratchet_tree: Some(payload.ratchet_tree.try_ref_into()?),
            encrypted_user_profile_key: Some(payload.encrypted_user_profile_key.into()),
            creator_client_reference: Some(payload.creator_client_reference.into()),
            group_info: Some(payload.group_info.try_ref_into()?),
            room_state: Some(payload.room_state.unverified().try_ref_into()?),
        };
        let request = payload.sign(signing_key)?;
        self.ds_grpc_client().create_group(request).await?;
        Ok(())
    }

    /// Performs a group operation.
    pub async fn ds_group_operation(
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
            client_metadata: Some(self.metadata().clone()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            commit: Some(payload.commit.try_ref_into()?),
            add_users_info,
        };
        let request = payload.sign(signing_key)?;
        let response = self
            .ds_grpc_client()
            .group_operation(request)
            .await?
            .into_inner();
        Ok(response
            .fanout_timestamp
            .ok_or(DsRequestError::UnexpectedResponse)?
            .into())
    }

    /// Get welcome information for a group.
    pub async fn ds_welcome_info(
        &self,
        group_id: GroupId,
        epoch: GroupEpoch,
        group_state_ear_key: &GroupStateEarKey,
        signing_key: &ClientSigningKey,
    ) -> Result<WelcomeInfoIn, DsRequestError> {
        let qgid: QualifiedGroupId = group_id.try_into()?;
        let payload = WelcomeInfoPayload {
            client_metadata: Some(self.metadata().clone()),
            qgid: Some(qgid.ref_into()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            sender: Some(signing_key.credential().verifying_key().clone().into()),
            epoch: Some(epoch.into()),
        };
        let request = payload.sign(signing_key)?;
        let response = self
            .ds_grpc_client()
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

    /// Get external commit information for a group.
    pub async fn ds_external_commit_info(
        &self,
        group_id: GroupId,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<ExternalCommitInfoIn, DsRequestError> {
        let qgid: QualifiedGroupId = group_id.try_into()?;
        let request = ExternalCommitInfoRequest {
            client_metadata: Some(self.metadata().clone()),
            qgid: Some(qgid.ref_into()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
        };
        let response = self
            .ds_grpc_client()
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

    /// Get external commit information for a connection group.
    pub async fn ds_connection_group_info(
        &self,
        group_id: GroupId,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<ExternalCommitInfoIn, DsRequestError> {
        let qgid: QualifiedGroupId = group_id.try_into()?;
        let request = ConnectionGroupInfoRequest {
            client_metadata: Some(self.metadata().clone()),
            group_id: Some(qgid.ref_into()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
        };
        let response = self
            .ds_grpc_client()
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

    /// Update your client in this group.
    pub async fn ds_update(
        &self,
        params: UpdateParamsOut,
        signing_key: &ClientSigningKey,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<TimeStamp, DsRequestError> {
        let payload = GroupOperationPayload {
            client_metadata: Some(self.metadata().clone()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            commit: Some(params.commit.try_ref_into()?),
            add_users_info: None,
        };
        let request = payload.sign(signing_key)?;
        let response = self
            .ds_grpc_client()
            .group_operation(request)
            .await?
            .into_inner();
        Ok(response
            .fanout_timestamp
            .ok_or(DsRequestError::UnexpectedResponse)?
            .into())
    }

    /// Join the connection group with a new client.
    pub async fn ds_join_connection_group(
        &self,
        commit: MlsMessageOut,
        group_info: MlsMessageOut,
        qs_client_reference: QsReference,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<TimeStamp, DsRequestError> {
        let external_commit = AssistedMessageOut::new(commit, Some(group_info));
        let request = JoinConnectionGroupRequest {
            client_metadata: Some(self.metadata().clone()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            external_commit: Some(external_commit.try_ref_into()?),
            qs_client_reference: Some(qs_client_reference.into()),
        };
        let response = self
            .ds_grpc_client()
            .join_connection_group(request)
            .await?
            .into_inner();
        Ok(response
            .fanout_timestamp
            .ok_or(DsRequestError::UnexpectedResponse)?
            .into())
    }

    /// Resync a client to rejoin a group.
    pub async fn ds_resync(
        &self,
        commit: MlsMessageOut,
        group_info: MlsMessageOut,
        signing_key: &ClientSigningKey,
        group_state_ear_key: &GroupStateEarKey,
        own_leaf_index: LeafNodeIndex,
    ) -> Result<TimeStamp, DsRequestError> {
        let external_commit = AssistedMessageOut::new(commit, Some(group_info));
        let payload = ResyncPayload {
            client_metadata: Some(self.metadata().clone()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            external_commit: Some(external_commit.try_ref_into()?),
            sender: Some(own_leaf_index.into()),
        };
        let request = payload.sign(signing_key)?;
        let response = self.ds_grpc_client().resync(request).await?.into_inner();
        Ok(response
            .fanout_timestamp
            .ok_or(DsRequestError::UnexpectedResponse)?
            .into())
    }

    /// Leave the given group with this client.
    pub async fn ds_self_remove(
        &self,
        params: SelfRemoveParamsOut,
        signing_key: &ClientSigningKey,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<TimeStamp, DsRequestError> {
        let payload = SelfRemovePayload {
            client_metadata: Some(self.metadata().clone()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            remove_proposal: Some(params.remove_proposal.try_ref_into()?),
        };
        let request = payload.sign(signing_key)?;
        let response = self
            .ds_grpc_client()
            .self_remove(request)
            .await?
            .into_inner();
        Ok(response
            .fanout_timestamp
            .ok_or(DsRequestError::UnexpectedResponse)?
            .into())
    }

    /// Send a message to the given group.
    pub async fn ds_send_message(
        &self,
        params: SendMessageParamsOut,
        signing_key: &ClientSigningKey,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<TimeStamp, DsRequestError> {
        let payload = SendMessagePayload {
            client_metadata: Some(self.metadata().clone()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            message: Some(params.message.try_ref_into()?),
            sender: Some(params.sender.into()),
            suppress_notifications: Some(params.suppress_notifications),
        };
        let request = payload.sign(signing_key)?;
        let response = self
            .ds_grpc_client()
            .send_message(request)
            .await?
            .into_inner();
        Ok(response
            .fanout_timestamp
            .ok_or(DsRequestError::UnexpectedResponse)?
            .into())
    }

    /// Send a message to the recipient within the given group.
    pub async fn ds_targeted_message(
        &self,
        params: TargetedMessageParamsOut,
        signing_key: &ClientSigningKey,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<TimeStamp, DsRequestError> {
        let payload = TargetedMessagePayload {
            client_metadata: Some(self.metadata().clone()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            sender: Some(params.sender.into()),
            targeted_message_type: Some(params.message_type.try_ref_into()?),
        };
        let request = payload.sign(signing_key)?;
        let response = self
            .ds_grpc_client()
            .targeted_message(request)
            .await?
            .into_inner();
        Ok(response
            .fanout_timestamp
            .ok_or(DsRequestError::UnexpectedResponse)?
            .into())
    }

    /// Delete the given group.
    pub async fn ds_delete_group(
        &self,
        params: DeleteGroupParamsOut,
        signing_key: &ClientSigningKey,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<TimeStamp, DsRequestError> {
        let payload = DeleteGroupPayload {
            client_metadata: Some(self.metadata().clone()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            commit: Some(params.commit.try_ref_into()?),
        };
        let request = payload.sign(signing_key)?;
        let response = self
            .ds_grpc_client()
            .delete_group(request)
            .await?
            .into_inner();
        Ok(response
            .fanout_timestamp
            .ok_or(DsRequestError::UnexpectedResponse)?
            .into())
    }

    /// Update the user's user profile key
    pub async fn ds_user_profile_key_update(
        &self,
        params: UserProfileKeyUpdateParams,
        signing_key: &ClientSigningKey,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<(), DsRequestError> {
        let qgid: QualifiedGroupId = params.group_id.try_into()?;
        let payload = UpdateProfileKeyPayload {
            client_metadata: Some(self.metadata().clone()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            group_id: Some(qgid.ref_into()),
            sender: Some(params.sender_index.into()),
            encrypted_user_profile_key: Some(params.user_profile_key.into()),
        };
        let request = payload.sign(signing_key)?;
        self.ds_grpc_client().update_profile_key(request).await?;
        Ok(())
    }

    /// Request a group ID.
    pub async fn ds_request_group_id(&self) -> Result<GroupId, DsRequestError> {
        let response = self
            .ds_grpc_client()
            .request_group_id(RequestGroupIdRequest {
                client_metadata: Some(self.metadata().clone()),
            })
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

    /// Provision an attachment for a group.
    ///
    /// The result is used to upload the attachment to the server.
    pub async fn ds_provision_attachment(
        &self,
        signing_key: &ClientSigningKey,
        group_state_ear_key: &GroupStateEarKey,
        group_id: &GroupId,
        sender_index: LeafNodeIndex,
        content_length: i64,
    ) -> Result<ProvisionAttachmentResponse, DsRequestError> {
        let qgid: QualifiedGroupId = group_id.try_into()?;
        let payload = ProvisionAttachmentPayload {
            client_metadata: Some(self.metadata().clone()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            group_id: Some(qgid.ref_into()),
            sender: Some(sender_index.into()),
            use_post_policy: true,
            content_length,
        };
        let request = payload.sign(signing_key)?;
        let response = self
            .ds_grpc_client()
            .provision_attachment(request)
            .await?
            .into_inner();
        Ok(response)
    }

    /// Get the download URL for an attachment.
    pub async fn ds_get_attachment_url(
        &self,
        signing_key: &ClientSigningKey,
        group_state_ear_key: &GroupStateEarKey,
        group_id: &GroupId,
        sender_index: LeafNodeIndex,
        attachment_id: AttachmentId,
    ) -> Result<String, DsRequestError> {
        let qgid: QualifiedGroupId = group_id.try_into()?;
        let payload = GetAttachmentUrlPayload {
            client_metadata: Some(self.metadata().clone()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            group_id: Some(qgid.ref_into()),
            sender: Some(sender_index.into()),
            attachment_id: Some(attachment_id.uuid().into()),
        };
        let request = payload.sign(signing_key)?;
        let response = self
            .ds_grpc_client()
            .get_attachment_url(request)
            .await?
            .into_inner();
        Ok(response.download_url)
    }
}
