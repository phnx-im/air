// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Bounded, expiring set of group ids handed out by `RequestGroupId`.

use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
    time::Duration,
};

use displaydoc::Display;
use metrics::{counter, describe_counter, describe_gauge, gauge};
use tokio::{sync::Mutex, time::Instant};
use tonic::Status;
use uuid::Uuid;

use super::ReservedGroupId;

/// How long a requested group id stays reserved. Group creation must claim
/// the id within this window.
const GROUP_ID_RESERVATION_TTL: Duration = Duration::from_secs(5 * 60);

/// Upper bound on unclaimed reservations held in memory. Requests beyond it
/// are rejected instead of growing the set.
const MAX_GROUP_ID_RESERVATIONS: usize = 100_000;

/// How often the background sweep releases expired reservations.
const GROUP_ID_RESERVATION_SWEEP_INTERVAL: Duration = Duration::from_secs(60);

const METRIC_AIR_DS_GROUP_ID_RESERVATIONS: &str = "air_ds_group_id_reservations";
const METRIC_AIR_DS_GROUP_ID_RESERVATIONS_CREATED: &str =
    "air_ds_group_id_reservations_created_total";
const METRIC_AIR_DS_GROUP_ID_RESERVATIONS_CLAIMED: &str =
    "air_ds_group_id_reservations_claimed_total";
const METRIC_AIR_DS_GROUP_ID_RESERVATIONS_EXPIRED: &str =
    "air_ds_group_id_reservations_expired_total";
const METRIC_AIR_DS_GROUP_ID_RESERVATIONS_REJECTED: &str =
    "air_ds_group_id_reservations_rejected_total";

/// Too many outstanding group id reservations
#[derive(Debug, PartialEq, Eq, thiserror::Error, Display)]
pub(crate) struct GroupIdReservationsFull;

impl From<GroupIdReservationsFull> for Status {
    fn from(error: GroupIdReservationsFull) -> Self {
        Status::resource_exhausted(format!("{error}, please try again later"))
    }
}

/// Unclaimed group ids together with their expiry.
#[derive(Debug, Default)]
pub(super) struct GroupIdReservations {
    expires_at: HashMap<Uuid, Instant>,
    by_expiry: BTreeSet<(Instant, Uuid)>,
}

impl GroupIdReservations {
    pub(super) fn describe_metrics() {
        describe_gauge!(
            METRIC_AIR_DS_GROUP_ID_RESERVATIONS,
            "Number of unclaimed group id reservations"
        );
        describe_counter!(
            METRIC_AIR_DS_GROUP_ID_RESERVATIONS_CREATED,
            "Total number of group id reservations handed out"
        );
        describe_counter!(
            METRIC_AIR_DS_GROUP_ID_RESERVATIONS_CLAIMED,
            "Total number of group id reservations consumed by group creation"
        );
        describe_counter!(
            METRIC_AIR_DS_GROUP_ID_RESERVATIONS_EXPIRED,
            "Total number of group id reservations that expired unclaimed"
        );
        describe_counter!(
            METRIC_AIR_DS_GROUP_ID_RESERVATIONS_REJECTED,
            "Total number of group id requests rejected at the reservation cap"
        );
    }

    pub(super) fn len(&self) -> usize {
        self.expires_at.len()
    }

    /// Reserves `N` fresh group ids until the TTL passes. A request that does
    /// not fit under the cap reserves nothing, so a rejected paired request
    /// strands no slot.
    pub(super) fn reserve_fresh<const N: usize>(
        &mut self,
        now: Instant,
    ) -> Result<[Uuid; N], GroupIdReservationsFull> {
        // Expired entries must not count against the cap, otherwise a burst
        // that filled the set keeps rejecting requests until the next sweep.
        self.expire(now);
        if self.len() + N > MAX_GROUP_ID_RESERVATIONS {
            counter!(METRIC_AIR_DS_GROUP_ID_RESERVATIONS_REJECTED).increment(1);
            return Err(GroupIdReservationsFull);
        }
        let expires_at = now + GROUP_ID_RESERVATION_TTL;
        let group_ids = std::array::from_fn(|_| self.reserve_one(expires_at));
        counter!(METRIC_AIR_DS_GROUP_ID_RESERVATIONS_CREATED).increment(N as u64);
        self.record_current();
        Ok(group_ids)
    }

    fn reserve_one(&mut self, expires_at: Instant) -> Uuid {
        // Generate UUIDs until we find one that is not yet reserved.
        let mut group_id = Uuid::new_v4();
        while self.expires_at.contains_key(&group_id) {
            group_id = Uuid::new_v4();
        }
        self.expires_at.insert(group_id, expires_at);
        self.by_expiry.insert((expires_at, group_id));
        group_id
    }

    /// Takes the reservation out of the set. Returns `None` when the id was
    /// never reserved, was already claimed, or expired.
    pub(super) fn claim(&mut self, group_id: Uuid, now: Instant) -> Option<ReservedGroupId> {
        self.expire(now);
        let expires_at = self.expires_at.remove(&group_id)?;
        self.by_expiry.remove(&(expires_at, group_id));
        counter!(METRIC_AIR_DS_GROUP_ID_RESERVATIONS_CLAIMED).increment(1);
        self.record_current();
        Some(ReservedGroupId(group_id))
    }

    /// Releases every reservation whose TTL passed and returns their number.
    pub(super) fn expire(&mut self, now: Instant) -> usize {
        let mut expired = 0;
        while let Some(&(expires_at, group_id)) = self.by_expiry.first() {
            if expires_at > now {
                break;
            }
            self.by_expiry.pop_first();
            self.expires_at.remove(&group_id);
            expired += 1;
        }
        if expired > 0 {
            counter!(METRIC_AIR_DS_GROUP_ID_RESERVATIONS_EXPIRED).increment(expired as u64);
            self.record_current();
        }
        expired
    }

    fn record_current(&self) {
        gauge!(METRIC_AIR_DS_GROUP_ID_RESERVATIONS).set(self.len() as f64);
    }
}

/// Releases expired reservations on a fixed interval, so that abandoned
/// reservations free their memory without waiting for the next request.
pub(super) async fn sweep_expired_group_id_reservations(
    reservations: Arc<Mutex<GroupIdReservations>>,
) {
    let mut interval = tokio::time::interval(GROUP_ID_RESERVATION_SWEEP_INTERVAL);
    loop {
        interval.tick().await;
        reservations.lock().await.expire(Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;
    use tokio_util::sync::CancellationToken;
    use tonic::Code;

    use crate::{air_service::BackendService, ds::Ds, version::VersionPolicy};

    use super::*;

    fn reserve_one(reservations: &mut GroupIdReservations, now: Instant) -> Uuid {
        let [group_id] = reservations.reserve_fresh::<1>(now).unwrap();
        group_id
    }

    fn fill(reservations: &mut GroupIdReservations, count: usize, now: Instant) {
        for _ in 0..count {
            reserve_one(reservations, now);
        }
    }

    #[test]
    fn paired_reservation_returns_distinct_ids() {
        let mut reservations = GroupIdReservations::default();
        let [first, second] = reservations.reserve_fresh::<2>(Instant::now()).unwrap();

        assert_ne!(first, second);
        assert_eq!(reservations.len(), 2);
    }

    #[test]
    fn claim_consumes_reservation() {
        let mut reservations = GroupIdReservations::default();
        let now = Instant::now();
        let group_id = reserve_one(&mut reservations, now);

        assert!(reservations.claim(group_id, now).is_some());
        assert!(reservations.claim(group_id, now).is_none());
        assert_eq!(reservations.len(), 0);
    }

    #[test]
    fn claim_rejects_unknown_id() {
        let mut reservations = GroupIdReservations::default();
        assert!(reservations.claim(Uuid::new_v4(), Instant::now()).is_none());
    }

    #[test]
    fn claim_rejects_expired_reservation() {
        let mut reservations = GroupIdReservations::default();
        let now = Instant::now();
        let group_id = reserve_one(&mut reservations, now);

        let later = now + GROUP_ID_RESERVATION_TTL;
        assert!(reservations.claim(group_id, later).is_none());
        assert_eq!(reservations.len(), 0);
    }

    #[test]
    fn claim_just_before_expiry_succeeds() {
        let mut reservations = GroupIdReservations::default();
        let now = Instant::now();
        let group_id = reserve_one(&mut reservations, now);

        let almost = now + GROUP_ID_RESERVATION_TTL - Duration::from_millis(1);
        assert!(reservations.claim(group_id, almost).is_some());
    }

    #[test]
    fn expire_releases_only_expired_entries() {
        let mut reservations = GroupIdReservations::default();
        let start = Instant::now();
        let old = reserve_one(&mut reservations, start);
        let later = start + Duration::from_secs(1);
        let fresh = reserve_one(&mut reservations, later);

        assert_eq!(reservations.expire(start + GROUP_ID_RESERVATION_TTL), 1);
        assert_eq!(reservations.len(), 1);
        assert!(reservations.claim(old, later).is_none());
        assert!(reservations.claim(fresh, later).is_some());
        assert_eq!(reservations.expire(later + GROUP_ID_RESERVATION_TTL), 0);
    }

    #[test]
    fn reserve_rejects_at_capacity() {
        let mut reservations = GroupIdReservations::default();
        let now = Instant::now();
        fill(&mut reservations, MAX_GROUP_ID_RESERVATIONS, now);

        assert_eq!(
            reservations.reserve_fresh::<1>(now),
            Err(GroupIdReservationsFull)
        );
        assert_eq!(reservations.len(), MAX_GROUP_ID_RESERVATIONS);
    }

    #[test]
    fn paired_reservation_needs_two_free_slots() {
        let mut reservations = GroupIdReservations::default();
        let now = Instant::now();
        fill(&mut reservations, MAX_GROUP_ID_RESERVATIONS - 1, now);

        assert_eq!(
            reservations.reserve_fresh::<2>(now),
            Err(GroupIdReservationsFull)
        );
        assert_eq!(reservations.len(), MAX_GROUP_ID_RESERVATIONS - 1);

        assert!(reservations.reserve_fresh::<1>(now).is_ok());
        assert_eq!(reservations.len(), MAX_GROUP_ID_RESERVATIONS);
    }

    #[test]
    fn claim_frees_capacity() {
        let mut reservations = GroupIdReservations::default();
        let now = Instant::now();
        let group_id = reserve_one(&mut reservations, now);
        fill(&mut reservations, MAX_GROUP_ID_RESERVATIONS - 1, now);

        reservations.claim(group_id, now).unwrap();
        assert!(reservations.reserve_fresh::<1>(now).is_ok());
    }

    #[test]
    fn expiry_frees_capacity_without_sweep() {
        let mut reservations = GroupIdReservations::default();
        let now = Instant::now();
        fill(&mut reservations, MAX_GROUP_ID_RESERVATIONS, now);

        let later = now + GROUP_ID_RESERVATION_TTL;
        assert!(reservations.reserve_fresh::<1>(later).is_ok());
        assert_eq!(reservations.len(), 1);
    }

    #[test]
    fn full_error_maps_to_resource_exhausted() {
        let status = Status::from(GroupIdReservationsFull);
        assert_eq!(status.code(), Code::ResourceExhausted);
    }

    #[tokio::test(start_paused = true)]
    async fn sweep_releases_abandoned_reservations() {
        let reservations = Arc::new(Mutex::new(GroupIdReservations::default()));
        let sweep = tokio::spawn(sweep_expired_group_id_reservations(reservations.clone()));

        reserve_one(&mut *reservations.lock().await, Instant::now());
        assert_eq!(reservations.lock().await.len(), 1);

        tokio::time::sleep(GROUP_ID_RESERVATION_TTL + GROUP_ID_RESERVATION_SWEEP_INTERVAL).await;
        assert_eq!(reservations.lock().await.len(), 0);

        sweep.abort();
    }

    #[sqlx::test]
    async fn paired_request_near_capacity_reserves_nothing(pool: PgPool) -> anyhow::Result<()> {
        let ds = Ds::new_from_pool(
            pool,
            "example.com".parse()?,
            VersionPolicy::default(),
            CancellationToken::new(),
        )
        .await?;
        fill(
            &mut *ds.group_id_reservations.lock().await,
            MAX_GROUP_ID_RESERVATIONS - 1,
            Instant::now(),
        );

        assert_eq!(
            ds.request_group_ids(true).await,
            Err(GroupIdReservationsFull)
        );
        assert_eq!(
            ds.group_id_reservations.lock().await.len(),
            MAX_GROUP_ID_RESERVATIONS - 1
        );

        let (_, pq_qgid) = ds.request_group_ids(false).await?;
        assert!(pq_qgid.is_none());
        assert_eq!(
            ds.request_group_ids(false).await,
            Err(GroupIdReservationsFull)
        );
        Ok(())
    }
}
