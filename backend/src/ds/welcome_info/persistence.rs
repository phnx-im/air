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

use super::{DsWelcomeInfo, EncryptedWelcomeInfo};

impl DsWelcomeInfo {
    /// Store the welcome information for one epoch.
    ///
    /// An epoch is reached at most once per group, so a conflict means the row is
    /// already there and there is nothing to change.
    pub(crate) async fn store(
        connection: impl PgExecutor<'_>,
        group_id: Uuid,
        epoch: GroupEpoch,
        ciphertext: &EncryptedWelcomeInfo,
        created_at: TimeStamp,
    ) -> Result<(), StorageError> {
        query!(
            "INSERT INTO
            ds_welcome_info
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

    pub(crate) async fn load(
        connection: impl PgExecutor<'_>,
        group_id: Uuid,
        epoch: GroupEpoch,
    ) -> Result<Option<EncryptedWelcomeInfo>, StorageError> {
        let ciphertext = query_scalar!(
            r#"SELECT
            ciphertext AS "ciphertext: BlobDecoded<EncryptedWelcomeInfo>"
        FROM
            ds_welcome_info
        WHERE
            group_id = $1 AND epoch = $2"#,
            group_id,
            epoch.as_u64() as i64,
        )
        .fetch_optional(connection)
        .await?
        .map(BlobDecoded::into_inner);
        Ok(ciphertext)
    }

    /// Delete the rows of `group_id` that are older than `retention`.
    ///
    /// Unlike the group state itself, this needs no key: `created_at` is a plain
    /// column, so the sweep is ordinary SQL.
    pub(crate) async fn delete_expired(
        connection: impl PgExecutor<'_>,
        group_id: Uuid,
        retention: Duration,
    ) -> sqlx::Result<()> {
        let cutoff = Utc::now() - retention;
        query!(
            "DELETE FROM
            ds_welcome_info
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
    };

    use super::*;

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
            None,
            CancellationToken::new(),
        )
        .await?)
    }

    #[sqlx::test]
    async fn store_and_load(pool: PgPool) -> anyhow::Result<()> {
        let ds = new_ds(pool.clone()).await?;
        let group_id = store_group(&pool, &ds).await?;
        let epoch = GroupEpoch::from(3);
        let ciphertext = EncryptedWelcomeInfo::from(Ciphertext::random());

        DsWelcomeInfo::store(&pool, group_id, epoch, &ciphertext, TimeStamp::now()).await?;

        assert_eq!(
            DsWelcomeInfo::load(&pool, group_id, epoch).await?,
            Some(ciphertext)
        );
        assert_eq!(
            DsWelcomeInfo::load(&pool, group_id, GroupEpoch::from(4)).await?,
            None
        );
        assert_eq!(
            DsWelcomeInfo::load(&pool, Uuid::new_v4(), epoch).await?,
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
        let first = EncryptedWelcomeInfo::from(Ciphertext::random());
        let second = EncryptedWelcomeInfo::from(Ciphertext::random());

        DsWelcomeInfo::store(&pool, group_id, epoch, &first, TimeStamp::now()).await?;
        DsWelcomeInfo::store(&pool, group_id, epoch, &second, TimeStamp::now()).await?;

        assert_eq!(
            DsWelcomeInfo::load(&pool, group_id, epoch).await?,
            Some(first)
        );

        Ok(())
    }

    #[sqlx::test]
    async fn expiry(pool: PgPool) -> anyhow::Result<()> {
        let ds = new_ds(pool.clone()).await?;
        let group_id = store_group(&pool, &ds).await?;
        let retention = Duration::days(7);

        let fresh = GroupEpoch::from(3);
        let stale = GroupEpoch::from(4);
        let stale_time = TimeStamp::from(Utc::now() - retention - Duration::hours(1));
        DsWelcomeInfo::store(
            &pool,
            group_id,
            fresh,
            &Ciphertext::random(),
            TimeStamp::now(),
        )
        .await?;
        DsWelcomeInfo::store(&pool, group_id, stale, &Ciphertext::random(), stale_time).await?;
        DsWelcomeInfo::delete_expired(&pool, group_id, retention).await?;

        assert!(DsWelcomeInfo::load(&pool, group_id, fresh).await?.is_some());
        assert!(DsWelcomeInfo::load(&pool, group_id, stale).await?.is_none());

        Ok(())
    }

    #[sqlx::test]
    async fn deleting_the_group_deletes_its_rows(pool: PgPool) -> anyhow::Result<()> {
        let ds = new_ds(pool.clone()).await?;
        let group_id = store_group(&pool, &ds).await?;
        let epoch = GroupEpoch::from(3);
        DsWelcomeInfo::store(
            &pool,
            group_id,
            epoch,
            &Ciphertext::random(),
            TimeStamp::now(),
        )
        .await?;

        let qgid = QualifiedGroupId::new(group_id, ds.own_domain.clone());
        StorableDsGroupData::<true>::delete(&pool, &qgid).await?;

        assert!(DsWelcomeInfo::load(&pool, group_id, epoch).await?.is_none());

        Ok(())
    }
}
