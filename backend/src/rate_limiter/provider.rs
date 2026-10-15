// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use tracing::error;

use super::{RlConfig, RlKey, StorageProvider};

pub(crate) const ALLOWANCE_SWEEP_INTERVAL: Duration = Duration::from_hours(1);

/// Deletes expired allowances every `period`, so keys that stop sending
/// requests do not keep their rows forever.
pub(crate) async fn sweep_expired_allowances(pool: PgPool, period: Duration) {
    let mut interval = tokio::time::interval(period);
    loop {
        interval.tick().await;
        if let Err(error) = persistence::delete_expired(&pool, Utc::now()).await {
            error!(%error, "Failed to delete expired allowances");
        }
    }
}

pub(crate) struct RlPostgresStorage {
    pool: PgPool,
}

impl RlPostgresStorage {
    pub(crate) fn new(pool: PgPool) -> Self {
        RlPostgresStorage { pool }
    }
}

impl StorageProvider for RlPostgresStorage {
    async fn try_consume(&self, config: &RlConfig, key: &RlKey, now: DateTime<Utc>) -> bool {
        // Fails open, so an unreachable database does not lock users out.
        persistence::try_consume(&self.pool, config, key, now)
            .await
            .unwrap_or_else(|error| {
                error!(%error, "Failed to consume allowance in Postgres");
                true
            })
    }
}

pub(crate) mod persistence {
    use chrono::{DateTime, Utc};
    use sqlx::{PgExecutor, query};

    use crate::{
        errors::StorageError,
        rate_limiter::{RlConfig, RlKey},
    };

    pub(super) async fn try_consume(
        connection: impl PgExecutor<'_>,
        config: &RlConfig,
        key: &RlKey,
        now: DateTime<Utc>,
    ) -> Result<bool, StorageError> {
        if config.max_requests == 0 {
            return Ok(false);
        }
        // Consumes one request from the allowance of `key` in a single
        // statement, so concurrent requests serialize on the row lock. Returns
        // whether the request is allowed.
        let consumed = query!(
            r#"INSERT INTO allowance_record AS a
                (key_value, remaining, valid_until)
                VALUES ($1, $2::bigint - 1, $4)
            ON CONFLICT (key_value) DO UPDATE SET
                remaining = CASE WHEN a.valid_until < $3
                    THEN EXCLUDED.remaining
                    ELSE a.remaining - 1 END,
                valid_until = CASE WHEN a.valid_until < $3
                    THEN EXCLUDED.valid_until
                    ELSE a.valid_until END
            WHERE a.valid_until < $3 OR a.remaining > 0
            RETURNING 1 AS "consumed!""#,
            key.serialize(),
            config.max_requests as i64,
            now,
            now + config.time_window,
        )
        .fetch_optional(connection)
        .await?;

        // A row that is not updated returns nothing, which means denied.
        Ok(consumed.is_some())
    }

    /// Delete all allowances expired at `now`.
    pub(super) async fn delete_expired(
        connection: impl PgExecutor<'_>,
        now: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        query!("DELETE FROM allowance_record WHERE valid_until < $1", now)
            .execute(connection)
            .await?;
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use std::time::Duration;

        use chrono::{SubsecRound, TimeDelta};
        use sqlx::{PgPool, query_as};

        use crate::rate_limiter::{
            RateLimiter, RlOperation,
            provider::{RlPostgresStorage, sweep_expired_allowances},
        };

        use super::*;

        const TIME_WINDOW: TimeDelta = TimeDelta::hours(1);

        /// A row of `allowance_record`.
        #[derive(Debug, PartialEq)]
        struct Allowance {
            remaining: u64,
            valid_until: DateTime<Utc>,
        }

        impl Allowance {
            fn expiring_in(time_window: TimeDelta) -> Self {
                Allowance {
                    remaining: 0,
                    // Postgres only stores microseconds
                    valid_until: (Utc::now() + time_window).round_subsecs(6),
                }
            }

            async fn load(pool: &PgPool, key: &RlKey) -> Option<Self> {
                struct AllowanceRecord {
                    remaining: i64,
                    valid_until: DateTime<Utc>,
                }

                let record = query_as!(
                    AllowanceRecord,
                    r#"SELECT
                        remaining AS "remaining: _",
                        valid_until AS "valid_until: _"
                    FROM allowance_record
                    WHERE key_value = $1"#,
                    key.serialize(),
                )
                .fetch_optional(pool)
                .await
                .unwrap();
                record.map(|record| Allowance {
                    remaining: record.remaining as u64,
                    valid_until: record.valid_until,
                })
            }

            async fn store(&self, pool: &PgPool, key: &RlKey) {
                query!(
                    "INSERT INTO allowance_record
                        (key_value, remaining, valid_until)
                        VALUES ($1, $2, $3)",
                    key.serialize(),
                    self.remaining as i64,
                    self.valid_until,
                )
                .execute(pool)
                .await
                .unwrap();
            }
        }

        fn limiter(pool: &PgPool, max_requests: u64) -> RateLimiter<RlPostgresStorage> {
            let config = RlConfig {
                max_requests,
                time_window: TIME_WINDOW,
            };
            RateLimiter::new(config, RlPostgresStorage::new(pool.clone()))
        }

        fn test_key(custom: &[u8]) -> RlKey {
            RlKey::new(RlOperation::ReserveGroupId, &[custom])
        }

        #[sqlx::test]
        async fn delete_expired_allowances(pool: PgPool) {
            let live_key = test_key(b"live");
            let live = Allowance::expiring_in(TimeDelta::hours(1));
            live.store(&pool, &live_key).await;

            let expired_key = test_key(b"expired");
            Allowance::expiring_in(-TimeDelta::weeks(1))
                .store(&pool, &expired_key)
                .await;

            delete_expired(&pool, Utc::now()).await.unwrap();

            assert_eq!(Allowance::load(&pool, &live_key).await, Some(live));
            assert_eq!(Allowance::load(&pool, &expired_key).await, None);
        }

        #[sqlx::test]
        async fn requests_over_the_limit_are_denied(pool: PgPool) {
            const LIMIT: u64 = 3;
            let rl = limiter(&pool, LIMIT);
            let key = test_key(b"test");
            let now = Utc::now();

            for _ in 0..LIMIT {
                assert!(rl.allowed(key.clone(), now).await);
            }
            assert!(!rl.allowed(key, now).await);
        }

        #[sqlx::test]
        async fn concurrent_requests_spend_the_allowance_once_each(pool: PgPool) {
            const REQUESTS: usize = 20;
            const LIMIT: u64 = 5;
            let key = test_key(b"test");
            let now = Utc::now();

            let mut requests = Vec::with_capacity(REQUESTS);
            for _ in 0..REQUESTS {
                let rl = limiter(&pool, LIMIT);
                let key = key.clone();
                requests.push(tokio::spawn(async move { rl.allowed(key, now).await }));
            }

            let mut allowed: u64 = 0;
            for request in requests {
                if request.await.expect("request task panicked") {
                    allowed += 1;
                }
            }
            assert_eq!(allowed, LIMIT);
        }

        #[sqlx::test]
        async fn a_stale_window_starts_over(pool: PgPool) {
            let key = test_key(b"test");
            let rl = limiter(&pool, 1);
            let now = Utc::now();
            assert!(rl.allowed(key.clone(), now).await);
            assert!(!rl.allowed(key.clone(), now).await);

            let later = now + TIME_WINDOW + TimeDelta::seconds(1);
            assert!(rl.allowed(key.clone(), later).await);
            assert!(!rl.allowed(key, later).await);
        }

        #[sqlx::test]
        async fn the_sweep_deletes_expired_allowances(pool: PgPool) {
            let key = test_key(b"test");
            Allowance::expiring_in(-TimeDelta::minutes(1))
                .store(&pool, &key)
                .await;

            let sweep = tokio::spawn(sweep_expired_allowances(
                pool.clone(),
                Duration::from_millis(10),
            ));
            tokio::time::timeout(Duration::from_secs(5), async {
                while Allowance::load(&pool, &key).await.is_some() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("the sweep did not delete the expired allowance");
            sweep.abort();
        }
    }
}
