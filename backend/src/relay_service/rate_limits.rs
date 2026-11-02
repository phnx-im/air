// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Rate limits on the relay's linking RPCs.

use aircommon::identifiers::QsUserId;
use chrono::{TimeDelta, Utc};
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;

use crate::{
    bucket_key::{BucketKey, load_or_generate_ip_bucket_key},
    client_ip::IpBucket,
    qs::Qs,
    rate_limiter::{
        RateLimiter, RlConfig, RlKey, RlOperation,
        provider::{ALLOWANCE_SWEEP_INTERVAL, RlPostgresStorage, sweep_expired_allowances},
    },
    settings::RelaySettings,
};

/// Client addresses are never stored, only a keyed name for their bucket.
const BUCKET_LABEL: &[u8] = b"AirRelayIpBucket";

const TIME_WINDOW: TimeDelta = TimeDelta::hours(1);

/// Hourly allowances per client address and per user, kept in the QS database
/// so that every replica shares them.
#[derive(Debug, Clone)]
pub struct RelayLimits {
    pool: PgPool,
    bucket_key: BucketKey,
    perip: u64,
    peruser: u64,
}

impl RelayLimits {
    /// Loads the address bucket key from the QS database and starts deleting
    /// expired allowances there until `stop` is cancelled.
    pub async fn new(
        qs: &Qs,
        settings: &RelaySettings,
        stop: CancellationToken,
    ) -> sqlx::Result<Self> {
        let pool = qs.db_pool().clone();
        let bucket_key = load_or_generate_ip_bucket_key(&pool).await?;
        tokio::spawn(stop.run_until_cancelled_owned(sweep_expired_allowances(
            pool.clone(),
            ALLOWANCE_SWEEP_INTERVAL,
        )));
        Ok(Self {
            pool,
            bucket_key,
            perip: settings.perip,
            peruser: settings.peruser,
        })
    }

    /// Whether this address may open another linking session.
    pub(crate) async fn allow_provision(&self, ip: IpBucket) -> bool {
        let key = self.ip_key(RlOperation::RelayProvision, ip);
        self.allow(self.perip, key).await
    }

    /// Whether this address may make another link attempt.
    pub(crate) async fn allow_link_from(&self, ip: IpBucket) -> bool {
        let key = self.ip_key(RlOperation::RelayLink, ip);
        self.allow(self.perip, key).await
    }

    /// Whether this user may make another link attempt.
    pub(crate) async fn allow_link_by(&self, qs_user_id: QsUserId) -> bool {
        let key = RlKey::new(
            RlOperation::RelayLink,
            &[b"qs_user", qs_user_id.as_uuid().as_bytes()],
        );
        self.allow(self.peruser, key).await
    }

    fn ip_key(&self, operation: RlOperation, ip: IpBucket) -> RlKey {
        let bucket = self.bucket_key.bucket(BUCKET_LABEL, &ip);
        RlKey::new(operation, &[b"ip", &bucket])
    }

    async fn allow(&self, max_requests: u64, key: RlKey) -> bool {
        let config = RlConfig {
            max_requests,
            time_window: TIME_WINDOW,
        };
        let storage = RlPostgresStorage::new(self.pool.clone());
        RateLimiter::new(config, storage)
            .allowed(key, Utc::now())
            .await
    }
}

#[cfg(test)]
mod test {
    use std::net::{IpAddr, Ipv4Addr};

    use crate::client_ip::ClientIp;

    use super::*;

    async fn limits(pool: &PgPool, perip: u64, peruser: u64) -> RelayLimits {
        RelayLimits {
            pool: pool.clone(),
            bucket_key: load_or_generate_ip_bucket_key(pool).await.unwrap(),
            perip,
            peruser,
        }
    }

    fn bucket(last: u8) -> IpBucket {
        ClientIp::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, last))).bucket()
    }

    async fn stored_keys(pool: &PgPool) -> Vec<Vec<u8>> {
        sqlx::query_scalar!("SELECT key_value FROM allowance_record")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    #[sqlx::test]
    async fn provisioning_is_capped_per_address(pool: PgPool) {
        let limits = limits(&pool, 2, 100).await;
        assert!(limits.allow_provision(bucket(1)).await);
        assert!(limits.allow_provision(bucket(1)).await);
        assert!(!limits.allow_provision(bucket(1)).await);
        assert!(
            limits.allow_provision(bucket(2)).await,
            "another address has its own allowance"
        );
    }

    #[sqlx::test]
    async fn link_attempts_are_capped_per_address(pool: PgPool) {
        let limits = limits(&pool, 2, 100).await;
        assert!(limits.allow_link_from(bucket(1)).await);
        assert!(limits.allow_link_from(bucket(1)).await);
        assert!(!limits.allow_link_from(bucket(1)).await);
        assert!(
            limits.allow_link_from(bucket(2)).await,
            "another address has its own allowance"
        );
    }

    #[sqlx::test]
    async fn link_attempts_are_capped_per_user(pool: PgPool) {
        let limits = limits(&pool, 100, 2).await;
        let user = QsUserId::random();
        assert!(limits.allow_link_by(user).await);
        assert!(limits.allow_link_by(user).await);
        assert!(!limits.allow_link_by(user).await);
        assert!(
            limits.allow_link_by(QsUserId::random()).await,
            "another user has its own allowance"
        );
    }

    #[sqlx::test]
    async fn the_address_charge_does_not_touch_the_user_allowance(pool: PgPool) {
        let limits = limits(&pool, 100, 1).await;
        for _ in 0..10 {
            assert!(limits.allow_link_from(bucket(1)).await);
        }
        assert!(
            limits.allow_link_by(QsUserId::random()).await,
            "the user's single attempt must survive unverified requests"
        );
    }

    #[sqlx::test]
    async fn the_two_rpcs_do_not_share_an_allowance(pool: PgPool) {
        let limits = limits(&pool, 1, 100).await;
        assert!(limits.allow_provision(bucket(1)).await);
        assert!(limits.allow_link_from(bucket(1)).await);
    }

    #[sqlx::test]
    async fn stored_keys_depend_on_the_bucket_key(pool: PgPool) {
        let limits = limits(&pool, 1, 100).await;
        let unkeyed = RlKey::new(RlOperation::RelayProvision, &[b"ip", &bucket(1)]);
        assert!(limits.allow_provision(bucket(1)).await);

        let rekeyed = RelayLimits {
            bucket_key: BucketKey::random(),
            ..limits
        };
        assert!(
            rekeyed.allow_provision(bucket(1)).await,
            "another bucket key names the address differently"
        );

        let keys = stored_keys(&pool).await;
        assert_eq!(keys.len(), 2);
        assert!(!keys.iter().any(|key| key == unkeyed.serialize()));
    }
}
