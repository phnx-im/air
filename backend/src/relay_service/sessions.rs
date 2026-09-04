// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The relay's session table: rendezvous ID assignment, quarantine and the
//! time-to-live reaper.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Duration,
};

use aircommon::identifiers::QsUserId;
use airprotos::relay_service::v1::RelayFrame;
use chrono::TimeDelta;
use rand::RngExt;
use tokio::{
    sync::{mpsc, oneshot},
    time::Instant,
};
use tokio_util::sync::CancellationToken;
use tonic::Status;
use tracing::{info, warn};

use aircommon::crypto::mdl::code::MIN_RENDEZVOUS_DIGITS;

use crate::{
    client_ip::IpBucket,
    rate_limiter::{RlConfig, RlKey, provider::RlMemoryStorage},
    settings::RelaySettings,
};

/// Service name the relay's rate-limiter keys are scoped under.
const RL_SERVICE: &[u8] = b"rs";
const RL_PROVISION: &[u8] = b"multi_device_provision_client";
const RL_LINK: &[u8] = b"multi_device_link_client";

/// Frames going out to one peer of a session.
pub(crate) type Outbound = mpsc::Sender<Result<RelayFrame, Status>>;

/// A session's rendezvous ID: a decimal string the relay assigns.
pub type SessionId = String;

/// Digits the relay starts assigning rendezvous IDs at.
const INITIAL_WIDTH: u32 = MIN_RENDEZVOUS_DIGITS as u32;

/// Widest rendezvous ID the relay assigns. A billion concurrent sessions is
/// far past what a single replica serves, so reaching this means something
/// else is wrong.
const MAX_WIDTH: u32 = 9;

/// Random draws at one width before giving up on it and widening.
const MAX_DRAWS_PER_WIDTH: u32 = 32;

/// The reaper sweeps a few times per session lifetime, so an idle session
/// is torn down at most a fraction of its lifetime late. The lower bound
/// only keeps a degenerate lifetime from turning the sweep into a busy loop.
const REAP_DIVISOR: u32 = 4;
const MIN_REAP_INTERVAL: Duration = Duration::from_millis(20);
const MAX_REAP_INTERVAL: Duration = Duration::from_secs(30);

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
    /// When the reaper tears this session down.
    expires_at: Instant,
    /// Cancels both peers' forwarding tasks.
    cancel: CancellationToken,
}

struct Table {
    live: HashMap<SessionId, Session>,
    /// Ended IDs and the instant they may be assigned again. A user typing a
    /// stale code must not consume an unrelated fresh session.
    quarantine: HashMap<SessionId, Instant>,
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
    fn assign(&mut self) -> Option<SessionId> {
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
                let candidate = format!("{n:0width$}");
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
    fn retire(&mut self, id: SessionId, quarantine_until: Instant) -> Option<Session> {
        let session = self.live.remove(&id);
        self.quarantine.insert(id, quarantine_until);
        session
    }

    /// IDs of the current width that are unavailable.
    fn occupancy(&self) -> u64 {
        let width = self.width as usize;
        let of_width = |id: &&SessionId| id.len() == width;
        let live = self.live.keys().filter(of_width).count();
        let quarantined = self.quarantine.keys().filter(of_width).count();
        (live + quarantined) as u64
    }
}

/// A peer's hold on a session. Dropping it ends the session, so every way a
/// peer's task can finish, cancellation and panics included, tears it down.
pub(crate) struct SessionGuard {
    rs: Rs,
    id: SessionId,
}

impl SessionGuard {
    pub(crate) fn id(&self) -> &str {
        &self.id
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
    allowances: RlMemoryStorage,
    settings: RelaySettings,
    stop: CancellationToken,
}

impl Rs {
    /// Creates the relay and starts its reaper.
    pub fn new(stop: CancellationToken, mut settings: RelaySettings) -> Self {
        if settings.idquarantine < settings.sessionttl {
            warn!(
                configured = ?settings.idquarantine,
                raised_to = ?settings.sessionttl,
                "the rendezvous id quarantine was shorter than the session lifetime"
            );
            settings.idquarantine = settings.sessionttl;
        }

        let rs = Self {
            table: Arc::default(),
            allowances: RlMemoryStorage::default(),
            settings,
            stop,
        };
        rs.spawn_reaper();
        rs
    }

    /// Whether this address may open another linking session.
    pub(crate) fn allow_provision(&self, ip: IpBucket) -> bool {
        self.allow(
            self.settings.perip,
            RlKey::new(RL_SERVICE, RL_PROVISION, &[b"ip", &ip]),
        )
    }

    /// Whether this address may make another link attempt.
    pub(crate) fn allow_link_from(&self, ip: IpBucket) -> bool {
        self.allow(
            self.settings.perip,
            RlKey::new(RL_SERVICE, RL_LINK, &[b"ip", &ip]),
        )
    }

    /// Whether this user may make another link attempt.
    pub(crate) fn allow_link_by(&self, qs_user_id: QsUserId) -> bool {
        self.allow(
            self.settings.peruser,
            RlKey::new(
                RL_SERVICE,
                RL_LINK,
                &[b"qs_user", qs_user_id.as_uuid().as_bytes()],
            ),
        )
    }

    fn allow(&self, max_requests: u64, key: RlKey) -> bool {
        let config = RlConfig {
            max_requests,
            time_window: TimeDelta::hours(1),
        };
        self.allowances.charge(&config, &key)
    }

    /// Opens a session for a provisioning device.
    pub(crate) fn open(
        &self,
        provisioner: Outbound,
    ) -> Option<(SessionGuard, oneshot::Receiver<Outbound>, CancellationToken)> {
        let (responder_ready_tx, responder_ready_rx) = oneshot::channel();
        let cancel = self.stop.child_token();

        let mut table = self.lock();
        let id = table.assign()?;
        table.live.insert(
            id.clone(),
            Session {
                provisioner,
                state: State::AwaitingResponder(responder_ready_tx),
                expires_at: Instant::now() + self.settings.sessionttl,
                cancel: cancel.clone(),
            },
        );
        drop(table);
        info!(rendezvous_id = %id, "opened a linking session");
        Some((self.guard(id), responder_ready_rx, cancel))
    }

    /// Attaches the single responder a session accepts.
    ///
    /// Returns the provisioner's outbound channel, the session's cancel
    /// token and the responder's guard. `None` means there is no such
    /// session, or it already has a responder, or the provisioner is gone.
    pub(crate) fn claim(
        &self,
        id: &str,
        responder: Outbound,
    ) -> Option<(Outbound, CancellationToken, SessionGuard)> {
        let mut table = self.lock();
        let session = table.live.get_mut(id)?;

        // The reaper runs on an interval, so a session can outlive its
        // deadline by a little.
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
        let (provisioner, cancel) = (session.provisioner.clone(), session.cancel.clone());
        drop(table);
        Some((provisioner, cancel, self.guard(id.to_owned())))
    }

    fn guard(&self, id: SessionId) -> SessionGuard {
        SessionGuard {
            rs: self.clone(),
            id,
        }
    }

    /// Ends a session and puts its ID into quarantine.
    fn end(&self, id: &str) {
        let quarantine_until = Instant::now() + self.settings.idquarantine;
        let session = self.lock().retire(id.to_owned(), quarantine_until);
        if let Some(session) = session {
            session.cancel.cancel();
            info!(rendezvous_id = %id, "ended a linking session");
        }
    }

    /// Whether `id` currently represents a live session.
    #[cfg(test)]
    fn is_live(&self, id: &str) -> bool {
        self.lock().live.contains_key(id)
    }

    /// Whether `id` is held back from reuse.
    #[cfg(test)]
    fn is_quarantined(&self, id: &str) -> bool {
        self.lock().quarantine.contains_key(id)
    }

    fn lock(&self) -> MutexGuard<'_, Table> {
        // A poisoned lock would mean a panic inside one of these short
        // critical sections, none of which can leave the table inconsistent.
        self.table.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Drops expired sessions and releases quarantined IDs.
    fn reap(&self) {
        let now = Instant::now();
        let quarantine_until = now + self.settings.idquarantine;
        let mut table = self.lock();

        let expired: Vec<SessionId> = table
            .live
            .iter()
            .filter(|(_, session)| session.expires_at <= now)
            .map(|(id, _)| id.clone())
            .collect();
        for id in expired {
            if let Some(session) = table.retire(id.clone(), quarantine_until) {
                session.cancel.cancel();
                warn!(rendezvous_id = %id, "linking session expired");
            }
        }

        table.quarantine.retain(|_, until| *until > now);
        drop(table);

        self.allowances.prune();
    }

    fn spawn_reaper(&self) {
        let lifetime = self.settings.sessionttl.min(self.settings.idquarantine);
        let interval = (lifetime / REAP_DIVISOR).clamp(MIN_REAP_INTERVAL, MAX_REAP_INTERVAL);

        let rs = self.clone();
        tokio::spawn(self.stop.clone().run_until_cancelled_owned(async move {
            loop {
                tokio::time::sleep(interval).await;
                rs.reap();
            }
        }));
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

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

    /// Opens a session and keeps both channel ends alive for the caller.
    struct Opened {
        id: SessionId,
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
            id: session.id().to_owned(),
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
        let ids: HashSet<&str> = sessions.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids.len(), 3, "ids must be pairwise distinct: {ids:?}");
        for id in ids {
            assert_eq!(id.len(), 3, "{id}");
            assert!(id.bytes().all(|b| b.is_ascii_digit()), "{id}");
        }
    }

    /// Moves the paused clock past `deadline` and lets the reaper sweep.
    async fn advance_past(deadline: Duration) {
        tokio::task::yield_now().await;
        tokio::time::advance(deadline + MAX_REAP_INTERVAL).await;
        tokio::task::yield_now().await;
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

        advance_past(long()).await;
        assert!(!rs.is_quarantined(&first.id));
    }

    #[tokio::test]
    async fn the_width_grows_past_half_occupancy() {
        let rs = relay(long(), long());
        {
            let mut table = rs.lock();
            let until = Instant::now() + long();
            for n in 0..501 {
                table.quarantine.insert(format!("{n:03}"), until);
            }
        }
        assert_eq!(open(&rs).id.len(), 4);
    }

    #[tokio::test]
    async fn a_dense_table_never_hands_out_a_taken_id() {
        let rs = relay(long(), long());
        let quarantined: HashSet<SessionId> =
            (0..900).step_by(2).map(|n| format!("{n:03}")).collect();
        {
            let mut table = rs.lock();
            let until = Instant::now() + long();
            for id in &quarantined {
                table.quarantine.insert(id.clone(), until);
            }
        }

        // Together with the quarantine this fills the width to exactly half,
        // which is the densest table that still assigns three digits.
        let sessions: Vec<Opened> = (0..50).map(|_| open(&rs)).collect();
        let ids: HashSet<&str> = sessions.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids.len(), sessions.len(), "a live id was handed out twice");
        for id in ids {
            assert_eq!(id.len(), 3, "{id}");
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

        // Reach past the deadline before the reaper has armed its first sweep,
        // so nothing but the claim itself can notice the expiry.
        tokio::time::advance(long() + Duration::from_secs(1)).await;
        assert!(rs.is_live(&session.id), "the reaper must not have run yet");

        let (responder, _rx) = mpsc::channel(8);
        assert!(rs.claim(&session.id, responder).is_none());
        assert!(!rs.is_live(&session.id));
        assert!(rs.is_quarantined(&session.id));
    }

    #[tokio::test]
    async fn the_quarantine_is_raised_to_the_session_lifetime() {
        let rs = Rs::new(
            CancellationToken::new(),
            RelaySettings {
                sessionttl: Duration::from_secs(600),
                idquarantine: Duration::from_secs(1),
                ..RelaySettings::default()
            },
        );
        assert_eq!(rs.settings.idquarantine, Duration::from_secs(600));

        let rs = Rs::new(
            CancellationToken::new(),
            RelaySettings {
                sessionttl: Duration::from_secs(600),
                idquarantine: Duration::from_secs(3600),
                ..RelaySettings::default()
            },
        );
        assert_eq!(
            rs.settings.idquarantine,
            Duration::from_secs(3600),
            "a longer quarantine must be left alone"
        );
    }

    #[tokio::test]
    async fn claiming_an_unknown_id_fails() {
        let rs = relay(long(), long());
        let (responder, _rx) = mpsc::channel(8);
        assert!(rs.claim("999", responder).is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn the_reaper_expires_and_quarantines_a_session() {
        let rs = relay(long(), long());
        let session = open(&rs);

        advance_past(long()).await;

        assert!(!rs.is_live(&session.id));
        assert!(rs.is_quarantined(&session.id));
        assert!(session.cancel.is_cancelled());

        let (responder, _rx) = mpsc::channel(8);
        assert!(rs.claim(&session.id, responder).is_none());
    }
}

#[cfg(test)]
mod rate_limit_tests {
    use std::net::{IpAddr, Ipv4Addr};

    use crate::client_ip::ClientIp;

    use super::*;

    fn relay(perip: u64, peruser: u64) -> Rs {
        Rs::new(
            CancellationToken::new(),
            RelaySettings {
                perip,
                peruser,
                ..RelaySettings::default()
            },
        )
    }

    fn bucket(last: u8) -> IpBucket {
        ClientIp::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, last))).bucket()
    }

    #[tokio::test]
    async fn provisioning_is_capped_per_address() {
        let rs = relay(2, 100);
        assert!(rs.allow_provision(bucket(1)));
        assert!(rs.allow_provision(bucket(1)));
        assert!(!rs.allow_provision(bucket(1)));
        assert!(
            rs.allow_provision(bucket(2)),
            "another address has its own allowance"
        );
    }

    #[tokio::test]
    async fn link_attempts_are_capped_per_user() {
        let rs = relay(100, 2);
        let user = QsUserId::random();
        assert!(rs.allow_link_by(user));
        assert!(rs.allow_link_by(user));
        assert!(!rs.allow_link_by(user));
        assert!(
            rs.allow_link_by(QsUserId::random()),
            "another user has its own allowance"
        );
    }

    #[tokio::test]
    async fn link_attempts_are_capped_per_address() {
        let rs = relay(2, 100);
        assert!(rs.allow_link_from(bucket(1)));
        assert!(rs.allow_link_from(bucket(1)));
        assert!(!rs.allow_link_from(bucket(1)));
        assert!(
            rs.allow_link_from(bucket(2)),
            "another address has its own allowance"
        );
    }

    #[tokio::test]
    async fn the_address_charge_does_not_touch_the_user_allowance() {
        let rs = relay(100, 1);
        let user = QsUserId::random();

        for _ in 0..10 {
            assert!(rs.allow_link_from(bucket(1)));
        }

        assert!(
            rs.allow_link_by(user),
            "the user's single attempt must survive unverified requests"
        );
    }

    #[tokio::test]
    async fn the_two_rpcs_do_not_share_an_allowance() {
        let rs = relay(1, 100);
        assert!(rs.allow_provision(bucket(1)));
        assert!(rs.allow_link_from(bucket(1)));
    }
}
