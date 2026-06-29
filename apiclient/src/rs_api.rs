// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    crypto::signatures::{keys::QsUserSigningKey, signable::Signable},
    identifiers::QsUserId,
};
use airprotos::relay_service::v1::{
    LinkClientRequest, LinkClientRequestPayload, LinkingSessionId, RelayFrame,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::ApiClient;

#[derive(thiserror::Error, Debug)]
pub enum RsRequestError {
    #[error(transparent)]
    LibraryError(#[from] aircommon::LibraryError),
    #[error(transparent)]
    Tls(#[from] tls_codec::Error),
    #[error(transparent)]
    Tonic(tonic::Status),
    #[error("send error: channel closed? {0}")]
    SendError(#[from] mpsc::error::SendError<RelayFrame>),
    #[error("session not found")]
    SessionNotFound,
}

impl From<tonic::Status> for RsRequestError {
    fn from(status: tonic::Status) -> Self {
        match status.code() {
            tonic::Code::NotFound => Self::SessionNotFound,
            _ => Self::Tonic(status),
        }
    }
}

impl ApiClient {
    pub async fn rs_multi_device_provision_client(
        &self,
    ) -> Result<(mpsc::Sender<RelayFrame>, tonic::Streaming<RelayFrame>), RsRequestError> {
        // don't buffer frames: we expect the peer to consume what we send before we move forward
        let (tx, rx) = mpsc::channel::<RelayFrame>(1);
        let request = tonic::Request::new(ReceiverStream::new(rx));

        let response: tonic::Response<tonic::Streaming<RelayFrame>> = self
            .rs_grpc_client()
            .multi_device_provision_client(request)
            .await?;

        Ok((tx, response.into_inner()))
    }

    pub async fn rs_multi_device_link_client(
        &self,
        qs_user_id: QsUserId,
        qs_user_signing_key: &QsUserSigningKey,
        linking_session_id: LinkingSessionId,
    ) -> Result<(mpsc::Sender<RelayFrame>, tonic::Streaming<RelayFrame>), RsRequestError> {
        // don't buffer frames: we expect the peer to consume what we send before we move forward
        let (tx, rx) = mpsc::channel::<RelayFrame>(1);

        let payload = LinkClientRequestPayload {
            client_metadata: Some(self.metadata().clone()),
            sender: Some(qs_user_id.into()),
            session_id: Some(linking_session_id),
        };

        let link_client_request: LinkClientRequest = payload.sign(qs_user_signing_key)?;
        tx.send(link_client_request.into_relay_frame()).await?;

        let response = self
            .rs_grpc_client()
            .multi_device_link_client(tonic::Request::new(ReceiverStream::new(rx)))
            .await?;

        Ok((tx, response.into_inner()))
    }
}
