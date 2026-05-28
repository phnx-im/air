pub mod grpc;

use std::{sync::Arc, time::Duration};

use airprotos::relay_service::v1::RelayFrame;
use dashmap::DashMap;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tonic::Status;

const SESSION_TIMEOUT: Duration = Duration::from_secs(60);
const MINIMUM_CODE_LEN: usize = 8;

/// A "pending" half of a session: someone joined but their peer hasn't yet.
#[derive(Debug)]
pub(crate) struct Pending {
    /// Send a clone of this to the peer's outbound channel when they arrive.
    outbound_tx: mpsc::Sender<Result<RelayFrame, Status>>,
    /// Fires when the peer connects, delivering the peer's outbound sender
    /// so this side can forward inbound traffic to them.
    peer_ready_tx: oneshot::Sender<mpsc::Sender<Result<RelayFrame, Status>>>,
}

#[derive(Debug, Clone)]
pub struct Rs {
    sessions: Arc<DashMap<String, Pending>>,
    stop: CancellationToken,
}

impl Rs {
    pub fn new(stop: CancellationToken) -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
            stop,
        }
    }
}
