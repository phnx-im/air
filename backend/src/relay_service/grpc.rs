// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::pin::Pin;

use aircommon::crypto::signatures::{keys::QsUserVerifyingKey, signable::Verifiable};
use airprotos::{
    relay_service::{
        mdl::{MDL_PROTOCOL_VERSION, MdlMessage, ProvisionRequest, SessionAssigned},
        v1::{
            LinkClientRequest, LinkClientRequestPayload, RelayFrame,
            relay_service_server::RelayService,
        },
    },
    signed::SignedRequest,
    validation::MissingFieldExt,
};
use futures_util::Stream;
use tokio::sync::mpsc;
use tokio_stream::{StreamExt, wrappers::ReceiverStream};
use tonic::{Request, Response, Status, Streaming, async_trait};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    client_ip::{ClientIp, IpBucket},
    qs::QsConnector,
    relay_service::{Rs, sessions::Outbound},
};

/// Frames a peer may fall behind by before its sender blocks. The protocol
/// is a ping-pong, so anything beyond a couple of frames is slack.
const CHANNEL_CAPACITY: usize = 8;

pub struct GrpcRs<Qep: QsConnector> {
    rs: Rs,
    qs_connector: Qep,
}

impl<Qep: QsConnector> GrpcRs<Qep> {
    pub fn new(rs: Rs, qs_connector: Qep) -> Self {
        Self { rs, qs_connector }
    }
}

/// The address bucket a request counts under.
fn ip_bucket<T>(request: &Request<T>) -> Result<IpBucket, Status> {
    request
        .extensions()
        .get::<ClientIp>()
        .map(ClientIp::bucket)
        .ok_or_else(|| {
            error!("no client address on a relay request");
            Status::internal("failed to resolve the client address")
        })
}

fn too_many_requests() -> Status {
    Status::resource_exhausted("Too many requests, please try again later")
}

/// Pipes one peer's inbound frames to the other peer.
///
/// Frames are opaque here: the relay never inspects a linking payload.
async fn forward(rendezvous_id: &str, mut inbound: Streaming<RelayFrame>, peer: Outbound) {
    while let Some(frame) = inbound.next().await {
        match frame {
            Ok(frame) => {
                if peer.send(Ok(frame)).await.is_err() {
                    break;
                }
            }
            Err(status) => {
                warn!(%rendezvous_id, %status, "inbound stream failed");
                break;
            }
        }
    }
    info!(%rendezvous_id, "peer disconnected");
}

#[async_trait]
impl<Qep: QsConnector> RelayService for GrpcRs<Qep> {
    type MultiDeviceProvisionClientStream =
        Pin<Box<dyn Stream<Item = Result<RelayFrame, Status>> + Send>>;

    async fn multi_device_provision_client(
        &self,
        request: Request<Streaming<RelayFrame>>,
    ) -> Result<Response<Self::MultiDeviceProvisionClientStream>, Status> {
        let ip = ip_bucket(&request)?;
        if !self.rs.allow_provision(ip) {
            return Err(too_many_requests());
        }

        let mut inbound = request.into_inner();

        let frame = inbound.message().await?.ok_or_else(|| {
            Status::invalid_argument("stream closed before the provisioning request")
        })?;
        let message = MdlMessage::from_frame(&frame).map_err(|error| {
            warn!(%error, "malformed linking frame");
            Status::invalid_argument("malformed linking message")
        })?;
        let MdlMessage::ProvisionRequest(ProvisionRequest { version }) = message else {
            return Err(Status::invalid_argument("expected a provisioning request"));
        };
        if version != MDL_PROTOCOL_VERSION {
            return Err(Status::failed_precondition(
                "unsupported linking protocol version",
            ));
        }

        let (outbound_tx, outbound_rx) = mpsc::channel(CHANNEL_CAPACITY);
        let (rendezvous_id, responder_ready_rx, cancel) = self
            .rs
            .open(outbound_tx.clone())
            .ok_or_else(|| Status::resource_exhausted("no rendezvous id available"))?;

        let assigned = MdlMessage::SessionAssigned(SessionAssigned {
            rendezvous_id: rendezvous_id.clone(),
        })
        .into_frame()
        .map_err(|error| {
            error!(%error, "failed to encode the session assignment");
            Status::internal("encoding failure")
        })?;
        if outbound_tx.send(Ok(assigned)).await.is_err() {
            self.rs.end(&rendezvous_id);
            return Err(Status::aborted("provisioner disconnected"));
        }

        let rs = self.rs.clone();
        tokio::spawn(cancel.run_until_cancelled_owned(async move {
            // Hold the provisioner's PAKE share until a responder attaches,
            // then hand it over as that peer's first frame.
            let buffered = match inbound.next().await {
                Some(Ok(frame)) => frame,
                Some(Err(status)) => {
                    warn!(%rendezvous_id, %status, "provisioner stream failed");
                    rs.end(&rendezvous_id);
                    return;
                }
                None => {
                    rs.end(&rendezvous_id);
                    return;
                }
            };

            let Ok(responder) = responder_ready_rx.await else {
                rs.end(&rendezvous_id);
                return;
            };

            if responder.send(Ok(buffered)).await.is_ok() {
                forward(&rendezvous_id, inbound, responder).await;
            }
            rs.end(&rendezvous_id);
        }));

        Ok(Response::new(
            Box::pin(ReceiverStream::new(outbound_rx)) as Self::MultiDeviceProvisionClientStream
        ))
    }

    type MultiDeviceLinkClientStream =
        Pin<Box<dyn Stream<Item = Result<RelayFrame, Status>> + Send>>;

    async fn multi_device_link_client(
        &self,
        request: Request<Streaming<RelayFrame>>,
    ) -> Result<Response<Self::MultiDeviceLinkClientStream>, Status> {
        let ip = ip_bucket(&request)?;
        if !self.rs.allow_link_from(ip) {
            return Err(too_many_requests());
        }
        let mut inbound = request.into_inner();

        // The first frame is the signed request, so only a registered user
        // can consume a session's single authentication attempt.
        let first_frame = inbound
            .message()
            .await?
            .ok_or_else(|| Status::invalid_argument("stream closed before LinkClientRequest"))?;

        let request: SignedRequest<LinkClientRequest> = prost::Message::decode(first_frame.payload)
            .map_err(|error| {
                error!(%error, "failed to decode initial msg");
                Status::internal("decoding failure")
            })?;

        let qs_user_id: Uuid = request
            .inner()
            .payload
            .as_ref()
            .ok_or_missing_field("payload")?
            .sender
            .ok_or_missing_field("sender")?
            .value
            .ok_or_missing_field("uuid value")?
            .into();

        let qs_user_id = aircommon::identifiers::QsUserId::from(qs_user_id);
        let qs_user_signature_key: QsUserVerifyingKey = self
            .qs_connector
            .user_verifying_key(qs_user_id)
            .await
            .map_err(|error| {
                error!(%error, "failed to load QS user signing key");
                Status::internal("internal error")
            })?
            .ok_or_else(|| Status::not_found("user not found"))?;

        let payload: LinkClientRequestPayload = request
            .verify(&qs_user_signature_key)
            .map_err(|_| Status::invalid_argument("invalid signature"))?;

        // Charged only now that the sender proved it owns the account.
        // Charging it off the unverified `sender` field would let anyone who
        // knows a `QsUserId` lock that user out of linking.
        if !self.rs.allow_link_by(qs_user_id) {
            return Err(too_many_requests());
        }

        let rendezvous_id = payload.rendezvous_id.ok_or_missing_field("rendezvous_id")?;
        if !rendezvous_id.is_well_formed() {
            return Err(Status::not_found("session not found"));
        }
        let rendezvous_id = rendezvous_id.value;

        let (outbound_tx, outbound_rx) = mpsc::channel(CHANNEL_CAPACITY);
        let Some((provisioner, cancel)) = self.rs.claim(&rendezvous_id, outbound_tx) else {
            return Err(Status::not_found("session not found"));
        };
        info!(%rendezvous_id, "attached the responder to a linking session");

        let rs = self.rs.clone();
        tokio::spawn(cancel.run_until_cancelled_owned(async move {
            forward(&rendezvous_id, inbound, provisioner).await;
            rs.end(&rendezvous_id);
        }));

        Ok(Response::new(
            Box::pin(ReceiverStream::new(outbound_rx)) as Self::MultiDeviceLinkClientStream
        ))
    }
}
