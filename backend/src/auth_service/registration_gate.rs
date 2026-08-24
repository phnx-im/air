// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The adaptive registration gate.
//!
//! Two counters decide whether a registration must answer a challenge, each
//! counting what suits its threat model.
//!
//! Each counter carries a list of independent windows, and reaching any one of
//! them closes it. A short window catches a burst, a long one catches the slow
//! trickle that stays under it.
//!
//! Per client address bucket, the gate counts registration attempts. Abuse
//! then only closes the abuser's own bucket, and the gap between reading the
//! count and recording the attempt is one short locked transaction.
//!
//! Across the deployment, the gate counts completed challenge-free
//! registrations. Counting attempts there would let a cheap flood of malformed
//! requests close public registration for everyone. Only unvouched signups
//! advance that counter, so the threshold reads as "at most N unvouched
//! accounts per window" and an invite-driven signup wave does not close the
//! door on everyone else.
//!
//! Reading the counters fails closed: a database error requires a challenge
//! rather than admitting the request. Degradation is graceful, because the
//! challenge path still admits users.
//!
//! The `air_registration_gate_open` gauge answers one deployment-scoped
//! question, "would a fresh address be admitted right now", so only the policy
//! and the deployment-wide counter move it. A single abusive bucket would
//! otherwise report the deployment as closed.

use chrono::Duration;
use hmac::{Hmac, KeyInit, Mac};
use metrics::gauge;
use sha2::Sha256;
use sqlx::{PgConnection, PgExecutor, PgPool, PgTransaction};
use tracing::error;

use crate::{
    client_ip::{ClientIp, IpBucket},
    settings::{RegistrationPolicy, RegistrationSettings, RegistrationThreshold},
};

/// The key that registration address buckets are hashed under.
///
/// Registrant addresses are never stored. The per-address counter only has to
/// tell two buckets apart inside one window, which a keyed hash does, and rows
/// are pruned once they age out of that window.
#[derive(Clone)]
pub(crate) struct IpBucketKey([u8; 32]);

/// Domain separation, so the same key can never produce a value that means
/// something else somewhere else.
const BUCKET_LABEL: &[u8] = b"AirRegistrationIpBucket";

impl IpBucketKey {
    /// The stored form of an address bucket.
    fn hash(&self, bucket: &IpBucket) -> [u8; 32] {
        let mut mac =
            Hmac::<Sha256>::new_from_slice(&self.0).expect("HMAC accepts a key of any length");
        mac.update(BUCKET_LABEL);
        mac.update(bucket);
        mac.finalize().into_bytes().into()
    }
}

impl std::fmt::Debug for IpBucketKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("IpBucketKey(redacted)")
    }
}

/// Class of the advisory lock that serializes attempt counting. The two-int
/// form of `pg_advisory_xact_lock` shares one namespace across the database, so
/// the class separates these locks from any other advisory lock use.
const ATTEMPT_LOCK_CLASS: i32 = 0x4154_4d50;

/// The lock object id for an address bucket, taken from its hash.
///
/// Two buckets that collide here only serialize against each other for the
/// length of one insert, so four bytes are enough.
fn bucket_lock_key(bucket: &[u8; 32]) -> i32 {
    i32::from_be_bytes([bucket[0], bucket[1], bucket[2], bucket[3]])
}

/// Whether a registration request has to answer a challenge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GateDecision {
    Open,
    ChallengeRequired(GateReason),
}

impl GateDecision {
    pub(crate) fn challenge_required(self) -> bool {
        matches!(self, Self::ChallengeRequired(_))
    }
}

/// Why the gate is closed. Reported as a metric label, which is why it carries
/// no address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GateReason {
    /// The configured policy always requires a challenge.
    Policy,
    /// This client address bucket reached its attempt threshold.
    PerIpThreshold,
    /// The deployment reached its threshold.
    TotalThreshold,
    /// The counters could not be read.
    CountersUnavailable,
    /// The request arrived without a resolvable client address, so the per-address
    /// counter cannot apply to it.
    AddressUnknown,
}

impl GateReason {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Policy => "policy",
            Self::PerIpThreshold => "per_ip_threshold",
            Self::TotalThreshold => "total_threshold",
            Self::CountersUnavailable => "counters_unavailable",
            Self::AddressUnknown => "address_unknown",
        }
    }
}

/// The gate, as configured and keyed for one deployment.
#[derive(Debug, Clone)]
pub(crate) struct RegistrationGate {
    settings: RegistrationSettings,
    bucket_key: IpBucketKey,
}

impl RegistrationGate {
    pub(crate) fn new(settings: RegistrationSettings, bucket_key: IpBucketKey) -> Self {
        Self {
            settings,
            bucket_key,
        }
    }

    pub(crate) fn settings(&self) -> &RegistrationSettings {
        &self.settings
    }

    pub(crate) fn set_settings(&mut self, settings: RegistrationSettings) {
        self.settings = settings;
    }

    /// How long each counter's rows stay relevant to it.
    ///
    /// A counter keeps its rows for its longest window, and a counter without
    /// windows keeps nothing.
    fn retention(&self) -> RegistrationRetention {
        RegistrationRetention {
            attempts: longest_window(&self.settings.perip),
            completions: longest_window(&self.settings.total),
        }
    }

    /// Reads the counters and decides, without recording anything.
    ///
    /// Answering a question about the gate must not move it, or a client could
    /// close its own gate by asking.
    pub(crate) async fn decide(&self, pool: &PgPool, client_ip: Option<ClientIp>) -> GateDecision {
        let bucket = match self.bucket_to_count(client_ip) {
            Ok(bucket) => bucket,
            Err(decision) => return decision,
        };

        self.decide_on(self.read_counts(pool, &bucket).await)
    }

    /// Records the attempt and decides.
    ///
    /// The count and the insert share one short transaction under a per-bucket
    /// advisory lock, so concurrent attempts from one bucket cannot all read
    /// the same count.
    pub(crate) async fn admit(&self, pool: &PgPool, client_ip: Option<ClientIp>) -> GateDecision {
        let bucket = match self.bucket_to_count(client_ip) {
            Ok(bucket) => bucket,
            Err(decision) => return decision,
        };

        self.decide_on(self.count_and_record_attempt(pool, &bucket).await)
    }

    /// Republishes the deployment view without a request behind it, so the
    /// gauge does not sit on the last registrant's reading.
    pub(crate) async fn refresh_gauge(&self, pool: &PgPool) {
        match self.settings.policy {
            RegistrationPolicy::Open => report_gate_open(true),
            RegistrationPolicy::Required => report_gate_open(false),
            RegistrationPolicy::Adaptive => match self.total_counts(pool).await {
                Ok(total) => report_gate_open(!reached(&self.settings.total, &total)),
                Err(error) => {
                    error!(%error, "failed to count registrations, reporting a closed gate");
                    report_gate_open(false);
                }
            },
        }
    }

    /// Drops rows that no window of their counter can still see.
    pub(crate) async fn prune(&self, pool: &PgPool) -> sqlx::Result<u64> {
        let retention = self.retention();

        let attempts = sqlx::query!(
            "DELETE FROM registration_attempt
            WHERE created_at < now() - ($1::BIGINT * INTERVAL '1 second')",
            retention.attempts.num_seconds(),
        )
        .execute(pool)
        .await?;

        let completions = sqlx::query!(
            "DELETE FROM registration_record
            WHERE created_at < now() - ($1::BIGINT * INTERVAL '1 second')",
            retention.completions.num_seconds(),
        )
        .execute(pool)
        .await?;

        Ok(attempts.rows_affected() + completions.rows_affected())
    }

    /// Counts a completed challenge-free registration.
    ///
    /// Runs inside the registration transaction, so a registration that does
    /// not complete does not count. Concurrent registrations can overshoot the
    /// deployment-wide threshold by the number in flight, which the per-address
    /// counter bounds per bucket.
    pub(crate) async fn record_completion(&self, txn: &mut PgTransaction<'_>) -> sqlx::Result<()> {
        sqlx::query!("INSERT INTO registration_record DEFAULT VALUES")
            .execute(txn.as_mut())
            .await?;
        Ok(())
    }

    /// The bucket to count under, or the decision the policy reaches without
    /// touching the counters at all.
    ///
    /// The policy decides the deployment view on its own, so those two arms
    /// publish it. A missing address says nothing about the deployment.
    fn bucket_to_count(&self, client_ip: Option<ClientIp>) -> Result<[u8; 32], GateDecision> {
        match self.settings.policy {
            RegistrationPolicy::Open => {
                report_gate_open(true);
                return Err(GateDecision::Open);
            }
            RegistrationPolicy::Required => {
                report_gate_open(false);
                return Err(GateDecision::ChallengeRequired(GateReason::Policy));
            }
            RegistrationPolicy::Adaptive => {}
        }

        // Without an address the per-address threshold cannot be applied, and
        // treating the request as if it were the first from a fresh bucket
        // would hand an attacker the open door by stripping a header.
        let Some(client_ip) = client_ip else {
            return Err(GateDecision::ChallengeRequired(GateReason::AddressUnknown));
        };

        Ok(self.bucket_key.hash(&client_ip.bucket()))
    }

    /// Publishes what the counts say about the deployment, then decides for the
    /// request that produced them.
    fn decide_on(&self, counts: sqlx::Result<Counts>) -> GateDecision {
        let counts = match counts {
            Ok(counts) => counts,
            Err(error) => {
                error!(%error, "failed to count registrations, requiring a challenge");
                report_gate_open(false);
                return GateDecision::ChallengeRequired(GateReason::CountersUnavailable);
            }
        };

        report_gate_open(!reached(&self.settings.total, &counts.total));
        self.verdict(&counts)
    }

    fn verdict(&self, counts: &Counts) -> GateDecision {
        if reached(&self.settings.perip, &counts.per_ip) {
            GateDecision::ChallengeRequired(GateReason::PerIpThreshold)
        } else if reached(&self.settings.total, &counts.total) {
            GateDecision::ChallengeRequired(GateReason::TotalThreshold)
        } else {
            GateDecision::Open
        }
    }

    async fn count_and_record_attempt(
        &self,
        pool: &PgPool,
        bucket: &[u8; 32],
    ) -> sqlx::Result<Counts> {
        let mut txn = pool.begin().await?;

        sqlx::query!(
            "SELECT pg_advisory_xact_lock($1, $2)",
            ATTEMPT_LOCK_CLASS,
            bucket_lock_key(bucket),
        )
        .execute(&mut *txn)
        .await?;

        // Counted before the insert, so a limit of N admits N per window.
        let counts = self.counts(&mut txn, bucket).await?;

        // Recorded whether or not the counts admit it. A caller already over
        // its limit must not earn free retries by hammering.
        sqlx::query!(
            "INSERT INTO registration_attempt (ip_bucket) VALUES ($1)",
            bucket.as_slice(),
        )
        .execute(&mut *txn)
        .await?;

        txn.commit().await?;

        Ok(counts)
    }

    async fn read_counts(&self, pool: &PgPool, bucket: &[u8; 32]) -> sqlx::Result<Counts> {
        let mut connection = pool.acquire().await?;
        self.counts(&mut connection, bucket).await
    }

    /// Both counters, each read over its own windows.
    ///
    /// One connection rather than two, so the two readings sit as close
    /// together in time as two statements can.
    async fn counts(
        &self,
        connection: &mut PgConnection,
        bucket: &[u8; 32],
    ) -> sqlx::Result<Counts> {
        Ok(Counts {
            per_ip: self.per_ip_counts(&mut *connection, bucket).await?,
            total: self.total_counts(&mut *connection).await?,
        })
    }

    /// Attempts from this bucket, one count per configured window.
    async fn per_ip_counts(
        &self,
        executor: impl PgExecutor<'_>,
        bucket: &[u8; 32],
    ) -> sqlx::Result<Vec<u64>> {
        let windows = window_seconds(&self.settings.perip);

        // Counting the joined column, so a window that matches nothing counts
        // zero rather than the one row the outer join leaves it.
        let counts = sqlx::query_scalar!(
            r#"
                SELECT COUNT(a.created_at) AS "count!"
                FROM unnest($2::BIGINT[]) WITH ORDINALITY AS w(win, ord)
                LEFT JOIN registration_attempt a
                  ON a.ip_bucket = $1
                 AND a.created_at >= now() - w.win * INTERVAL '1 second'
                GROUP BY w.ord
                ORDER BY w.ord
            "#,
            bucket.as_slice(),
            &windows,
        )
        .fetch_all(executor)
        .await?;

        Ok(counts.into_iter().map(saturating_count).collect())
    }

    /// Challenge-free registrations across the deployment, one count per
    /// configured window.
    async fn total_counts(&self, executor: impl PgExecutor<'_>) -> sqlx::Result<Vec<u64>> {
        let windows = window_seconds(&self.settings.total);

        let counts = sqlx::query_scalar!(
            r#"
                SELECT COUNT(r.created_at) AS "count!"
                FROM unnest($1::BIGINT[]) WITH ORDINALITY AS w(win, ord)
                LEFT JOIN registration_record r
                  ON r.created_at >= now() - w.win * INTERVAL '1 second'
                GROUP BY w.ord
                ORDER BY w.ord
            "#,
            &windows,
        )
        .fetch_all(executor)
        .await?;

        Ok(counts.into_iter().map(saturating_count).collect())
    }
}

/// One count per window of a counter, in the order the windows are configured.
struct Counts {
    per_ip: Vec<u64>,
    total: Vec<u64>,
}

/// Whether any window of a counter reached its limit.
fn reached(thresholds: &[RegistrationThreshold], counts: &[u64]) -> bool {
    debug_assert_eq!(thresholds.len(), counts.len());
    thresholds
        .iter()
        .zip(counts)
        .any(|(threshold, count)| *count >= threshold.limit)
}

fn longest_window(thresholds: &[RegistrationThreshold]) -> Duration {
    thresholds
        .iter()
        .map(|threshold| threshold.window)
        .max()
        .unwrap_or_else(Duration::zero)
}

fn window_seconds(thresholds: &[RegistrationThreshold]) -> Vec<i64> {
    thresholds
        .iter()
        .map(|threshold| threshold.window.num_seconds())
        .collect()
}

fn saturating_count(count: i64) -> u64 {
    count.try_into().unwrap_or(u64::MAX)
}

/// Publishes whether a fresh address would be admitted right now.
fn report_gate_open(open: bool) {
    gauge!("air_registration_gate_open").set(if open { 1.0 } else { 0.0 });
}

/// How long each of the gate's tables keeps its rows.
#[derive(Debug, Clone, Copy)]
struct RegistrationRetention {
    attempts: Duration,
    completions: Duration,
}

mod persistence {
    use rand::RngExt;
    use sqlx::{PgPool, query_scalar};

    use super::*;

    impl IpBucketKey {
        /// Loads the deployment's key, generating it on first start.
        ///
        /// The no-op update is what makes the insert return the existing row,
        /// so a concurrent first start cannot end up with two keys.
        pub(crate) async fn load_or_generate(pool: &PgPool) -> sqlx::Result<Self> {
            let fresh: [u8; 32] = rand::rng().random();

            let stored = query_scalar!(
                "INSERT INTO registration_ip_bucket_key (key)
                VALUES ($1)
                ON CONFLICT (singleton) DO UPDATE SET key = registration_ip_bucket_key.key
                RETURNING key",
                fresh.as_slice(),
            )
            .fetch_one(pool)
            .await?;

            let stored: [u8; 32] = stored.try_into().map_err(|_| {
                sqlx::Error::Decode("registration ip bucket key is not 32 bytes".into())
            })?;
            Ok(Self(stored))
        }
    }
}

#[cfg(test)]
mod test {
    use std::{
        net::{IpAddr, Ipv4Addr},
        sync::Arc,
    };

    use sqlx::{PgPool, postgres::PgPoolOptions};
    use tokio::{sync::Barrier, task::JoinSet};

    use aircommon::registration::ChallengeKind;

    use super::*;

    fn client_ip(last: u8) -> ClientIp {
        ClientIp::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, last)))
    }

    fn threshold(limit: u64, window: Duration) -> RegistrationThreshold {
        RegistrationThreshold { limit, window }
    }

    /// A day-long window on both counters, which no test outlives.
    fn settings(policy: RegistrationPolicy, perip: u64, total: u64) -> RegistrationSettings {
        settings_with(
            policy,
            vec![threshold(perip, Duration::days(1))],
            vec![threshold(total, Duration::days(1))],
        )
    }

    fn settings_with(
        policy: RegistrationPolicy,
        perip: Vec<RegistrationThreshold>,
        total: Vec<RegistrationThreshold>,
    ) -> RegistrationSettings {
        RegistrationSettings {
            policy,
            challenges: vec![ChallengeKind::InvitationCode],
            perip,
            total,
        }
    }

    async fn gate(
        pool: &PgPool,
        policy: RegistrationPolicy,
        perip: u64,
        total: u64,
    ) -> RegistrationGate {
        let key = IpBucketKey::load_or_generate(pool).await.unwrap();
        RegistrationGate::new(settings(policy, perip, total), key)
    }

    async fn gate_with(pool: &PgPool, settings: RegistrationSettings) -> RegistrationGate {
        let key = IpBucketKey::load_or_generate(pool).await.unwrap();
        RegistrationGate::new(settings, key)
    }

    async fn complete(gate: &RegistrationGate, pool: &PgPool) -> anyhow::Result<()> {
        let mut txn = pool.begin().await?;
        gate.record_completion(&mut txn).await?;
        txn.commit().await?;
        Ok(())
    }

    async fn attempts(pool: &PgPool) -> sqlx::Result<i64> {
        sqlx::query_scalar!(r#"SELECT COUNT(*) AS "count!" FROM registration_attempt"#)
            .fetch_one(pool)
            .await
    }

    async fn completions(pool: &PgPool) -> sqlx::Result<i64> {
        sqlx::query_scalar!(r#"SELECT COUNT(*) AS "count!" FROM registration_record"#)
            .fetch_one(pool)
            .await
    }

    #[sqlx::test]
    async fn open_policy_never_asks_and_records_nothing(pool: PgPool) -> anyhow::Result<()> {
        let gate = gate(&pool, RegistrationPolicy::Open, 0, 0).await;

        assert_eq!(
            gate.admit(&pool, Some(client_ip(1))).await,
            GateDecision::Open
        );
        assert_eq!(
            gate.decide(&pool, Some(client_ip(1))).await,
            GateDecision::Open
        );
        assert_eq!(attempts(&pool).await?, 0);
        assert_eq!(completions(&pool).await?, 0);

        Ok(())
    }

    #[sqlx::test]
    async fn required_policy_always_asks_and_records_nothing(pool: PgPool) -> anyhow::Result<()> {
        let gate = gate(&pool, RegistrationPolicy::Required, 100, 100).await;

        assert_eq!(
            gate.admit(&pool, Some(client_ip(1))).await,
            GateDecision::ChallengeRequired(GateReason::Policy)
        );
        assert_eq!(
            gate.decide(&pool, Some(client_ip(1))).await,
            GateDecision::ChallengeRequired(GateReason::Policy)
        );
        assert_eq!(attempts(&pool).await?, 0);
        assert_eq!(completions(&pool).await?, 0);

        Ok(())
    }

    #[sqlx::test]
    async fn adaptive_admits_up_to_the_attempt_limit(pool: PgPool) -> anyhow::Result<()> {
        let gate = gate(&pool, RegistrationPolicy::Adaptive, 3, 100).await;
        let ip = client_ip(1);

        for _ in 0..3 {
            assert_eq!(gate.admit(&pool, Some(ip)).await, GateDecision::Open);
        }

        assert_eq!(
            gate.admit(&pool, Some(ip)).await,
            GateDecision::ChallengeRequired(GateReason::PerIpThreshold)
        );

        Ok(())
    }

    /// The windows of a counter are independent, so the long one closes the
    /// gate while the short one has room to spare.
    #[sqlx::test]
    async fn a_longer_per_ip_window_closes_the_gate(pool: PgPool) -> anyhow::Result<()> {
        let gate = gate_with(
            &pool,
            settings_with(
                RegistrationPolicy::Adaptive,
                vec![
                    threshold(100, Duration::seconds(10)),
                    threshold(2, Duration::hours(1)),
                ],
                vec![threshold(100, Duration::days(1))],
            ),
        )
        .await;
        let ip = client_ip(1);

        for _ in 0..2 {
            assert_eq!(gate.admit(&pool, Some(ip)).await, GateDecision::Open);
        }

        assert_eq!(
            gate.admit(&pool, Some(ip)).await,
            GateDecision::ChallengeRequired(GateReason::PerIpThreshold)
        );

        Ok(())
    }

    #[sqlx::test]
    async fn attempts_are_counted_per_bucket(pool: PgPool) {
        let gate = gate(&pool, RegistrationPolicy::Adaptive, 1, 100).await;

        assert_eq!(
            gate.admit(&pool, Some(client_ip(1))).await,
            GateDecision::Open
        );
        assert_eq!(
            gate.admit(&pool, Some(client_ip(1))).await,
            GateDecision::ChallengeRequired(GateReason::PerIpThreshold)
        );
        assert_eq!(
            gate.admit(&pool, Some(client_ip(2))).await,
            GateDecision::Open
        );
    }

    /// Hammering a bucket that is already over its limit does not earn free
    /// retries once the window rolls on.
    #[sqlx::test]
    async fn attempts_over_the_limit_are_still_recorded(pool: PgPool) -> anyhow::Result<()> {
        let gate = gate(&pool, RegistrationPolicy::Adaptive, 1, 100).await;
        let ip = client_ip(1);

        assert_eq!(gate.admit(&pool, Some(ip)).await, GateDecision::Open);
        for _ in 0..2 {
            assert_eq!(
                gate.admit(&pool, Some(ip)).await,
                GateDecision::ChallengeRequired(GateReason::PerIpThreshold)
            );
        }

        assert_eq!(attempts(&pool).await?, 3);

        Ok(())
    }

    /// Six attempts racing on a limit of three admit exactly three: the count
    /// and the insert share one locked transaction, so the six cannot all read
    /// the same count.
    #[sqlx::test]
    async fn concurrent_attempts_admit_exactly_the_limit(pool: PgPool) -> anyhow::Result<()> {
        const RACERS: usize = 6;

        let gate = gate(&pool, RegistrationPolicy::Adaptive, 3, 100).await;
        let ip = client_ip(1);

        // Its own pool, so all six are in flight at once rather than queueing
        // on the test pool's connection limit.
        let racers = PgPoolOptions::new()
            .max_connections(u32::try_from(RACERS)?)
            .connect_with((*pool.connect_options()).clone())
            .await?;

        // Connecting is slow enough to stagger the racers past each other, so
        // every connection is established before any of them counts.
        let mut warm = Vec::with_capacity(RACERS);
        for _ in 0..RACERS {
            warm.push(racers.acquire().await?);
        }
        drop(warm);

        // Released together, so none of them can finish before the rest start.
        let start = Arc::new(Barrier::new(RACERS));

        let mut attempting = JoinSet::new();
        for _ in 0..RACERS {
            let gate = gate.clone();
            let racers = racers.clone();
            let start = start.clone();
            attempting.spawn(async move {
                start.wait().await;
                gate.admit(&racers, Some(ip)).await
            });
        }
        let decisions = attempting.join_all().await;

        let admitted = decisions
            .iter()
            .filter(|decision| **decision == GateDecision::Open)
            .count();
        assert_eq!(admitted, 3);
        assert_eq!(attempts(&pool).await?, 6);

        Ok(())
    }

    /// The deployment-wide counter closes the gate for a bucket that has
    /// registered nothing itself.
    #[sqlx::test]
    async fn completions_close_the_gate_deployment_wide(pool: PgPool) -> anyhow::Result<()> {
        let gate = gate(&pool, RegistrationPolicy::Adaptive, 100, 2).await;

        for _ in 0..2 {
            complete(&gate, &pool).await?;
        }

        assert_eq!(
            gate.admit(&pool, Some(client_ip(9))).await,
            GateDecision::ChallengeRequired(GateReason::TotalThreshold)
        );

        Ok(())
    }

    /// The deployment-wide counter's long window closes the gate while its
    /// short one has room to spare.
    #[sqlx::test]
    async fn a_longer_total_window_closes_the_gate(pool: PgPool) -> anyhow::Result<()> {
        let gate = gate_with(
            &pool,
            settings_with(
                RegistrationPolicy::Adaptive,
                vec![threshold(100, Duration::days(1))],
                vec![
                    threshold(100, Duration::seconds(60)),
                    threshold(2, Duration::hours(1)),
                ],
            ),
        )
        .await;

        for _ in 0..2 {
            complete(&gate, &pool).await?;
        }

        assert_eq!(
            gate.admit(&pool, Some(client_ip(9))).await,
            GateDecision::ChallengeRequired(GateReason::TotalThreshold)
        );

        Ok(())
    }

    /// A counter without windows never closes the gate, but the attempts it
    /// does not count are still recorded.
    #[sqlx::test]
    async fn counters_without_windows_never_close_the_gate(pool: PgPool) -> anyhow::Result<()> {
        let gate = gate_with(
            &pool,
            settings_with(RegistrationPolicy::Adaptive, Vec::new(), Vec::new()),
        )
        .await;
        let ip = client_ip(1);

        for _ in 0..5 {
            complete(&gate, &pool).await?;
            assert_eq!(gate.admit(&pool, Some(ip)).await, GateDecision::Open);
        }

        assert_eq!(attempts(&pool).await?, 5);

        Ok(())
    }

    #[sqlx::test]
    async fn rolled_back_registrations_do_not_count(pool: PgPool) -> anyhow::Result<()> {
        let gate = gate(&pool, RegistrationPolicy::Adaptive, 100, 1).await;

        let mut txn = pool.begin().await?;
        gate.record_completion(&mut txn).await?;
        txn.rollback().await?;

        assert_eq!(completions(&pool).await?, 0);
        assert_eq!(
            gate.admit(&pool, Some(client_ip(1))).await,
            GateDecision::Open
        );

        Ok(())
    }

    /// Asking about the gate must not move it, or a client could close its own
    /// gate by asking.
    #[sqlx::test]
    async fn decide_records_nothing(pool: PgPool) -> anyhow::Result<()> {
        let gate = gate(&pool, RegistrationPolicy::Adaptive, 1, 100).await;
        let ip = client_ip(1);

        for _ in 0..3 {
            assert_eq!(gate.decide(&pool, Some(ip)).await, GateDecision::Open);
        }

        assert_eq!(attempts(&pool).await?, 0);
        assert_eq!(gate.admit(&pool, Some(ip)).await, GateDecision::Open);

        Ok(())
    }

    #[sqlx::test]
    async fn adaptive_asks_without_an_address(pool: PgPool) -> anyhow::Result<()> {
        let gate = gate(&pool, RegistrationPolicy::Adaptive, 100, 100).await;

        assert_eq!(
            gate.admit(&pool, None).await,
            GateDecision::ChallengeRequired(GateReason::AddressUnknown)
        );
        assert_eq!(
            gate.decide(&pool, None).await,
            GateDecision::ChallengeRequired(GateReason::AddressUnknown)
        );
        assert_eq!(attempts(&pool).await?, 0);

        Ok(())
    }

    /// Counters that cannot be read require a challenge, rather than admitting
    /// the request. The challenge path still works, so this degrades to
    /// invitation-only instead of to an outage.
    #[sqlx::test]
    async fn unreadable_counters_ask_for_a_challenge(pool: PgPool) -> anyhow::Result<()> {
        let gate = gate(&pool, RegistrationPolicy::Adaptive, 100, 100).await;

        sqlx::query("DROP TABLE registration_attempt")
            .execute(&pool)
            .await?;
        sqlx::query("DROP TABLE registration_record")
            .execute(&pool)
            .await?;

        assert_eq!(
            gate.admit(&pool, Some(client_ip(1))).await,
            GateDecision::ChallengeRequired(GateReason::CountersUnavailable)
        );
        assert_eq!(
            gate.decide(&pool, Some(client_ip(1))).await,
            GateDecision::ChallengeRequired(GateReason::CountersUnavailable)
        );

        Ok(())
    }

    /// Each table ages out on the longest window of its own counter, so a row
    /// that one window has left can still sit inside another.
    #[sqlx::test]
    async fn pruning_drops_only_aged_rows(pool: PgPool) -> anyhow::Result<()> {
        let gate = gate_with(
            &pool,
            settings_with(
                RegistrationPolicy::Adaptive,
                vec![
                    threshold(100, Duration::seconds(1)),
                    threshold(100, Duration::seconds(10)),
                ],
                vec![
                    threshold(100, Duration::seconds(10)),
                    threshold(100, Duration::hours(1)),
                ],
            ),
        )
        .await;

        assert_eq!(
            gate.admit(&pool, Some(client_ip(1))).await,
            GateDecision::Open
        );
        complete(&gate, &pool).await?;

        sqlx::query!(
            "INSERT INTO registration_attempt (ip_bucket, created_at)
            VALUES ($1, now() - INTERVAL '1 minute')",
            vec![0u8; 32],
        )
        .execute(&pool)
        .await?;
        sqlx::query!(
            "INSERT INTO registration_record (created_at)
            VALUES (now() - INTERVAL '1 minute'), (now() - INTERVAL '2 hours')"
        )
        .execute(&pool)
        .await?;

        let pruned = gate.prune(&pool).await?;

        // The aged attempt and the two-hour-old completion, but not the
        // minute-old completion.
        assert_eq!(pruned, 2);
        assert_eq!(attempts(&pool).await?, 1);
        assert_eq!(completions(&pool).await?, 2);

        Ok(())
    }

    #[sqlx::test]
    async fn the_bucket_key_is_stable_across_loads(pool: PgPool) -> anyhow::Result<()> {
        let first = IpBucketKey::load_or_generate(&pool).await?;
        let second = IpBucketKey::load_or_generate(&pool).await?;

        let bucket = client_ip(1).bucket();
        assert_eq!(first.hash(&bucket), second.hash(&bucket));

        Ok(())
    }
}
