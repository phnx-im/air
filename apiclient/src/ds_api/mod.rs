// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! API client implementation for the DS

use aircommon::{
    LibraryError,
    credentials::keys::ClientSigningKey,
    crypto::ear::keys::GroupStateEarKey,
    identifiers::{AttachmentId, QsReference},
    messages::{
        client_ds::UserProfileKeyUpdateParams,
        client_ds_out::{
            CreateGroupParamsOut, DeleteGroupParamsOut, ExternalCommitInfoIn,
            GroupOperationParamsOut, SelfRemoveParamsOut, SendMessageParamsOut, UpdateParamsOut,
            WelcomeInfoIn,
        },
    },
    time::TimeStamp,
};
pub use airprotos::delivery_service::v1::ProvisionAttachmentResponse;
use mls_assist::{
    messages::AssistedMessageOut,
    openmls::prelude::{GroupEpoch, GroupId, LeafNodeIndex, MlsMessageOut},
};

use crate::ApiClient;

pub mod grpc;

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
        self.ds_grpc_client
            .create_group(payload, signing_key, group_state_ear_key)
            .await
    }

    /// Performs a group operation.
    pub async fn ds_group_operation(
        &self,
        payload: GroupOperationParamsOut,
        signing_key: &ClientSigningKey,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<TimeStamp, DsRequestError> {
        self.ds_grpc_client
            .group_operation(payload, signing_key, group_state_ear_key)
            .await
    }

    /// Get welcome information for a group.
    pub async fn ds_welcome_info(
        &self,
        group_id: GroupId,
        epoch: GroupEpoch,
        group_state_ear_key: &GroupStateEarKey,
        signing_key: &ClientSigningKey,
    ) -> Result<WelcomeInfoIn, DsRequestError> {
        self.ds_grpc_client
            .welcome_info(group_id, epoch, group_state_ear_key, signing_key)
            .await
    }

    /// Get external commit information for a group.
    pub async fn ds_external_commit_info(
        &self,
        group_id: GroupId,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<ExternalCommitInfoIn, DsRequestError> {
        self.ds_grpc_client
            .external_commit_info(group_id, group_state_ear_key)
            .await
    }

    /// Get external commit information for a connection group.
    pub async fn ds_connection_group_info(
        &self,
        group_id: GroupId,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<ExternalCommitInfoIn, DsRequestError> {
        self.ds_grpc_client
            .connection_group_info(group_id, group_state_ear_key)
            .await
    }

    /// Update your client in this group.
    pub async fn ds_update(
        &self,
        params: UpdateParamsOut,
        signing_key: &ClientSigningKey,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<TimeStamp, DsRequestError> {
        self.ds_grpc_client
            .update(params.commit, signing_key, group_state_ear_key)
            .await
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
        self.ds_grpc_client
            .join_connection_group(external_commit, qs_client_reference, group_state_ear_key)
            .await
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
        self.ds_grpc_client
            .resync(
                external_commit,
                signing_key,
                own_leaf_index,
                group_state_ear_key,
            )
            .await
    }

    /// Leave the given group with this client.
    pub async fn ds_self_remove(
        &self,
        params: SelfRemoveParamsOut,
        signing_key: &ClientSigningKey,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<TimeStamp, DsRequestError> {
        self.ds_grpc_client
            .self_remove(params.remove_proposal, signing_key, group_state_ear_key)
            .await
    }

    /// Send a message to the given group.
    pub async fn ds_send_message(
        &self,
        params: SendMessageParamsOut,
        signing_key: &ClientSigningKey,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<TimeStamp, DsRequestError> {
        self.ds_grpc_client
            .send_message(params, signing_key, group_state_ear_key)
            .await
    }

    /// Delete the given group.
    pub async fn ds_delete_group(
        &self,
        params: DeleteGroupParamsOut,
        signing_key: &ClientSigningKey,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<TimeStamp, DsRequestError> {
        self.ds_grpc_client
            .delete_group(params, signing_key, group_state_ear_key)
            .await
    }

    /// Update the user's user profile key
    pub async fn ds_user_profile_key_update(
        &self,
        params: UserProfileKeyUpdateParams,
        signing_key: &ClientSigningKey,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<(), DsRequestError> {
        self.ds_grpc_client
            .user_profile_key_update(params, signing_key, group_state_ear_key)
            .await
    }

    /// Request a group ID.
    pub async fn ds_request_group_id(&self) -> Result<GroupId, DsRequestError> {
        self.ds_grpc_client.request_group_id().await
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
        self.ds_grpc_client
            .provision_attachment(
                signing_key,
                group_state_ear_key,
                group_id,
                sender_index,
                content_length,
            )
            .await
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
        self.ds_grpc_client
            .get_attachment_url(
                signing_key,
                group_state_ear_key,
                group_id,
                sender_index,
                attachment_id,
            )
            .await
    }
}
