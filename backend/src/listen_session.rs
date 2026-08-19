// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::pin::pin;

use futures_util::{Stream, stream::BoxStream};
use tokio_stream::{StreamExt, wrappers::ReceiverStream};
use tokio_util::sync::CancellationToken;
use tonic::{Status, Streaming};
use tracing::error;

use crate::util::StatusExt;

/// Handles a single request of a listen session.
pub(crate) trait ListenRequestHandler<Req>: Send + 'static {
    /// Processes a single request.
    ///
    /// Returning an error ends the session. The error is sent to the client as the terminal status
    /// of the response stream.
    fn handle(&mut self, request: Req) -> impl Future<Output = Result<(), Status>> + Send;
}

enum Event<Req, Resp> {
    Incoming(Req),
    Deliver(Resp),
    Evicted,
    Aborted,
}

/// Spawns a task driving a listen session over a bidirectional gRPC stream and returns its response
/// stream.
///
/// Requests are handled by `handler`, `responses` are delivered to the client as is. Both are
/// processed sequentially in a single task, biased towards requests, so that already received
/// requests are handled before shutdown or eviction ends the session.
///
/// The session ends when:
///
/// - The client half-closes the stream: all requests received before are handled, then the response
///   stream is closed with OK. Observing OK on the client means that everything sent was processed.
/// - The transport fails or the client resets the stream: the session ends abruptly, in-flight
///   requests are unconfirmed.
/// - `responses` ends: the client is told via an ABORTED status. The caller must ensure that
///   `responses` only ends when this session is supersed (evicted).
/// - `stop` is cancelled: the client is told an UNAVAILABLE status.
/// - `handler` fails: its error is terminal status.
///
/// `name` identifies the session in logs.
pub(crate) fn spawn_listen_session<Req, Resp>(
    mut requests: Streaming<Req>,
    responses: impl Stream<Item = Resp> + Send + 'static,
    stop: CancellationToken,
    mut handler: impl ListenRequestHandler<Req>,
    name: &'static str,
) -> BoxStream<'static, Result<Resp, Status>>
where
    Req: Send + 'static,
    Resp: Send + 'static,
{
    const OUT_CHANNEL_BUFFER_SIZE: usize = 16; // not too big for applying backpressure
    let (out_tx, out_rx) = tokio::sync::mpsc::channel(OUT_CHANNEL_BUFFER_SIZE);

    tokio::spawn(async move {
        let mut responses = pin!(responses);

        // Process incoming requests and responses to deliver sequentially
        loop {
            let event = tokio::select! {
                biased;
                req = requests.next() => {
                    match req {
                        // The client sent a request
                        Some(Ok(req)) => Event::Incoming(req),
                        // The transport failed or the client reset the stream. Ending abruptly,
                        // in-flight acks are lost and the corresponding messages will be
                        // redelivered.
                        Some(Err(status)) => {
                            if !status.is_client_disconnect() {
                                error!(%status, %name, "listen request stream failed");
                            }
                            return;
                        }
                        // The client half-closed the request stream. All requests sent before are
                        // processed at this point. Returning drops out_tx which closes the response
                        // with OK trailers. This is the client's confirmation that its requests are
                        // durable.
                        None => return,
                    }
                }
                // The server is shutting down
                _ = stop.cancelled() => Event::Aborted,
                resp = responses.next() => match resp {
                    // A response to deliver to the client
                    Some(resp) => Event::Deliver(resp),
                    // The stream ended (e.g. evicted by a newer listener).
                    None => Event::Evicted,
                },
            };

            match event {
                Event::Incoming(request) => {
                    if let Err(error) = handler.handle(request).await {
                        // We report the error to the client and stop.
                        error!(%error, %name, "error processing listen request");
                        let _ = out_tx.send(Err(error)).await;
                        return;
                    }
                }
                Event::Deliver(response) => {
                    if out_tx.send(Ok(response)).await.is_err() {
                        return;
                    }
                }
                Event::Evicted => {
                    let _ = out_tx.try_send(Err(Status::aborted("evicted")));
                    return;
                }
                Event::Aborted => {
                    let _ = out_tx.try_send(Err(Status::unavailable("server stopped")));
                    return;
                }
            }
        }
    });

    Box::pin(ReceiverStream::new(out_rx))
}
