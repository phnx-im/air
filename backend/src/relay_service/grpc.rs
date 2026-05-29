use std::pin::Pin;

use aircommon::crypto::signatures::{keys::QsUserVerifyingKey, signable::Verifiable};
use airprotos::{
    relay_service::v1::{
        LinkClientRequest, LinkClientRequestPayload, LinkingSessionId, RelayFrame,
        relay_service_server::RelayService,
    },
    validation::MissingFieldExt,
};
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
    session_id: LinkingSessionId,
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
        let mut inbound = request.into_inner();

        let (outbound_tx, outbound_rx) = mpsc::channel::<Result<RelayFrame, Status>>(8);
        let first_frame_outbound_tx = outbound_tx.clone();

        let relay_sessions = self.rs.sessions.clone();
        tokio::spawn(self.rs.stop.clone().run_until_cancelled_owned(async move {
            // we always expect a KeyPackage as first frame from the initiating client
            // and we use its SHA256 digest as the session ID
            let Some(Ok(key_package_bytes)) = inbound.next().await else {
                error!("failed to receive KeyPackage");
                return;
            };

            // we take the requested session ID from the initiator and truncate it as long as we don't have collisions
            // we lock sessions for all clients because we might check many buckets
            let mut sessions = relay_sessions.lock().await;
            let Some(truncated_session_id) =
                LinkingSessionId::generate(key_package_bytes.as_slice(), |session_id| {
                    sessions.contains_key(session_id)
                })
            else {
                error!("linking session ID collision");
                return;
            };

            info!(%truncated_session_id, "starting new pairing session");

            let (peer_ready_tx, peer_ready_rx) = oneshot::channel();

            sessions.insert(
                truncated_session_id.clone(),
                Pending {
                    outbound_tx,
                    peer_ready_tx,
                },
            );

            // release the lock
            drop(sessions);

            // we report the length of the truncated session ID to the peer
            if let Err(error) = first_frame_outbound_tx
                .send(Ok(RelayFrame {
                    payload: Bytes::from_owner(truncated_session_id.digits().to_be_bytes()),
                }))
                .await
            {
                error!(%error, "failed to send session ID length");
                return;
            };

            // then we wait for the peer to connect
            let peer_outbound = match timeout(SESSION_TIMEOUT, peer_ready_rx).await {
                Ok(Ok(tx)) => tx,
                Ok(Err(error)) => {
                    relay_sessions.lock().await.remove(&truncated_session_id);
                    info!(%error, "peer disconnected before sending outbound channel");
                    return;
                }
                Err(_) => {
                    relay_sessions.lock().await.remove(&truncated_session_id);
                    info!(%truncated_session_id, "timed out waiting for peer");
                    return;
                }
            };

            // then we (re)send the key package to the peer
            if let Err(error) = peer_outbound.send(Ok(key_package_bytes)).await {
                error!(%error, "failed to send key package");
                return;
            };

            // finally we pipe the inbound relay frames to the peer
            pipe_inbound_to_peer_outbound(truncated_session_id, inbound, peer_outbound).await;
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

        let session_id = payload.session_id.ok_or_missing_field("session_id")?;
        info!(%session_id, "pairing with existing session");

        // Outbound channel: messages we will send back to *this* client.
        let (outbound_tx, outbound_rx) = mpsc::channel::<Result<RelayFrame, Status>>(8);

        if let Some(pending) = self.rs.sessions.lock().await.remove(&session_id) {
            // Fire the peer's oneshot with our outbound sender so they can start forwarding to us.
            if pending.peer_ready_tx.send(outbound_tx).is_err() {
                warn!("failed to signal that peer is ready (initiator disconnected)");
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
