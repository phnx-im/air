// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The relay's session table: rendezvous ID assignment, quarantine and the
//! time-to-live reaper.

use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Duration,
};

use aircommon::identifiers::QsUserId;
use airprotos::relay_service::v1::RelayFrame;
use chrono::TimeDelta;
use tokio::{
    sync::{mpsc, oneshot},
    time::Instant,
};
use tokio_util::sync::CancellationToken;
use tonic::Status;
use tracing::{info, warn};

use crate::{
    client_ip::IpBucket,
    rate_limiter::{RateLimiter, RlConfig, RlKey, provider::RlMemoryStorage},
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
const INITIAL_WIDTH: u32 = 3;

/// Widest rendezvous ID the relay assigns. A billion concurrent sessions is
/// far past what a single replica serves, so reaching this means something
/// else is wrong.
const MAX_WIDTH: u32 = 9;

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

#[derive(Default)]
struct Table {
    live: HashMap<SessionId, Session>,
    /// Ended IDs and the instant they may be assigned again. A user typing a
    /// stale code must not consume an unrelated fresh session.
    quarantine: HashMap<SessionId, Instant>,
    /// Digits currently being assigned. It only ever grows, and clients never
    /// parse structure out of an ID.
    width: u32,
}

impl Table {
    /// The lowest ID at the current width that is neither live nor
    /// quarantined, widening when the current width is more than half full.
    fn assign(&mut self) -> Option<SessionId> {
        while self.width <= MAX_WIDTH {
            let capacity = 10u64.pow(self.width);
            if self.occupancy() * 2 > capacity {
                self.width += 1;
                continue;
            }
            let width = self.width as usize;
            for n in 0..capacity {
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

    /// IDs of the current width that are unavailable.
    fn occupancy(&self) -> u64 {
        let width = self.width as usize;
        let of_width = |id: &&SessionId| id.len() == width;
        let live = self.live.keys().filter(of_width).count();
        let quarantined = self.quarantine.keys().filter(of_width).count();
        (live + quarantined) as u64
    }
}

impl fmt::Debug for Table {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Table")
            .field("live", &self.live.len())
            .field("quarantine", &self.quarantine.len())
            .field("width", &self.width)
            .finish()
    }
}

/// The relay's shared state.
#[derive(Clone)]
pub struct Rs {
    table: Arc<Mutex<Table>>,
    /// Allowances live in memory alongside the sessions they guard. A
    /// restart forgets them, which buys an attacker no more than waiting the
    /// window out would.
    allowances: RlMemoryStorage,
    settings: RelaySettings,
    stop: CancellationToken,
}

impl fmt::Debug for Rs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Rs")
            .field("settings", &self.settings)
            .finish_non_exhaustive()
    }
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
            table: Arc::new(Mutex::new(Table {
                width: INITIAL_WIDTH,
                ..Table::default()
            })),
            allowances: RlMemoryStorage::default(),
            settings,
            stop,
        };
        rs.spawn_reaper();
        rs
    }

    /// Whether this address may open another linking session.
    pub(crate) async fn allow_provision(&self, ip: IpBucket) -> bool {
        self.allow(
            self.settings.perip,
            RlKey::new(RL_SERVICE, RL_PROVISION, &[b"ip", &ip]),
        )
        .await
    }

    /// Whether this address and this user may make another link attempt.
    ///
    /// Both allowances are charged, the address first, so an attacker cannot
    /// spread attempts over accounts or over addresses alone.
    pub(crate) async fn allow_link(&self, ip: IpBucket, qs_user_id: QsUserId) -> bool {
        let by_ip = self
            .allow(
                self.settings.perip,
                RlKey::new(RL_SERVICE, RL_LINK, &[b"ip", &ip]),
            )
            .await;
        let by_user = self
            .allow(
                self.settings.peruser,
                RlKey::new(
                    RL_SERVICE,
                    RL_LINK,
                    &[b"qs_user", qs_user_id.as_uuid().as_bytes()],
                ),
            )
            .await;
        by_ip && by_user
    }

    async fn allow(&self, max_requests: u64, key: RlKey) -> bool {
        let config = RlConfig {
            max_requests,
            time_window: TimeDelta::hours(1),
        };
        RateLimiter::new(config, self.allowances.clone())
            .allowed(key)
            .await
    }

    /// Opens a session for a provisioning device.
    ///
    /// Returns the assigned rendezvous ID, the receiver that fires with the
    /// responder's outbound channel once one attaches, and the token that
    /// cancels the session.
    pub(crate) fn open(
        &self,
        provisioner: Outbound,
    ) -> Option<(SessionId, oneshot::Receiver<Outbound>, CancellationToken)> {
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
        info!(rendezvous_id = %id, "opened a linking session");
        Some((id, responder_ready_rx, cancel))
    }

    /// Attaches the single responder a session accepts.
    ///
    /// Returns the provisioner's outbound channel and the session's cancel
    /// token. `None` means there is no such session, or it already has a
    /// responder, or the provisioner is gone.
    pub(crate) fn claim(
        &self,
        id: &str,
        responder: Outbound,
    ) -> Option<(Outbound, CancellationToken)> {
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
        Some((session.provisioner.clone(), session.cancel.clone()))
    }

    /// Ends a session and puts its ID into quarantine.
    pub(crate) fn end(&self, id: &str) {
        let quarantine_until = Instant::now() + self.settings.idquarantine;
        let mut table = self.lock();
        if let Some(session) = table.live.remove(id) {
            session.cancel.cancel();
            info!(rendezvous_id = %id, "ended a linking session");
        }
        table.quarantine.insert(id.to_owned(), quarantine_until);
    }

    /// Whether `id` currently names a live session.
    pub fn is_live(&self, id: &str) -> bool {
        self.lock().live.contains_key(id)
    }

    /// Whether `id` is held back from reuse.
    pub fn is_quarantined(&self, id: &str) -> bool {
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
            if let Some(session) = table.live.remove(&id) {
                session.cancel.cancel();
                warn!(rendezvous_id = %id, "linking session expired");
            }
            table.quarantine.insert(id, quarantine_until);
        }

        table.quarantine.retain(|_, until| *until > now);
        drop(table);

        self.allowances.prune();
    }

    fn spawn_reaper(&self) {
        let interval = self
            .settings
            .sessionttl
            .min(self.settings.idquarantine)
            .checked_div(REAP_DIVISOR)
            .unwrap_or(MIN_REAP_INTERVAL)
            .clamp(MIN_REAP_INTERVAL, MAX_REAP_INTERVAL);

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
        cancel: CancellationToken,
        _provisioner_rx: mpsc::Receiver<Result<RelayFrame, Status>>,
        _responder_ready_rx: oneshot::Receiver<Outbound>,
    }

    fn open(rs: &Rs) -> Opened {
        let (tx, rx) = mpsc::channel(8);
        let (id, responder_ready_rx, cancel) = rs.open(tx).expect("no rendezvous id available");
        Opened {
            id,
            cancel,
            _provisioner_rx: rx,
            _responder_ready_rx: responder_ready_rx,
        }
    }

    /// Moves the paused clock past `deadline` and lets the reaper sweep.
    ///
    /// The reaper registers its timer on its first poll, so it has to run
    /// once before the clock moves, or the advance passes it by.
    async fn advance_past(deadline: Duration) {
        tokio::task::yield_now().await;
        tokio::time::advance(deadline + MAX_REAP_INTERVAL).await;
        tokio::task::yield_now().await;
    }

    #[tokio::test(start_paused = true)]
    async fn ids_start_at_three_digits_and_count_up() {
        let rs = relay(long(), long());
        let sessions: Vec<Opened> = (0..3).map(|_| open(&rs)).collect();
        let ids: Vec<&str> = sessions.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["000", "001", "002"]);
    }

    #[tokio::test]
    async fn an_ended_id_is_reused_only_after_quarantine() {
        let rs = relay(long(), long());
        let first = open(&rs);
        assert_eq!(first.id, "000");

        rs.end(&first.id);
        assert!(!rs.is_live(&first.id));
        assert!(rs.is_quarantined(&first.id));

        let second = open(&rs);
        assert_eq!(second.id, "001", "a quarantined id must not be handed out");

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
    async fn only_the_first_responder_is_attached() {
        let rs = relay(long(), long());
        let session = open(&rs);

        let (first, _first_rx) = mpsc::channel(8);
        assert!(rs.claim(&session.id, first).is_some());

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

    #[tokio::test(start_paused = true)]
    async fn claiming_an_unknown_id_fails() {
        let rs = relay(long(), long());
        let (responder, _rx) = mpsc::channel(8);
        assert!(rs.claim("999", responder).is_none());
    }

    #[tokio::test]
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
        assert!(rs.allow_provision(bucket(1)).await);
        assert!(rs.allow_provision(bucket(1)).await);
        assert!(!rs.allow_provision(bucket(1)).await);
        assert!(
            rs.allow_provision(bucket(2)).await,
            "another address has its own allowance"
        );
    }

    #[tokio::test]
    async fn link_attempts_are_capped_per_user() {
        let rs = relay(100, 2);
        let user = QsUserId::random();
        assert!(rs.allow_link(bucket(1), user).await);
        assert!(rs.allow_link(bucket(1), user).await);
        assert!(!rs.allow_link(bucket(2), user).await);
        assert!(
            rs.allow_link(bucket(1), QsUserId::random()).await,
            "another user has its own allowance"
        );
    }

    #[tokio::test]
    async fn link_attempts_are_capped_per_address() {
        let rs = relay(2, 100);
        assert!(rs.allow_link(bucket(1), QsUserId::random()).await);
        assert!(rs.allow_link(bucket(1), QsUserId::random()).await);
        assert!(!rs.allow_link(bucket(1), QsUserId::random()).await);
    }

    #[tokio::test]
    async fn the_two_rpcs_do_not_share_an_allowance() {
        let rs = relay(1, 100);
        assert!(rs.allow_provision(bucket(1)).await);
        assert!(rs.allow_link(bucket(1), QsUserId::random()).await);
    }
}
