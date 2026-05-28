use std::pin::Pin;

use aircommon::crypto::signatures::{keys::QsUserVerifyingKey, signable::Verifiable};
use airprotos::{
    relay_service::v1::{
        LinkClientRequest, LinkClientRequestPayload, METADATA_SESSION_ID, RelayFrame,
        relay_service_server::RelayService,
    },
    validation::MissingFieldExt,
};
use dashmap::Entry;
use futures_util::Stream;
use prost::bytes::Bytes;
use tokio::{
    sync::{mpsc, oneshot},
    time::timeout,
};
use tokio_stream::{StreamExt, wrappers::ReceiverStream};
use tonic::{Request, Response, Status, Streaming, async_trait};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    qs::QsConnector,
    relay_service::{Pending, Rs, SESSION_TIMEOUT},
};

pub struct GrpcRs<Qep: QsConnector> {
    rs: Rs,
    qs_connector: Qep,
}

impl<Qep: QsConnector> GrpcRs<Qep> {
    pub fn new(rs: Rs, qs_connector: Qep) -> Self {
        Self { rs, qs_connector }
    }
}

async fn pipe_inbound_to_peer_outbound(
    session_id: String,
    mut inbound: Streaming<RelayFrame>,
    peer_outbound: mpsc::Sender<Result<RelayFrame, Status>>,
) {
    while let Some(msg) = inbound.next().await {
        match msg {
            Ok(frame) => {
                if peer_outbound.send(Ok(frame)).await.is_err() {
                    break;
                }
            }
            Err(status) => {
                warn!(%session_id, %status, "inbound error");
                break;
            }
        }
    }

    info!(session_id = %session_id, "client disconnected");
}

#[async_trait]
impl<Qep: QsConnector> RelayService for GrpcRs<Qep> {
    type ProvisionClientStream = Pin<Box<dyn Stream<Item = Result<RelayFrame, Status>> + Send>>;

    async fn provision_client(
        &self,
        request: Request<Streaming<RelayFrame>>,
    ) -> Result<Response<Self::ProvisionClientStream>, Status> {
        let requested_session_id = request
            .metadata()
            .get(METADATA_SESSION_ID)
            .ok_or_else(|| Status::invalid_argument("no session id in metadata"))?
            .to_str()
            .map_err(|_| Status::invalid_argument("invalid session id"))?
            .to_owned();

        let inbound = request.into_inner();

        let (outbound_tx, outbound_rx) = mpsc::channel::<Result<RelayFrame, Status>>(8);
        let first_frame_outbound_tx = outbound_tx.clone();

        let (peer_ready_tx, peer_ready_rx) = oneshot::channel();

        loop {
            match self.rs.sessions.entry(requested_session_id.clone()) {
                Entry::Occupied(_) => {
                    return Err(Status::aborted("session ID collision"));
                }
                Entry::Vacant(vacant) => {
                    info!(requested_session_id = %vacant.key(), "starting new pairing session");
                    vacant.insert(Pending {
                        outbound_tx,
                        peer_ready_tx,
                    });
                    break;
                }
            }
        }

        // we report the session ID to the peer
        let session_id = requested_session_id.to_string();
        // TODO: this should be instead the fingerprint of the key package
        first_frame_outbound_tx
            .send(Ok(RelayFrame {
                payload: Bytes::from(session_id.clone()),
            }))
            .await
            .map_err(|_| Status::internal("failed to send session ID"))?;

        let sessions = self.rs.sessions.clone();
        tokio::spawn(self.rs.stop.clone().run_until_cancelled_owned(async move {
            let peer_outbound = match timeout(SESSION_TIMEOUT, peer_ready_rx).await {
                Ok(Ok(tx)) => tx,
                Ok(Err(_)) => {
                    sessions.remove(&session_id);
                    error!("peer disconnected before sending outbound channel");
                    return;
                }
                Err(_) => {
                    sessions.remove(&session_id);
                    warn!(%session_id, "timed out waiting for peer");
                    return;
                }
            };
            pipe_inbound_to_peer_outbound(session_id, inbound, peer_outbound).await;
        }));

        let out_stream = ReceiverStream::new(outbound_rx);
        Ok(Response::new(
            Box::pin(out_stream) as Self::ProvisionClientStream
        ))
    }

    type LinkClientStream = Pin<Box<dyn Stream<Item = Result<RelayFrame, Status>> + Send>>;

    async fn link_client(
        &self,
        request: Request<Streaming<RelayFrame>>,
    ) -> Result<Response<Self::LinkClientStream>, Status> {
        let mut inbound = request.into_inner();

        // the first frame we expect is the initial request payload
        let first_frame = inbound
            .message()
            .await?
            .ok_or_else(|| Status::invalid_argument("stream closed before LinkClientRequest"))?;

        let request: LinkClientRequest =
            prost::Message::decode(first_frame.payload).map_err(|error| {
                error!(%error, "failed to decode initial msg");
                Status::internal("decoding failure")
            })?;

        let qs_user_id: Uuid = request
            .payload
            .as_ref()
            .ok_or_missing_field("payload")?
            .sender
            .ok_or_missing_field("sender")?
            .value
            .ok_or_missing_field("uuid value")?
            .into();

        let qs_user_signature_key: QsUserVerifyingKey = self
            .qs_connector
            .user_verifying_key(aircommon::identifiers::QsUserId::from(qs_user_id))
            .await
            .map_err(|error| {
                error!(%error, "failed to load QS user signing key");
                Status::internal("internal error")
            })?
            .ok_or_else(|| Status::not_found("user not found"))?;

        let payload: LinkClientRequestPayload = request
            .verify(&qs_user_signature_key)
            .map_err(|_| Status::invalid_argument("invalid signature"))?;

        let session_id = payload.session_id;
        info!(%session_id, "pairing with existing session");

        // Outbound channel: messages we will send back to *this* client.
        let (outbound_tx, outbound_rx) = mpsc::channel::<Result<RelayFrame, Status>>(8);

        if let Some((_, pending)) = self.rs.sessions.remove(&session_id) {
            // Fire the peer's oneshot with our outbound sender so they can start forwarding to us.
            if pending.peer_ready_tx.send(outbound_tx).is_err() {
                error!("failed to send peer ready oneshot");
                return Err(Status::aborted(
                    "peer disconnected before establishing relay pipe",
                ));
            }

            tokio::spawn(self.rs.stop.clone().run_until_cancelled_owned(
                pipe_inbound_to_peer_outbound(session_id, inbound, pending.outbound_tx),
            ));
        } else {
            return Err(Status::not_found("session not found"));
        }

        let out_stream = ReceiverStream::new(outbound_rx);
        Ok(Response::new(Box::pin(out_stream) as Self::LinkClientStream))
    }
}
