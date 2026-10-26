// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The relay's session table: rendezvous ID assignment and quarantine.

use std::{collections::HashMap, sync::Arc};

use airprotos::relay_service::v1::{RelayFrame, RendezVousId};
use parking_lot::Mutex;
use rand::RngExt;
use tokio::{
    sync::{mpsc, oneshot},
    time::Instant,
};
use tokio_util::sync::CancellationToken;
use tonic::Status;
use tracing::info;

use aircommon::crypto::mdl::code::MIN_RENDEZVOUS_DIGITS;

use crate::settings::RelaySettings;

/// Frames going out to one peer of a session.
pub(crate) type Outbound = mpsc::Sender<Result<RelayFrame, Status>>;

/// Digits the relay starts assigning rendezvous IDs at.
const INITIAL_WIDTH: u32 = MIN_RENDEZVOUS_DIGITS as u32;

/// Widest rendezvous ID the relay assigns. A billion concurrent sessions is
/// far past what a single replica serves, so reaching this means something
/// else is wrong.
const MAX_WIDTH: u32 = 9;

/// Random draws at one width before giving up on it and widening.
const MAX_DRAWS_PER_WIDTH: u32 = 32;

/// How far a session has got.
enum State {
    /// The provisioner is waiting for an existing device to answer its code.
    AwaitingResponder(oneshot::Sender<Outbound>),
    /// Both peers are attached.
    Connected,
}

struct Session {
    /// Frames going to the provisioning device.
    provisioner: Outbound,
    state: State,
    /// When the session is torn down.
    expires_at: Instant,
    /// Cancels both peers' forwarding tasks.
    cancel: CancellationToken,
}

struct Table {
    live: HashMap<RendezVousId, Session>,
    /// Ended IDs and the instant they may be assigned again. A code typed just
    /// after it expired must not consume an unrelated fresh session.
    quarantine: HashMap<RendezVousId, Instant>,
    /// Digits of the IDs currently being drawn. It only ever grows, and
    /// clients never parse structure out of an ID.
    width: u32,
}

impl Default for Table {
    fn default() -> Self {
        Self {
            live: HashMap::new(),
            quarantine: HashMap::new(),
            width: INITIAL_WIDTH,
        }
    }
}

impl Table {
    /// A random ID at the current width that is neither live nor
    /// quarantined, widening when the current width is more than half full.
    fn assign(&mut self) -> Option<RendezVousId> {
        let mut rng = rand::rng();
        while self.width <= MAX_WIDTH {
            let capacity = 10u64.pow(self.width);
            if self.occupancy() * 2 > capacity {
                self.width += 1;
                continue;
            }
            let width = self.width as usize;
            for _ in 0..MAX_DRAWS_PER_WIDTH {
                let n = rng.random_range(0..capacity);
                let candidate = RendezVousId::new(format!("{n:0width$}"))
                    .expect("a zero-padded integer is a run of digits");
                if !self.live.contains_key(&candidate) && !self.quarantine.contains_key(&candidate)
                {
                    return Some(candidate);
                }
            }
            self.width += 1;
        }
        None
    }

    /// Removes a live session and holds its ID back until `quarantine_until`.
    fn retire(&mut self, id: RendezVousId, quarantine_until: Instant) -> Option<Session> {
        let session = self.live.remove(&id);
        self.quarantine.insert(id, quarantine_until);
        session
    }

    /// IDs of the current width that are unavailable.
    fn occupancy(&self) -> u64 {
        let width = self.width as usize;
        let of_width = |id: &&RendezVousId| id.value.len() == width;
        let live = self.live.keys().filter(of_width).count();
        let quarantined = self.quarantine.keys().filter(of_width).count();
        (live + quarantined) as u64
    }
}

/// A peer's hold on a session. Dropping it ends the session, so every way a
/// peer's task can finish, cancellation and panics included, tears it down.
pub(crate) struct SessionGuard {
    rs: Rs,
    id: RendezVousId,
    expires_at: Instant,
}

impl SessionGuard {
    pub(crate) fn id(&self) -> &RendezVousId {
        &self.id
    }

    /// When the session must be torn down.
    pub(crate) fn expires_at(&self) -> Instant {
        self.expires_at
    }
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        self.rs.end(&self.id);
    }
}

/// The relay's shared state.
#[derive(Clone)]
pub struct Rs {
    table: Arc<Mutex<Table>>,
    settings: RelaySettings,
    stop: CancellationToken,
}

impl Rs {
    /// Creates the relay.
    pub fn new(stop: CancellationToken, settings: RelaySettings) -> Self {
        Self {
            table: Arc::default(),
            settings,
            stop,
        }
    }

    /// Opens a session for a provisioning device.
    pub(crate) fn open(
        &self,
        provisioner: Outbound,
    ) -> Option<(SessionGuard, oneshot::Receiver<Outbound>, CancellationToken)> {
        let (responder_ready_tx, responder_ready_rx) = oneshot::channel();
        let cancel = self.stop.child_token();
        let now = Instant::now();
        let expires_at = now + self.settings.sessionttl;

        let mut table = self.table.lock();
        // Released IDs only matter when a new one is drawn.
        table.quarantine.retain(|_, until| *until > now);
        let id = table.assign()?;
        table.live.insert(
            id.clone(),
            Session {
                provisioner,
                state: State::AwaitingResponder(responder_ready_tx),
                expires_at,
                cancel: cancel.clone(),
            },
        );
        drop(table);
        info!(rendezvous_id = %id, "opened a linking session");
        Some((self.guard(id, expires_at), responder_ready_rx, cancel))
    }

    /// Attaches the single responder a session accepts.
    ///
    /// Returns the provisioner's outbound channel, the session's cancel
    /// token and the responder's guard. `None` means there is no such
    /// session, or it already has a responder, or the provisioner is gone.
    pub(crate) fn claim(
        &self,
        id: &RendezVousId,
        responder: Outbound,
    ) -> Option<(Outbound, CancellationToken, SessionGuard)> {
        let mut table = self.table.lock();
        let session = table.live.get_mut(id)?;

        // The provisioner's task tears the session down at the deadline,
        // but it may not have run yet.
        if session.expires_at <= Instant::now() {
            drop(table);
            self.end(id);
            return None;
        }

        let State::AwaitingResponder(responder_ready_tx) =
            std::mem::replace(&mut session.state, State::Connected)
        else {
            return None;
        };
        if responder_ready_tx.send(responder).is_err() {
            // The provisioner went away, so nothing would forward to us.
            drop(table);
            self.end(id);
            return None;
        }
        let provisioner = session.provisioner.clone();
        let cancel = session.cancel.clone();
        let expires_at = session.expires_at;
        drop(table);
        Some((provisioner, cancel, self.guard(id.clone(), expires_at)))
    }

    fn guard(&self, id: RendezVousId, expires_at: Instant) -> SessionGuard {
        SessionGuard {
            rs: self.clone(),
            id,
            expires_at,
        }
    }

    /// Ends a session and puts its ID into quarantine.
    fn end(&self, id: &RendezVousId) {
        let quarantine_until = Instant::now() + self.settings.idquarantine;
        let session = self.table.lock().retire(id.clone(), quarantine_until);
        if let Some(session) = session {
            session.cancel.cancel();
            info!(rendezvous_id = %id, "ended a linking session");
        }
    }

    /// Whether `id` currently represents a live session.
    #[cfg(test)]
    fn is_live(&self, id: &RendezVousId) -> bool {
        self.table.lock().live.contains_key(id)
    }

    /// Whether `id` is held back from reuse.
    #[cfg(test)]
    fn is_quarantined(&self, id: &RendezVousId) -> bool {
        self.table.lock().quarantine.contains_key(id)
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, time::Duration};

    use super::*;

    /// A relay whose deadlines are short enough for a test to wait them out.
    fn relay(ttl: Duration, quarantine: Duration) -> Rs {
        Rs::new(
            CancellationToken::new(),
            RelaySettings {
                sessionttl: ttl,
                idquarantine: quarantine,
                ..RelaySettings::default()
            },
        )
    }

    fn long() -> Duration {
        Duration::from_secs(60)
    }

    /// A well-formed ID for a test to hand the relay.
    fn id(digits: &str) -> RendezVousId {
        RendezVousId::new(digits.to_owned()).expect("test ids are digit runs")
    }

    /// Opens a session and keeps both channel ends alive for the caller.
    struct Opened {
        id: RendezVousId,
        session: SessionGuard,
        cancel: CancellationToken,
        _provisioner_rx: mpsc::Receiver<Result<RelayFrame, Status>>,
        _responder_ready_rx: oneshot::Receiver<Outbound>,
    }

    fn open(rs: &Rs) -> Opened {
        let (tx, rx) = mpsc::channel(8);
        let (session, responder_ready_rx, cancel) =
            rs.open(tx).expect("no rendezvous id available");
        Opened {
            id: session.id().clone(),
            session,
            cancel,
            _provisioner_rx: rx,
            _responder_ready_rx: responder_ready_rx,
        }
    }

    #[tokio::test]
    async fn fresh_ids_are_distinct_three_digit_numbers() {
        let rs = relay(long(), long());
        let sessions: Vec<Opened> = (0..3).map(|_| open(&rs)).collect();
        let ids: HashSet<&RendezVousId> = sessions.iter().map(|s| &s.id).collect();
        assert_eq!(ids.len(), 3, "ids must be pairwise distinct: {ids:?}");
        for id in ids {
            assert_eq!(id.value.len(), 3, "{id}");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn an_ended_id_is_reused_only_after_quarantine() {
        let rs = relay(long(), long());
        let first = open(&rs);

        drop(first.session);
        assert!(!rs.is_live(&first.id));
        assert!(rs.is_quarantined(&first.id));

        let second = open(&rs);
        assert_ne!(
            second.id, first.id,
            "a quarantined id must not be handed out"
        );

        tokio::time::advance(long() + Duration::from_secs(1)).await;
        assert!(
            rs.is_quarantined(&first.id),
            "a stale entry is only released by the next open"
        );
        let _third = open(&rs);
        assert!(!rs.is_quarantined(&first.id));
    }

    #[tokio::test]
    async fn dropping_a_guard_ends_the_session() {
        let rs = relay(long(), long());
        let session = open(&rs);

        drop(session.session);

        assert!(session.cancel.is_cancelled());
        assert!(!rs.is_live(&session.id));
        assert!(rs.is_quarantined(&session.id));
    }

    #[tokio::test]
    async fn the_width_grows_past_half_occupancy() {
        let rs = relay(long(), long());
        {
            let mut table = rs.table.lock();
            let until = Instant::now() + long();
            for n in 0..501 {
                table.quarantine.insert(id(&format!("{n:03}")), until);
            }
        }
        assert_eq!(open(&rs).id.value.len(), 4);
    }

    #[tokio::test]
    async fn a_dense_table_never_hands_out_a_taken_id() {
        let rs = relay(long(), long());
        let quarantined: HashSet<RendezVousId> = (0..900)
            .step_by(2)
            .map(|n| id(&format!("{n:03}")))
            .collect();
        {
            let mut table = rs.table.lock();
            let until = Instant::now() + long();
            for id in &quarantined {
                table.quarantine.insert(id.clone(), until);
            }
        }

        // Together with the quarantine this fills the width to exactly half,
        // which is the densest table that still assigns three digits.
        let sessions: Vec<Opened> = (0..50).map(|_| open(&rs)).collect();
        let ids: HashSet<&RendezVousId> = sessions.iter().map(|s| &s.id).collect();
        assert_eq!(ids.len(), sessions.len(), "a live id was handed out twice");
        for id in ids {
            assert_eq!(id.value.len(), 3, "{id}");
            assert!(!quarantined.contains(id), "{id} is quarantined");
        }
    }

    #[tokio::test]
    async fn only_the_first_responder_is_attached() {
        let rs = relay(long(), long());
        let session = open(&rs);

        let (first, _first_rx) = mpsc::channel(8);
        let claimed = rs.claim(&session.id, first);
        assert!(claimed.is_some());

        let (second, _second_rx) = mpsc::channel(8);
        assert!(rs.claim(&session.id, second).is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn an_expired_session_cannot_be_claimed() {
        let rs = relay(long(), long());
        let session = open(&rs);

        // The table never expires a session by itself, so only the claim can
        // notice the deadline here.
        tokio::time::advance(long() + Duration::from_secs(1)).await;
        assert!(
            rs.is_live(&session.id),
            "the table must not expire sessions"
        );

        let (responder, _rx) = mpsc::channel(8);
        assert!(rs.claim(&session.id, responder).is_none());
        assert!(!rs.is_live(&session.id));
        assert!(rs.is_quarantined(&session.id));
    }

    #[tokio::test]
    async fn claiming_an_unknown_id_fails() {
        let rs = relay(long(), long());
        let (responder, _rx) = mpsc::channel(8);
        assert!(rs.claim(&id("999"), responder).is_none());
    }
}
