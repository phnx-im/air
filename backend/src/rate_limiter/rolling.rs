// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Rolling-window allowances, which keep one row per charge until it ages
//! out of the window.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use tracing::error;

use super::{RlConfig, RlDecision, RlKey, StorageProvider};

/// Rate limiter storage over a rolling window. A charge counts against its key
/// for `time_window` after it was made.
pub(crate) struct RlRollingPostgresStorage {
    pool: PgPool,
}

impl RlRollingPostgresStorage {
    pub(crate) fn new(pool: PgPool) -> Self {
        RlRollingPostgresStorage { pool }
    }
}

impl StorageProvider for RlRollingPostgresStorage {
    async fn try_consume(
        &self,
        config: &RlConfig,
        key: &RlKey,
        cost: u64,
        now: DateTime<Utc>,
    ) -> RlDecision {
        // Fails open, so an unreachable database does not lock users out.
        persistence::try_consume(&self.pool, config, key, cost, now)
            .await
            .unwrap_or_else(|error| {
                error!(%error, "Failed to charge rolling allowance in Postgres");
                RlDecision::Allowed
            })
    }
}

pub(super) mod persistence {
    use chrono::{DateTime, Utc};
    use sqlx::{PgExecutor, PgPool, query, query_scalar};

    use crate::{
        errors::StorageError,
        rate_limiter::{RlConfig, RlDecision, RlKey, provider::persistence::saturating_i64},
        window_counter::bucket_lock_key,
    };

    /// The class of the advisory locks that serialize charges per key. The
    /// two-int form of `pg_advisory_xact_lock` shares one namespace across the
    /// database.
    const CHARGE_LOCK_CLASS: i32 = 0x524c_4556;

    pub(in crate::rate_limiter) async fn try_consume(
        pool: &PgPool,
        config: &RlConfig,
        key: &RlKey,
        cost: u64,
        now: DateTime<Utc>,
    ) -> Result<RlDecision, StorageError> {
        if cost > config.limit {
            return Ok(RlDecision::Denied { retry_at: None });
        }
        let limit = saturating_i64(config.limit);
        let cost = saturating_i64(cost);

        let mut transaction = pool.begin().await?;
        query!(
            "SELECT pg_advisory_xact_lock($1, $2)",
            CHARGE_LOCK_CLASS,
            bucket_lock_key(&key.key),
        )
        .execute(&mut *transaction)
        .await?;

        let used = query_scalar!(
            r#"SELECT COALESCE(SUM(cost), 0)::bigint AS "used!"
            FROM allowance_event
            WHERE key_value = $1 AND expires_at > $2"#,
            key.serialize(),
            now,
        )
        .fetch_one(&mut *transaction)
        .await?;

        let excess = used.saturating_add(cost).saturating_sub(limit);
        if excess <= 0 {
            query!(
                "INSERT INTO allowance_event (key_value, cost, expires_at)
                VALUES ($1, $2, $3)",
                key.serialize(),
                cost,
                now + config.time_window,
            )
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
            return Ok(RlDecision::Allowed);
        }

        // The charge fits once the oldest charges worth `excess` have expired.
        let retry_at = query_scalar!(
            r#"SELECT expires_at AS "expires_at!"
            FROM (
                SELECT
                    expires_at,
                    (SUM(cost) OVER (ORDER BY expires_at))::bigint AS freed
                FROM allowance_event
                WHERE key_value = $1 AND expires_at > $2
            ) AS charges
            WHERE freed >= $3
            ORDER BY expires_at
            LIMIT 1"#,
            key.serialize(),
            now,
            excess,
        )
        .fetch_optional(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(RlDecision::Denied { retry_at })
    }

    /// Deletes all charges expired at `now`.
    pub(in crate::rate_limiter) async fn delete_expired(
        connection: impl PgExecutor<'_>,
        now: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        query!("DELETE FROM allowance_event WHERE expires_at <= $1", now)
            .execute(connection)
            .await?;
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use chrono::{SubsecRound, TimeDelta};

        use crate::rate_limiter::{RateLimiter, RlOperation, rolling::RlRollingPostgresStorage};

        use super::*;

        const TIME_WINDOW: TimeDelta = TimeDelta::days(7);

        fn limiter(pool: &PgPool, limit: u64) -> RateLimiter<RlRollingPostgresStorage> {
            let config = RlConfig {
                limit,
                time_window: TIME_WINDOW,
            };
            RateLimiter::new(config, RlRollingPostgresStorage::new(pool.clone()))
        }

        fn test_key(custom: &[u8]) -> RlKey {
            RlKey::new(RlOperation::ReserveGroupId, &[custom])
        }

        /// The current time as Postgres stores it, in microseconds.
        fn now() -> DateTime<Utc> {
            Utc::now().round_subsecs(6)
        }

        async fn event_count(pool: &PgPool) -> i64 {
            query_scalar!(r#"SELECT COUNT(*) AS "count!" FROM allowance_event"#)
                .fetch_one(pool)
                .await
                .unwrap()
        }

        #[sqlx::test]
        async fn charges_over_the_limit_are_denied(pool: PgPool) {
            let rl = limiter(&pool, 10);
            let key = test_key(b"test");
            let now = now();

            assert!(rl.allowed(key.clone(), 6, now).await.is_allowed());
            assert!(rl.allowed(key.clone(), 4, now).await.is_allowed());
            assert!(!rl.allowed(key, 1, now).await.is_allowed());
            assert_eq!(event_count(&pool).await, 2);
        }

        #[sqlx::test]
        async fn charges_age_out_one_by_one(pool: PgPool) {
            let rl = limiter(&pool, 10);
            let key = test_key(b"test");
            let start = now();
            let later = start + TimeDelta::days(1);

            assert!(rl.allowed(key.clone(), 6, start).await.is_allowed());
            assert!(rl.allowed(key.clone(), 4, later).await.is_allowed());
            assert!(!rl.allowed(key.clone(), 6, later).await.is_allowed());

            // Only the first charge has expired here, which frees 6.
            let after_first = start + TIME_WINDOW;
            assert!(!rl.allowed(key.clone(), 7, after_first).await.is_allowed());
            assert!(rl.allowed(key, 6, after_first).await.is_allowed());
        }

        #[sqlx::test]
        async fn a_denial_retries_once_enough_charges_expire(pool: PgPool) {
            let rl = limiter(&pool, 10);
            let key = test_key(b"test");
            let start = now();

            for day in 0..3 {
                let at = start + TimeDelta::days(day);
                assert!(rl.allowed(key.clone(), 3, at).await.is_allowed());
            }

            // 9 of 10 are used. A charge of 5 needs the two oldest charges to
            // expire.
            let at = start + TimeDelta::days(3);
            assert_eq!(
                rl.allowed(key, 5, at).await,
                RlDecision::Denied {
                    retry_at: Some(start + TimeDelta::days(1) + TIME_WINDOW)
                }
            );
        }

        #[sqlx::test]
        async fn a_charge_over_the_limit_never_fits(pool: PgPool) {
            let rl = limiter(&pool, 10);

            assert_eq!(
                rl.allowed(test_key(b"test"), 11, now()).await,
                RlDecision::Denied { retry_at: None }
            );
            assert_eq!(event_count(&pool).await, 0);
        }

        #[sqlx::test]
        async fn keys_have_separate_allowances(pool: PgPool) {
            let rl = limiter(&pool, 10);
            let now = now();

            assert!(rl.allowed(test_key(b"a"), 10, now).await.is_allowed());
            assert!(rl.allowed(test_key(b"b"), 10, now).await.is_allowed());
            assert!(!rl.allowed(test_key(b"a"), 1, now).await.is_allowed());
        }

        #[sqlx::test]
        async fn concurrent_charges_spend_the_allowance_once_each(pool: PgPool) {
            const CHARGES: usize = 20;
            const LIMIT: u64 = 5;
            let key = test_key(b"test");
            let now = now();

            let mut charges = Vec::with_capacity(CHARGES);
            for _ in 0..CHARGES {
                let rl = limiter(&pool, LIMIT);
                let key = key.clone();
                charges.push(tokio::spawn(async move {
                    rl.allowed(key, 1, now).await.is_allowed()
                }));
            }

            let mut allowed: u64 = 0;
            for charge in charges {
                if charge.await.expect("charge task panicked") {
                    allowed += 1;
                }
            }
            assert_eq!(allowed, LIMIT);
        }

        #[sqlx::test]
        async fn delete_expired_keeps_live_charges(pool: PgPool) {
            let rl = limiter(&pool, 10);
            let start = now();
            assert!(rl.allowed(test_key(b"old"), 1, start).await.is_allowed());
            let later = start + TimeDelta::days(1);
            assert!(rl.allowed(test_key(b"new"), 1, later).await.is_allowed());

            delete_expired(&pool, start + TIME_WINDOW).await.unwrap();

            assert_eq!(event_count(&pool).await, 1);
        }
    }
}
