use std::pin::Pin;

use airprotos::{
    relay_service::v1::{LinkClientRequest, RelayFrame, relay_service_server::RelayService},
    validation::MissingFieldExt,
};
use futures_util::Stream;
use prost::{Message, bytes::Bytes};
use tokio::{
    sync::{mpsc, oneshot},
    time::timeout,
};
use tokio_stream::{StreamExt, wrappers::ReceiverStream};
use tonic::{Request, Response, Status, Streaming, async_trait};
use tracing::{error, info, warn};

use crate::{
    qs::QsConnector,
    relay_service::{Pending, Rs, SESSION_TIMEOUT},
};

pub struct GrpcRs<Qep: QsConnector> {
    pub(super) rs: Rs,
    qs_connector: Qep,
}

impl<Qep: QsConnector> GrpcRs<Qep> {
    pub fn new(rs: Rs, qs_connector: Qep) -> Self {
        Self { rs, qs_connector }
    }

    fn pipe_inbound_to_peer_outbound(
        session_id: String,
        mut inbound: Streaming<RelayFrame>,
        peer_outbound: mpsc::Sender<Result<RelayFrame, Status>>,
    ) {
        // Pipe inbound -> peer_outbound.
        tokio::spawn(async move {
            let timed_out = timeout(SESSION_TIMEOUT, async {
                while let Some(msg) = inbound.next().await {
                    match msg {
                        Ok(frame) => {
                            if peer_outbound.send(Ok(frame)).await.is_err() {
                                break;
                            }
                        }
                        Err(status) => {
                            warn!(%session_id, %status , "inbound error");
                            break;
                        }
                    }
                }
            })
            .await
            .is_err();

            if timed_out {
                warn!(session_id = %session_id, "session timed out after 30s");
            } else {
                info!(session_id = %session_id, "client disconnected");
            }
        });
    }
}

#[async_trait]
impl<Qep: QsConnector> RelayService for GrpcRs<Qep> {
    type ProvisionClientStream = Pin<Box<dyn Stream<Item = Result<RelayFrame, Status>> + Send>>;

    async fn provision_client(
        &self,
        request: Request<Streaming<RelayFrame>>,
    ) -> Result<Response<Self::ProvisionClientStream>, Status> {
        let inbound = request.into_inner();

        let (outbound_tx, outbound_rx) = mpsc::channel::<Result<RelayFrame, Status>>(8);
        let (peer_ready_tx, peer_ready_rx) = oneshot::channel();

        let session_id = self
            .rs
            .insert_session(Pending {
                outbound_tx: outbound_tx.clone(),
                peer_ready: peer_ready_tx,
            })
            .await
            .ok_or_else(|| Status::internal("no more session IDs available"))?;

        // we report the session ID to the peer
        // TODO: this should be instead the fingerprint of the key package
        outbound_tx
            .send(Ok(RelayFrame {
                payload: Bytes::from(session_id.clone()),
            }))
            .await
            .map_err(|_| Status::internal("failed to send session ID"))?;

        let peer_outbound = match timeout(SESSION_TIMEOUT, peer_ready_rx).await {
            Ok(Ok(tx)) => tx,
            Ok(Err(_)) => return Err(Status::aborted("peer never arrived")),
            Err(_) => {
                self.rs.sessions.lock().await.remove(&session_id);
                warn!(%session_id, "timed out waiting for peer");
                return Err(Status::deadline_exceeded("timed out waiting for peer"));
            }
        };

        Self::pipe_inbound_to_peer_outbound(session_id, inbound, peer_outbound);

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

        // TODO: we should check that the payload has been signed by the right user

        let session_id = request.payload.ok_or_missing_field("payload")?.session_id;

        // Outbound channel: messages we will send back to *this* client.
        let (outbound_tx, outbound_rx) = mpsc::channel::<Result<RelayFrame, Status>>(8);

        let mut sessions = self.rs.sessions.lock().await;
        if let Some(pending) = sessions.remove(&session_id) {
            // Fire the peer's oneshot with our outbound sender so they can start forwarding to us.
            let _ = pending.peer_ready.send(outbound_tx.clone());
            Self::pipe_inbound_to_peer_outbound(session_id, inbound, pending.outbound_tx);
        } else {
            return Err(Status::not_found("session not found"));
        }

        let out_stream = ReceiverStream::new(outbound_rx);
        Ok(Response::new(Box::pin(out_stream) as Self::LinkClientStream))
    }
}
