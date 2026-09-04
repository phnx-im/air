// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, PoisonError},
};

use sqlx::PgPool;

use super::{Allowance, RlConfig, RlKey, StorageProvider};

/// Allowances kept in process memory.
#[derive(Debug, Clone, Default)]
pub(crate) struct RlMemoryStorage {
    allowances: Arc<Mutex<HashMap<Vec<u8>, Allowance>>>,
}

impl RlMemoryStorage {
    /// Charges one request against `key` and reports whether it is allowed.
    pub(crate) fn charge(&self, config: &RlConfig, key: &RlKey) -> bool {
        let mut allowances = self.lock();
        allowances
            .entry(key.serialize().to_owned())
            .or_insert_with(|| Allowance::new(config))
            .allowed(config)
    }

    /// Forgets allowances whose window has passed, so the map does not grow
    /// with every address that ever connected.
    pub(crate) fn prune(&self) {
        self.lock().retain(|_, allowance| !allowance.is_stale());
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<Vec<u8>, Allowance>> {
        self.allowances
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
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
    async fn get(&self, key: &RlKey) -> Option<Allowance> {
        Allowance::load(&self.pool, key).await.ok().flatten()
    }

    async fn set(&self, key: RlKey, allowance: Allowance) {
        if let Err(error) = allowance.store(&self.pool, &key).await {
            tracing::error!(%error, "Failed to store allowance in Postgres");
        }
    }
}

pub(crate) mod persistence {

    use chrono::SubsecRound;
    use sqlx::{
        PgExecutor, query, query_as,
        types::chrono::{DateTime, Utc},
    };

    use crate::{errors::StorageError, rate_limiter::RlKey};

    use super::Allowance;

    impl Allowance {
        /// Load an Allowance from the database by its key.
        pub(in crate::rate_limiter) async fn load(
            connection: impl PgExecutor<'_>,
            key: &RlKey,
        ) -> Result<Option<Allowance>, StorageError> {
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
            .fetch_optional(connection)
            .await?;
            Ok(record.map(|record| Allowance {
                remaining: record.remaining as u64,
                valid_until: record.valid_until,
            }))
        }

        /// Store an Allowance in the database.
        pub(in crate::rate_limiter) async fn store(
            &self,
            connection: impl PgExecutor<'_>,
            key: &RlKey,
        ) -> Result<(), StorageError> {
            // Ensure valid_until is rounded to microseconds, since postgres
            // only supports microsecond precision.
            let valid_until = self.valid_until.round_subsecs(6);
            query!(
                "INSERT INTO allowance_record
                    (key_value, remaining, valid_until)
                    VALUES ($1, $2, $3)",
                key.serialize(),
                self.remaining as i64,
                valid_until,
            )
            .execute(connection)
            .await?;
            Ok(())
        }

        /// Delete all expired allowances.
        #[allow(dead_code)]
        pub(in crate::rate_limiter) async fn delete_expired(
            connection: impl PgExecutor<'_>,
        ) -> Result<(), sqlx::Error> {
            query!("DELETE FROM allowance_record WHERE valid_until < NOW()")
                .execute(connection)
                .await?;
            Ok(())
        }
    }

    #[cfg(test)]
    pub(crate) mod tests {
        use chrono::TimeDelta;
        use sqlx::PgPool;

        use crate::rate_limiter::RlConfig;

        use super::*;

        pub async fn store_random_allowance(
            pool: &PgPool,
            key: &RlKey,
        ) -> anyhow::Result<Allowance> {
            let config = RlConfig {
                max_requests: 10,
                time_window: TimeDelta::hours(1),
            };
            let allowance = Allowance::new(&config);
            allowance.store(pool, key).await?;
            Ok(allowance)
        }

        #[sqlx::test]
        async fn load_allowance(pool: PgPool) -> anyhow::Result<()> {
            let key = RlKey::new(b"test_service", b"test_rpc", &[]);
            let allowance = store_random_allowance(&pool, &key).await?;

            let loaded = Allowance::load(&pool, &key)
                .await?
                .expect("missing allowance record");
            assert_eq!(loaded, allowance);

            Ok(())
        }

        #[sqlx::test]
        async fn delete_expired_allowances(pool: PgPool) -> anyhow::Result<()> {
            // First, store an allowance that is valid
            let key = RlKey::new(b"test_service", b"test_rpc", &[]);
            let allowance = store_random_allowance(&pool, &key).await?;

            // Then, delete expired allowances (should not delete the valid one)
            Allowance::delete_expired(&pool).await?;

            // Load the valid allowance to ensure it still exists
            let loaded = Allowance::load(&pool, &key)
                .await?
                .expect("missing allowance record");
            assert_eq!(loaded, allowance);

            // Now, store an expired allowance
            let expired_key = RlKey::new(b"expired_service", b"expired_rpc", &[]);
            let expired_allowance = Allowance {
                remaining: 0,
                valid_until: Utc::now() - TimeDelta::weeks(1), // already expired
            };
            expired_allowance.store(&pool, &expired_key).await?;

            // Delete expired allowances again
            Allowance::delete_expired(&pool).await?;

            // Ensure the expired allowance is deleted
            let loaded_expired = Allowance::load(&pool, &expired_key).await?;
            assert!(loaded_expired.is_none());

            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeDelta;

    use super::*;

    /// Concurrent requests for one key must not all read the same allowance
    /// and pass together.
    #[tokio::test]
    async fn concurrent_charges_spend_the_allowance_once_each() {
        const REQUESTS: usize = 20;
        const LIMIT: u64 = 5;

        let storage = RlMemoryStorage::default();
        let config = RlConfig {
            max_requests: LIMIT,
            time_window: TimeDelta::hours(1),
        };
        let key = RlKey::new(b"test_service", b"test_rpc", &[]);

        let mut charges = Vec::with_capacity(REQUESTS);
        for _ in 0..REQUESTS {
            let storage = storage.clone();
            let config = config.clone();
            let key = key.clone();
            charges.push(tokio::spawn(async move { storage.charge(&config, &key) }));
        }

        let mut allowed: u64 = 0;
        for charge in charges {
            if charge.await.expect("charge task panicked") {
                allowed += 1;
            }
        }
        assert_eq!(allowed, LIMIT);
    }

    #[tokio::test]
    async fn pruning_forgets_only_stale_allowances() {
        let storage = RlMemoryStorage::default();
        let live = RlKey::new(b"test_service", b"live", &[]);
        let stale = RlKey::new(b"test_service", b"stale", &[]);

        assert!(storage.charge(
            &RlConfig {
                max_requests: 1,
                time_window: TimeDelta::hours(1),
            },
            &live
        ));
        assert!(storage.charge(
            &RlConfig {
                max_requests: 1,
                time_window: TimeDelta::milliseconds(-1),
            },
            &stale
        ));

        storage.prune();
        let allowances = storage.lock();
        assert!(allowances.contains_key(live.serialize()));
        assert!(!allowances.contains_key(stale.serialize()));
    }
}
