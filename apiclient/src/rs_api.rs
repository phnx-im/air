use aircommon::{
    crypto::signatures::{keys::QsUserSigningKey, signable::Signable},
    identifiers::QsUserId,
};
use airprotos::relay_service::v1::{LinkClientRequest, LinkClientRequestPayload, RelayFrame};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::metadata::MetadataValue;

use crate::ApiClient;

const SESSION_ID: &str = "session-id";

#[derive(thiserror::Error, Debug)]
pub enum RsRequestError {
    #[error(transparent)]
    LibraryError(#[from] aircommon::LibraryError),
    #[error(transparent)]
    Tls(#[from] tls_codec::Error),
    #[error(transparent)]
    Tonic(#[from] tonic::Status),
    #[error("send error: channel closed? {0}")]
    SendError(#[from] mpsc::error::SendError<RelayFrame>),
}

// TODO: TruncatedSessionId with ALPHABET and 6 chars all lowercase, normalised
// with a function from_bytes

impl ApiClient {
    // start point (new client that shows Qrcode)
    pub async fn rs_provision_client(
        &self,
        session_id: &[u8],
    ) -> Result<(mpsc::Sender<RelayFrame>, tonic::Streaming<RelayFrame>), RsRequestError> {
        let (tx, rx) = mpsc::channel::<RelayFrame>(8);
        let mut request = tonic::Request::new(ReceiverStream::new(rx));
        request
            .metadata_mut()
            .insert_bin(SESSION_ID, MetadataValue::from_bytes(session_id));

        let response: tonic::Response<tonic::Streaming<RelayFrame>> =
            self.rs_grpc_client().provision_client(request).await?;

        Ok((tx, response.into_inner()))
    }

    pub async fn rs_link_client(
        &self,
        qs_user_id: QsUserId,
        qs_user_signing_key: &QsUserSigningKey,
        session_id: String,
    ) -> Result<(mpsc::Sender<RelayFrame>, tonic::Streaming<RelayFrame>), RsRequestError> {
        let (tx, rx) = mpsc::channel::<RelayFrame>(8);

        let payload = LinkClientRequestPayload {
            client_metadata: Some(self.metadata().clone()),
            sender: Some(qs_user_id.into()),
            session_id,
        };

        let link_client_request: LinkClientRequest = payload.sign(qs_user_signing_key)?;
        tx.send(link_client_request.into_relay_frame()).await?;

        let response = self
            .rs_grpc_client()
            .link_client(tonic::Request::new(ReceiverStream::new(rx)))
            .await?;

        Ok((tx, response.into_inner()))
    }
}
