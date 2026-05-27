pub mod grpc;

use std::{collections::HashMap, sync::Arc, time::Duration};

use airprotos::relay_service::v1::RelayFrame;
use rand::Rng;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tonic::Status;
use tracing::warn;

const SESSION_TIMEOUT: Duration = Duration::from_secs(30);
const SESSION_COLLISION_MAX_RETRIES: usize = 10;
const ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTUVWXYZ";
const CODE_LEN: usize = 8;

/// A "pending" half of a session: someone joined but their peer hasn't yet.
#[derive(Debug)]
pub(crate) struct Pending {
    /// Send a clone of this to the peer's outbound channel when they arrive.
    outbound_tx: mpsc::Sender<Result<RelayFrame, Status>>,
    /// Fires when the peer connects, delivering the peer's outbound sender
    /// so this side can forward inbound traffic to them.
    peer_ready: oneshot::Sender<mpsc::Sender<Result<RelayFrame, Status>>>,
}

#[derive(Debug, Clone)]
pub struct Rs {
    sessions: Arc<Mutex<HashMap<String, Pending>>>,
    stop: CancellationToken,
}

impl Rs {
    pub fn new(stop: CancellationToken) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            stop,
        }
    }
}
