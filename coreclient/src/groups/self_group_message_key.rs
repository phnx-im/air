// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Derivation and persistence of the per-epoch self-group message key.
//!
//! Self-group commits carry encrypted `SelfGroupMessages` payloads (settings
//! updates today) under a symmetric key that is scoped to a single self-group
//! epoch. The key is derived from the MLS safe exporter of the T group for
//! [`AIR_COMPONENT_ID`] and then run through one further KDF step.
//!
//! The MLS application export tree is a puncturable PRF: exporting the same
//! component ID twice within one epoch fails, because the first export
//! punctures the tree. That is fine for the sender, but the receive path also
//! needs the key, and in a commit race the losing committer has to decrypt the
//! *winning* commit against the same (pre-merge) epoch it originally derived
//! the key in — a second export. We therefore derive lazily and cache the key
//! in a per-group table: the first access in an epoch performs the single
//! permitted export and persists the result, and every later access in that
//! epoch returns the cached key. The punctured tree and the cached key are
//! written in the same transaction, so they can never diverge.

use aircommon::crypto::{
    aead::{AEAD_KEY_SIZE, keys::SelfGroupMessageKey},
    kdf::{KdfDerivable, keys::SelfGroupExporterSecret},
};
use airprotos::client::component::{AIR_COMPONENT_ID, AirComponent};
use anyhow::{Result, anyhow, ensure};
use openmls_traits::OpenMlsProvider;

use crate::{
    db::access::WriteDbTransaction,
    groups::{Group, openmls_provider::AirOpenMlsProvider},
};

impl Group {
    /// Returns whether this group is a self-group, according to the
    /// `is_self_group` flag in the AIR component of the group context.
    // Wired into the send/receive paths in later steps of the settings-sync
    // plan; kept here alongside the key accessor that already relies on it.
    #[allow(dead_code)]
    pub(crate) fn is_self_group(&self) -> bool {
        self.mls_group()
            .extensions()
            .app_data_dictionary()
            .and_then(|dict| dict.dictionary().get(&AIR_COMPONENT_ID))
            .and_then(|data| AirComponent::from_bytes(data).ok())
            .map(|component| component.is_self_group)
            .unwrap_or(false)
    }

    /// Returns the message key for the self-group's current epoch, deriving and
    /// persisting it on first access in an epoch.
    ///
    /// On first access in an epoch this exports `AIR_COMPONENT_ID` from the T
    /// group's safe exporter (which punctures the export tree), derives the
    /// [`SelfGroupMessageKey`], and upserts it into the cache — replacing any
    /// row left over from a previous epoch. Every later access in the same
    /// epoch returns the cached key without re-exporting. See the module-level
    /// docs for why the cache is mandatory rather than an optimization.
    ///
    /// The export tree is persisted by `safe_export_secret` through `txn`, and
    /// the cached key is written through the same `txn`, so they commit
    /// atomically.
    ///
    /// Errors if called on a group that is not the self-group: deriving the AIR
    /// component export for an ordinary chat group would puncture that group's
    /// export tree for nothing.
    // Wired into the send/receive paths in later steps of the settings-sync plan.
    #[allow(dead_code)]
    pub(crate) async fn self_group_message_key(
        &mut self,
        txn: &mut WriteDbTransaction<'_>,
    ) -> Result<SelfGroupMessageKey> {
        ensure!(
            self.is_self_group(),
            "self_group_message_key must only be called on the self group"
        );

        let group_id = self.group_id().clone();
        let current_epoch = self.mls_group().epoch().as_u64();

        // Return the cached key if it was derived for the current epoch.
        if let Some(stored) = persistence::load(&mut *txn, &group_id).await?
            && stored.epoch == current_epoch
        {
            return Ok(stored.key);
        }

        // Otherwise export from the T group and derive. This punctures the
        // export tree; the punctured state is persisted by `safe_export_secret`
        // through the provider's storage, i.e. through `txn`.
        let key = {
            let provider = AirOpenMlsProvider::new(txn.as_mut());
            let (t_mls_group, _pq_mls_group) = self.apq_mls_groups_mut()?;
            let exporter_bytes = t_mls_group.safe_export_secret(
                provider.crypto(),
                provider.storage(),
                AIR_COMPONENT_ID,
            )?;
            let exporter_bytes: [u8; AEAD_KEY_SIZE] = exporter_bytes
                .try_into()
                .map_err(|_| anyhow!("unexpected self-group exporter secret length"))?;
            let exporter = SelfGroupExporterSecret::from_bytes(exporter_bytes);
            SelfGroupMessageKey::derive(&exporter, &Vec::new())?
            // `exporter` is zeroized here as it is dropped at the end of this
            // scope (its inner secret is `ZeroizeOnDrop`).
        };

        // Upsert the cache, replacing any stale epoch's key.
        persistence::store(&mut *txn, &group_id, current_epoch, &key).await?;

        Ok(key)
    }
}

mod persistence {
    use aircommon::crypto::aead::keys::SelfGroupMessageKey;
    use openmls::group::GroupId;
    use sqlx::query;

    use crate::{
        db::access::{ReadConnection, WriteConnection},
        utils::persistence::GroupIdRefWrapper,
    };

    /// A self-group message key together with the epoch it was derived for.
    pub(super) struct StoredSelfGroupMessageKey {
        pub(super) epoch: u64,
        pub(super) key: SelfGroupMessageKey,
    }

    pub(super) async fn load(
        mut connection: impl ReadConnection,
        group_id: &GroupId,
    ) -> sqlx::Result<Option<StoredSelfGroupMessageKey>> {
        let group_id = GroupIdRefWrapper::from(group_id);
        let row = query!(
            r#"SELECT
                epoch AS "epoch!: i64",
                key AS "key!: SelfGroupMessageKey"
            FROM self_group_message_key
            WHERE group_id = ?"#,
            group_id,
        )
        .fetch_optional(connection.as_mut())
        .await?;
        Ok(row.map(|row| StoredSelfGroupMessageKey {
            epoch: row.epoch as u64,
            key: row.key,
        }))
    }

    pub(super) async fn store(
        mut connection: impl WriteConnection,
        group_id: &GroupId,
        epoch: u64,
        key: &SelfGroupMessageKey,
    ) -> sqlx::Result<()> {
        let group_id = GroupIdRefWrapper::from(group_id);
        let epoch = epoch as i64;
        query!(
            r#"INSERT OR REPLACE INTO self_group_message_key (group_id, epoch, key)
            VALUES (?, ?, ?)"#,
            group_id,
            epoch,
            key,
        )
        .execute(connection.as_mut())
        .await?;
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use aircommon::{RustCrypto, crypto::secrets::Secret};
        use openmls::group::GroupId;
        use sqlx::SqlitePool;

        use crate::db::access::DbAccess;

        use super::*;

        fn random_key() -> SelfGroupMessageKey {
            SelfGroupMessageKey::from(Secret::random().unwrap())
        }

        #[sqlx::test]
        async fn store_and_load(pool: SqlitePool) -> anyhow::Result<()> {
            let pool = DbAccess::for_tests(pool);
            let group_id = GroupId::random(&RustCrypto::default());
            let key = random_key();

            assert!(load(pool.read().await?, &group_id).await?.is_none());

            store(pool.write().await?, &group_id, 7, &key).await?;

            let loaded = load(pool.read().await?, &group_id)
                .await?
                .expect("stored key should load");
            assert_eq!(loaded.epoch, 7);
            assert_eq!(loaded.key, key);

            Ok(())
        }

        #[sqlx::test]
        async fn upsert_replaces_on_epoch_change(pool: SqlitePool) -> anyhow::Result<()> {
            let pool = DbAccess::for_tests(pool);
            let group_id = GroupId::random(&RustCrypto::default());
            let old_key = random_key();
            let new_key = random_key();

            store(pool.write().await?, &group_id, 1, &old_key).await?;
            store(pool.write().await?, &group_id, 2, &new_key).await?;

            // The stale epoch's key is gone, only the current one remains.
            let loaded = load(pool.read().await?, &group_id)
                .await?
                .expect("stored key should load");
            assert_eq!(loaded.epoch, 2);
            assert_eq!(loaded.key, new_key);

            // Exactly one row per group.
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM self_group_message_key")
                .fetch_one(pool.read().await?.as_mut())
                .await?;
            assert_eq!(count, 1);

            Ok(())
        }
    }
}

#[cfg(test)]
mod derivation_tests {
    use aircommon::{
        credentials::{keys::ClientSigningKey, test_utils::create_test_credentials},
        crypto::aead::keys::IdentityLinkWrapperKey,
        identifiers::{QualifiedGroupId, UserId},
        mls_group_config::AppComponent,
    };
    use airprotos::client::component::AirComponent;
    use openmls::prelude::GroupId;
    use uuid::Uuid;

    use crate::{
        db::access::{DbAccess, WriteConnection, WriteDbTransaction},
        groups::{Group, GroupDataBytes, openmls_provider::AirOpenMlsProvider},
        utils::persistence::open_db_in_memory,
    };

    fn random_group_id() -> GroupId {
        GroupId::from(QualifiedGroupId::new(
            Uuid::new_v4(),
            "example.com".parse().unwrap(),
        ))
    }

    /// Creates a fresh single-member APQ group with the given self-group flag.
    fn create_group(
        txn: &mut WriteDbTransaction<'_>,
        signer: &ClientSigningKey,
        is_self_group: bool,
    ) -> anyhow::Result<Group> {
        let air_component = if is_self_group {
            AirComponent::default_for_self_group()
        } else {
            AirComponent::default_for_leaf_or_key_package()
        };
        let (group, _params) = Group::create_apq_group(
            &mut *txn,
            signer,
            IdentityLinkWrapperKey::random()?,
            random_group_id(),
            random_group_id(),
            GroupDataBytes::from(b"test-group-data".to_vec()),
            None,
            air_component,
        )?;
        Ok(group)
    }

    /// Advances the group's epoch by staging and merging a forced self-update.
    fn self_update_and_merge(
        group: &mut Group,
        txn: &mut WriteDbTransaction<'_>,
        signer: &ClientSigningKey,
    ) -> anyhow::Result<()> {
        let provider = AirOpenMlsProvider::new(txn.as_mut());
        let (t_mls_group, pq_mls_group) = group.apq_mls_groups_mut()?;
        let _bundle = apqmls::commit_builder::CommitBuilder::from_groups(
            &mut *t_mls_group,
            &mut *pq_mls_group,
        )
        .force_self_update(true)
        .finalize(&provider, signer, |_| true, |_| true)?;
        t_mls_group.merge_pending_commit(&provider)?;
        pq_mls_group.merge_pending_commit(&provider)?;
        Ok(())
    }

    /// THE regression test for the PPRF puncture constraint: two accessor calls
    /// in the same epoch must return the same key and must not error, even
    /// though the underlying safe exporter can only be punctured once per epoch.
    #[tokio::test(flavor = "multi_thread")]
    async fn stable_within_epoch() -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let (_as_key, signer) = create_test_credentials(UserId::random("example.com".parse()?));

        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;

        let mut group = create_group(&mut txn, &signer, true)?;

        let key_1 = group.self_group_message_key(&mut txn).await?;
        let key_2 = group.self_group_message_key(&mut txn).await?;
        assert_eq!(key_1, key_2, "same-epoch derivations must match");

        txn.commit().await?;
        Ok(())
    }

    /// The key rotates when the group's epoch changes.
    #[tokio::test(flavor = "multi_thread")]
    async fn rotates_across_epoch() -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let (_as_key, signer) = create_test_credentials(UserId::random("example.com".parse()?));

        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;

        let mut group = create_group(&mut txn, &signer, true)?;

        let key_before = group.self_group_message_key(&mut txn).await?;

        self_update_and_merge(&mut group, &mut txn, &signer)?;

        let key_after = group.self_group_message_key(&mut txn).await?;
        assert_ne!(
            key_before, key_after,
            "key must rotate after an epoch change"
        );

        // And it is stable again within the new epoch.
        let key_after_again = group.self_group_message_key(&mut txn).await?;
        assert_eq!(key_after, key_after_again);

        txn.commit().await?;
        Ok(())
    }

    /// The accessor refuses to derive for a group that is not the self-group, so
    /// ordinary chat groups never have their export tree punctured for nothing.
    #[tokio::test(flavor = "multi_thread")]
    async fn rejects_non_self_group() -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let (_as_key, signer) = create_test_credentials(UserId::random("example.com".parse()?));

        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;

        let mut group = create_group(&mut txn, &signer, false)?;
        assert!(!group.is_self_group());
        assert!(group.self_group_message_key(&mut txn).await.is_err());

        txn.commit().await?;
        Ok(())
    }
}
