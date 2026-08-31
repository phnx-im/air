// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    codec::{BlobDecoded, BlobEncoded},
    time::{Duration, TimeStamp},
};
use mls_assist::openmls::prelude::GroupEpoch;
use sqlx::{
    PgExecutor, query, query_scalar,
    types::chrono::{DateTime, Utc},
};
use uuid::Uuid;

use crate::errors::StorageError;

use super::{DsEpochSnapshot, EncryptedEpochSnapshot};

impl DsEpochSnapshot {
    /// Store the snapshot of one epoch.
    pub(crate) async fn store(
        connection: impl PgExecutor<'_>,
        group_id: Uuid,
        epoch: GroupEpoch,
        ciphertext: &EncryptedEpochSnapshot,
        created_at: TimeStamp,
    ) -> Result<(), StorageError> {
        query!(
            "INSERT INTO
            ds_epoch_snapshot
            (group_id, epoch, ciphertext, created_at)
        VALUES
            ($1, $2, $3, $4)
        ON CONFLICT (group_id, epoch) DO NOTHING",
            group_id,
            epoch.as_u64() as i64,
            BlobEncoded(ciphertext) as _,
            DateTime::<Utc>::from(created_at),
        )
        .execute(connection)
        .await?;
        Ok(())
    }

    /// Load the snapshot of one epoch, unless it is older than `retention`.
    ///
    /// The sweep only runs when a group sees a commit, so the cutoff belongs in
    /// the query too. Otherwise an idle group would keep serving rows the
    /// retention window has passed.
    pub(crate) async fn load(
        connection: impl PgExecutor<'_>,
        group_id: Uuid,
        epoch: GroupEpoch,
        retention: Duration,
    ) -> Result<Option<EncryptedEpochSnapshot>, StorageError> {
        let cutoff = Utc::now() - retention;
        let ciphertext = query_scalar!(
            r#"SELECT
            ciphertext AS "ciphertext: BlobDecoded<EncryptedEpochSnapshot>"
        FROM
            ds_epoch_snapshot
        WHERE
            group_id = $1 AND epoch = $2 AND created_at >= $3"#,
            group_id,
            epoch.as_u64() as i64,
            cutoff,
        )
        .fetch_optional(connection)
        .await?
        .map(BlobDecoded::into_inner);
        Ok(ciphertext)
    }

    /// Delete the rows of `group_id` that are older than `retention`.
    pub(crate) async fn delete_expired(
        connection: impl PgExecutor<'_>,
        group_id: Uuid,
        retention: Duration,
    ) -> sqlx::Result<()> {
        let cutoff = Utc::now() - retention;
        query!(
            "DELETE FROM
            ds_epoch_snapshot
        WHERE
            group_id = $1 AND created_at < $2",
            group_id,
            cutoff,
        )
        .execute(connection)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use aircommon::{crypto::aead::Ciphertext, identifiers::QualifiedGroupId};
    use sqlx::PgPool;
    use tokio_util::sync::CancellationToken;

    use crate::{
        air_service::BackendService,
        ds::{Ds, group_state::StorableDsGroupData},
        version::VersionPolicy,
    };

    use super::*;

    /// A retention no test row can age out of, so `load` filters only what the
    /// test means it to.
    const KEEP_ALL: Duration = Duration::days(3650);

    /// The rows reference `encrypted_group`, so a group has to exist first.
    async fn store_group(pool: &PgPool, ds: &Ds) -> anyhow::Result<Uuid> {
        let group_uuid = Uuid::new_v4();
        assert!(ds.reserve_group_id(group_uuid).await);
        let qgid = QualifiedGroupId::new(group_uuid, ds.own_domain.clone());
        let reserved = ds.claim_reserved_group_id(qgid.group_uuid()).await.unwrap();
        StorableDsGroupData::new_and_store(pool, reserved, Ciphertext::random()).await?;
        Ok(group_uuid)
    }

    async fn new_ds(pool: PgPool) -> anyhow::Result<Ds> {
        Ok(Ds::new_from_pool(
            pool,
            "example.com".parse().unwrap(),
            VersionPolicy::default(),
            CancellationToken::new(),
        )
        .await?)
    }

    #[sqlx::test]
    async fn store_and_load(pool: PgPool) -> anyhow::Result<()> {
        let ds = new_ds(pool.clone()).await?;
        let group_id = store_group(&pool, &ds).await?;
        let epoch = GroupEpoch::from(3);
        let ciphertext = EncryptedEpochSnapshot::from(Ciphertext::random());

        DsEpochSnapshot::store(&pool, group_id, epoch, &ciphertext, TimeStamp::now()).await?;

        assert_eq!(
            DsEpochSnapshot::load(&pool, group_id, epoch, KEEP_ALL).await?,
            Some(ciphertext)
        );
        assert_eq!(
            DsEpochSnapshot::load(&pool, group_id, GroupEpoch::from(4), KEEP_ALL).await?,
            None
        );
        assert_eq!(
            DsEpochSnapshot::load(&pool, Uuid::new_v4(), epoch, KEEP_ALL).await?,
            None
        );

        Ok(())
    }

    /// A second store for the same epoch leaves the first row alone.
    #[sqlx::test]
    async fn store_is_idempotent(pool: PgPool) -> anyhow::Result<()> {
        let ds = new_ds(pool.clone()).await?;
        let group_id = store_group(&pool, &ds).await?;
        let epoch = GroupEpoch::from(3);
        let first = EncryptedEpochSnapshot::from(Ciphertext::random());
        let second = EncryptedEpochSnapshot::from(Ciphertext::random());

        DsEpochSnapshot::store(&pool, group_id, epoch, &first, TimeStamp::now()).await?;
        DsEpochSnapshot::store(&pool, group_id, epoch, &second, TimeStamp::now()).await?;

        assert_eq!(
            DsEpochSnapshot::load(&pool, group_id, epoch, KEEP_ALL).await?,
            Some(first)
        );

        Ok(())
    }

    /// The rows the sweep leaves behind, loaded with a retention that hides
    /// nothing, so this is about deletion rather than the load cutoff.
    #[sqlx::test]
    async fn expiry(pool: PgPool) -> anyhow::Result<()> {
        let ds = new_ds(pool.clone()).await?;
        let group_id = store_group(&pool, &ds).await?;
        let retention = Duration::days(7);

        let fresh = GroupEpoch::from(3);
        let stale = GroupEpoch::from(4);
        let stale_time = TimeStamp::from(Utc::now() - retention - Duration::hours(1));
        DsEpochSnapshot::store(
            &pool,
            group_id,
            fresh,
            &Ciphertext::random(),
            TimeStamp::now(),
        )
        .await?;
        DsEpochSnapshot::store(&pool, group_id, stale, &Ciphertext::random(), stale_time).await?;
        DsEpochSnapshot::delete_expired(&pool, group_id, retention).await?;

        assert!(
            DsEpochSnapshot::load(&pool, group_id, fresh, KEEP_ALL)
                .await?
                .is_some()
        );
        assert!(
            DsEpochSnapshot::load(&pool, group_id, stale, KEEP_ALL)
                .await?
                .is_none()
        );

        Ok(())
    }

    /// The sweep runs only when a group sees a commit, so an aged row of an
    /// idle group has to stay unserved on its own.
    #[sqlx::test]
    async fn a_stale_row_does_not_load_before_the_sweep(pool: PgPool) -> anyhow::Result<()> {
        let ds = new_ds(pool.clone()).await?;
        let group_id = store_group(&pool, &ds).await?;
        let epoch = GroupEpoch::from(3);
        let retention = Duration::days(7);
        let stale_time = TimeStamp::from(Utc::now() - retention - Duration::hours(1));

        DsEpochSnapshot::store(&pool, group_id, epoch, &Ciphertext::random(), stale_time).await?;

        assert!(
            DsEpochSnapshot::load(&pool, group_id, epoch, retention)
                .await?
                .is_none()
        );
        // The row is still there, the cutoff only hides it.
        assert!(
            DsEpochSnapshot::load(&pool, group_id, epoch, KEEP_ALL)
                .await?
                .is_some()
        );

        Ok(())
    }

    #[sqlx::test]
    async fn deleting_the_group_deletes_its_rows(pool: PgPool) -> anyhow::Result<()> {
        let ds = new_ds(pool.clone()).await?;
        let group_id = store_group(&pool, &ds).await?;
        let epoch = GroupEpoch::from(3);
        DsEpochSnapshot::store(
            &pool,
            group_id,
            epoch,
            &Ciphertext::random(),
            TimeStamp::now(),
        )
        .await?;

        let qgid = QualifiedGroupId::new(group_id, ds.own_domain.clone());
        StorableDsGroupData::<true>::delete(&pool, &qgid).await?;

        assert!(
            DsEpochSnapshot::load(&pool, group_id, epoch, KEEP_ALL)
                .await?
                .is_none()
        );

        Ok(())
    }
}
