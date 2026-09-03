// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Retention of the derivation epochs of an emulation group.
//!
//! OpenMLS keeps the state of a derivation epoch for as long as the group's
//! log, an emulation binding or retained KeyPackage material names it, see
//! [`VcDerivationEpochRetentionPolicy`]. Air runs the self group under
//! [`VcDerivationEpochRetentionPolicy::KeepAll`] and prunes the log on a
//! wall-clock window instead, since the window is what decides whether a
//! delayed operation of a sibling emulator client can still be processed.
//!
//! Epochs from before the log existed were never tracked, so nothing would
//! keep their state once a binding or retained KeyPackage stops naming it. The
//! migration to the log puts a hold on them: a row in
//! `vc_derivation_epoch_legacy_hold` that the storage provider's sweep treats
//! as a reference until its `held_until`, one retention window after the
//! migration.

use std::time::{Duration, SystemTime};

use aircommon::{
    codec::PersistenceCodec,
    mls_group_config::{VC_DERIVATION_EPOCH_MAX_COUNT, VC_DERIVATION_EPOCH_RETENTION_WINDOW},
    time::TimeStamp,
};
use chrono::Utc;
#[expect(
    deprecated,
    reason = "decodes records written before the per-entry rows"
)]
use openmls::components::vc_derivation_info::{RegisteredVcDerivationEpoch, VcEmulationBindings};
use openmls::{
    components::vc_derivation_info::{VcDerivationEpochLogEntry, VcEmulationBinding},
    group::{GroupId, VcDerivationEpochDeletion, VcDerivationEpochRetentionPolicy},
};
use openmls_traits::{OpenMlsProvider, storage::StorageProvider};
use tracing::info;

use crate::{
    db::access::WriteTransaction,
    groups::{Group, openmls_provider::AirOpenMlsProvider},
};

/// Prunes the derivation epochs of `group` superseded more than `window` ago,
/// capped at [`VC_DERIVATION_EPOCH_MAX_COUNT`]. See
/// [`VcDerivationEpochDeletion`] for what is selected.
pub(crate) fn sweep_vc_derivation_epochs(
    mut txn: impl WriteTransaction,
    group: &mut Group,
    window: Duration,
) -> anyhow::Result<()> {
    let provider = AirOpenMlsProvider::new(txn.as_mut());

    // Flips self groups that were created before the policy existed, whose
    // stored join config deserializes to the `MaxEpochs` default.
    if group.mls_group().vc_derivation_epoch_retention_policy()
        != &VcDerivationEpochRetentionPolicy::KeepAll
    {
        group
            .mls_group_mut()
            .set_vc_derivation_epoch_retention_policy(
                &provider,
                VcDerivationEpochRetentionPolicy::KeepAll,
            )?;
    }

    let deletion = VcDerivationEpochDeletion::older_than_duration(window)
        .max_epochs(VC_DERIVATION_EPOCH_MAX_COUNT);
    let result = group
        .mls_group()
        .delete_vc_derivation_epochs(&provider, deletion)?;
    if !result.deleted.is_empty() || !result.kept.is_empty() {
        info!(
            deleted = result.deleted.len(),
            kept = result.kept.len(),
            "Swept derivation epochs of the emulation group"
        );
    }

    Ok(())
}

/// Code half of the `20260902120000_vc_derivation_epoch_retention` migration.
///
/// Converts the per-group registration and binding records into rows and
/// holds the epoch state the converted log does not name for one retention
/// window.
pub(crate) async fn migrate_vc_derivation_epoch_retention(
    mut write: impl WriteTransaction,
) -> anyhow::Result<()> {
    // Read first, the provider borrows the connection for its whole lifetime.
    // Both queries are unchecked because the old tables are dropped by the next
    // migration and so are absent from the schema the checked queries compile
    // against.
    let registrations: Vec<(Vec<u8>, Vec<u8>)> =
        sqlx::query_as("SELECT group_id, registration FROM vc_registered_emulation_epoch")
            .fetch_all(write.as_mut())
            .await?;
    let binding_records: Vec<(Vec<u8>, Vec<u8>)> =
        sqlx::query_as("SELECT group_id, bindings FROM vc_emulation_binding_record")
            .fetch_all(write.as_mut())
            .await?;
    info!(
        registrations = registrations.len(),
        binding_records = binding_records.len(),
        "Migrating derivation epoch registrations and emulation bindings to rows"
    );

    {
        let provider = AirOpenMlsProvider::new(write.as_mut());
        let storage = provider.storage();

        for (group_id, registration) in registrations {
            let group_id: GroupId = PersistenceCodec::from_slice(&group_id)?;
            #[expect(
                deprecated,
                reason = "decodes records written before the per-entry rows"
            )]
            let RegisteredVcDerivationEpoch {
                group_epoch,
                epoch_id,
            } = PersistenceCodec::from_slice(&registration)?;
            // The timestamp is never compared for this entry's own deletion:
            // the sweep ages an entry out against its successor's timestamp.
            // The next rotation is what starts this entry's window.
            let entry = VcDerivationEpochLogEntry::from_legacy_record(
                group_epoch,
                epoch_id,
                SystemTime::now(),
            );
            storage.write_vc_derivation_epoch_log_entry(&group_id, entry.epoch_id(), &entry)?;
        }

        // Without its bindings a higher-level group cannot resolve the
        // derivation epoch its messages were protected with, and the sweep
        // would take the state those groups still need.
        for (group_id, record) in binding_records {
            let group_id: GroupId = PersistenceCodec::from_slice(&group_id)?;
            #[expect(
                deprecated,
                reason = "decodes records written before the per-entry rows"
            )]
            let record: VcEmulationBindings = PersistenceCodec::from_slice(&record)?;
            for (group_epoch, epoch_id) in record.into_entries() {
                let binding = VcEmulationBinding::from_legacy_record(group_epoch, epoch_id.clone());
                storage.write_vc_emulation_binding(&group_id, &group_epoch, &epoch_id, &binding)?;
            }
        }
    }

    // Nothing records when a queued sibling operation can no longer reference
    // an epoch from before the log, and a binding or retained KeyPackage that
    // names one today can be gone tomorrow. Losing the state means
    // `MissingDerivationEpochState` and a resync the user has to trigger, so
    // every epoch outside the converted log gets one retention window.
    let window = chrono::Duration::from_std(VC_DERIVATION_EPOCH_RETENTION_WINDOW)?;
    let held_until = TimeStamp::from(Utc::now() + window);
    let held = sqlx::query!(
        "INSERT INTO vc_derivation_epoch_legacy_hold(epoch_id, held_until)
        SELECT epoch_id, ?1 FROM (
            SELECT epoch_id FROM vc_emulation_group_secret
                WHERE secret_type = 'emulation_epoch_state'
            UNION
            SELECT epoch_id FROM vc_operation_tree
        ) AS candidate
        WHERE NOT EXISTS (
            SELECT 1 FROM vc_derivation_epoch_log_entry
            WHERE epoch_id = candidate.epoch_id
        )",
        held_until,
    )
    .execute(write.as_mut())
    .await?
    .rows_affected();
    info!(
        held,
        "Holding the derivation epoch state from before the log for one retention window"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use aircommon::{
        credentials::keys::{LeafSigningKey, SelfGroupSigningKey},
        crypto::aead::keys::IdentityLinkWrapperKey,
        identifiers::{QualifiedGroupId, UserId},
        mls_group_config::AppComponent,
    };
    use airprotos::client::component::AirComponent;
    use chrono::DateTime;
    use openmls::components::vc_derivation_info::{EpochId, VC_COMPONENT_ID};
    use uuid::Uuid;

    use crate::{
        db::access::{DbAccess, WriteConnection, WriteDbTransaction},
        groups::GroupDataBytes,
        utils::persistence::open_db_in_memory,
    };

    use super::*;

    fn random_group_id() -> GroupId {
        GroupId::from(QualifiedGroupId::new(
            Uuid::new_v4(),
            "example.com".parse().unwrap(),
        ))
    }

    fn create_self_group(
        txn: &mut WriteDbTransaction<'_>,
        signer: &LeafSigningKey,
    ) -> anyhow::Result<Group> {
        let (group, _params) = Group::create_apq_group(
            &mut *txn,
            signer,
            UserId::random("example.com".parse()?),
            IdentityLinkWrapperKey::random()?,
            random_group_id(),
            random_group_id(),
            GroupDataBytes::from(b"test-group-data".to_vec()),
            // Registering a derivation epoch requires Safe AAD framing, as on
            // the real self group.
            Some(vec![VC_COMPONENT_ID]),
            AirComponent::default_for_self_group(),
        )?;
        Ok(group)
    }

    fn rotate_derivation_epoch(
        group: &mut Group,
        txn: &mut WriteDbTransaction<'_>,
        signer: &LeafSigningKey,
    ) -> anyhow::Result<()> {
        let provider = AirOpenMlsProvider::new(txn.as_mut());
        let (t_mls_group, pq_mls_group) = group.apq_mls_groups_mut()?;
        let _bundle = apqmls::commit_builder::CommitBuilder::from_groups(
            &mut *t_mls_group,
            &mut *pq_mls_group,
        )
        .force_self_update(true)
        .derivation_epoch(true)
        .finalize(&provider, signer, |_| true, |_| true)?;
        t_mls_group.merge_pending_commit(&provider)?;
        pq_mls_group.merge_pending_commit(&provider)?;
        Ok(())
    }

    fn newest_epoch(group: &Group, txn: &mut WriteDbTransaction<'_>) -> anyhow::Result<EpochId> {
        let provider = AirOpenMlsProvider::new(txn.as_mut());
        Ok(group
            .mls_group()
            .newest_vc_derivation_epoch(provider.storage())?
            .expect("the self group has a derivation epoch"))
    }

    async fn epoch_state_exists(
        txn: &mut WriteDbTransaction<'_>,
        epoch_id: &EpochId,
    ) -> anyhow::Result<bool> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1 FROM vc_emulation_group_secret
                WHERE epoch_id = ?1 AND secret_type = 'emulation_epoch_state'
            )",
        )
        .bind(PersistenceCodec::to_vec(epoch_id)?)
        .fetch_one(txn.as_mut())
        .await?;
        Ok(exists)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sweep_deletes_superseded_epochs() -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let signer = LeafSigningKey::SelfGroup(SelfGroupSigningKey::generate(Uuid::new_v4())?);

        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;

        let mut group = create_self_group(&mut txn, &signer)?;
        let first = newest_epoch(&group, &mut txn)?;
        rotate_derivation_epoch(&mut group, &mut txn, &signer)?;
        let second = newest_epoch(&group, &mut txn)?;
        rotate_derivation_epoch(&mut group, &mut txn, &signer)?;
        let third = newest_epoch(&group, &mut txn)?;

        assert_ne!(first, second);
        assert_ne!(second, third);
        for epoch in [&first, &second, &third] {
            assert!(epoch_state_exists(&mut txn, epoch).await?);
        }

        sweep_vc_derivation_epochs(&mut txn, &mut group, Duration::ZERO)?;

        assert!(!epoch_state_exists(&mut txn, &first).await?);
        assert!(!epoch_state_exists(&mut txn, &second).await?);
        assert!(
            epoch_state_exists(&mut txn, &third).await?,
            "the newest derivation epoch must survive"
        );

        // The group still resolves its derivation epoch, so it can keep
        // operating after the sweep.
        assert_eq!(newest_epoch(&group, &mut txn)?, third);

        txn.commit().await?;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sweep_keeps_epochs_inside_the_window() -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let signer = LeafSigningKey::SelfGroup(SelfGroupSigningKey::generate(Uuid::new_v4())?);

        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;

        let mut group = create_self_group(&mut txn, &signer)?;
        let first = newest_epoch(&group, &mut txn)?;
        rotate_derivation_epoch(&mut group, &mut txn, &signer)?;
        let second = newest_epoch(&group, &mut txn)?;

        sweep_vc_derivation_epochs(&mut txn, &mut group, VC_DERIVATION_EPOCH_RETENTION_WINDOW)?;

        assert!(epoch_state_exists(&mut txn, &first).await?);
        assert!(epoch_state_exists(&mut txn, &second).await?);

        txn.commit().await?;
        Ok(())
    }

    /// An epoch from before the log, which nothing references.
    fn untracked_epoch() -> EpochId {
        EpochId::new(b"legacy-epoch".to_vec())
    }

    async fn store_untracked_epoch_state(txn: &mut WriteDbTransaction<'_>) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO vc_emulation_group_secret(epoch_id, secret_type, vc_secret)
            VALUES (?, 'emulation_epoch_state', ?)",
        )
        .bind(PersistenceCodec::to_vec(&untracked_epoch())?)
        .bind(b"secret".to_vec())
        .execute(txn.as_mut())
        .await?;
        Ok(())
    }

    async fn hold_untracked_epoch(
        txn: &mut WriteDbTransaction<'_>,
        held_until: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO vc_derivation_epoch_legacy_hold(epoch_id, held_until)
            VALUES (?1, ?2)
            ON CONFLICT(epoch_id) DO UPDATE SET held_until = excluded.held_until",
        )
        .bind(PersistenceCodec::to_vec(&untracked_epoch())?)
        .bind(TimeStamp::from(held_until))
        .execute(txn.as_mut())
        .await?;
        Ok(())
    }

    async fn hold_count(txn: &mut WriteDbTransaction<'_>) -> anyhow::Result<i64> {
        Ok(
            sqlx::query_scalar("SELECT COUNT(*) FROM vc_derivation_epoch_legacy_hold")
                .fetch_one(txn.as_mut())
                .await?,
        )
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn rotation_sweeps_untracked_epoch_state() -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let signer = LeafSigningKey::SelfGroup(SelfGroupSigningKey::generate(Uuid::new_v4())?);

        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;

        let mut group = create_self_group(&mut txn, &signer)?;
        store_untracked_epoch_state(&mut txn).await?;
        assert!(epoch_state_exists(&mut txn, &untracked_epoch()).await?);

        rotate_derivation_epoch(&mut group, &mut txn, &signer)?;

        assert!(!epoch_state_exists(&mut txn, &untracked_epoch()).await?);
        let newest = newest_epoch(&group, &mut txn)?;
        assert!(epoch_state_exists(&mut txn, &newest).await?);

        txn.commit().await?;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn held_epoch_state_survives_until_the_hold_expires() -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let signer = LeafSigningKey::SelfGroup(SelfGroupSigningKey::generate(Uuid::new_v4())?);

        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;

        let mut group = create_self_group(&mut txn, &signer)?;
        store_untracked_epoch_state(&mut txn).await?;
        hold_untracked_epoch(&mut txn, Utc::now() + chrono::Duration::days(1)).await?;

        rotate_derivation_epoch(&mut group, &mut txn, &signer)?;
        sweep_vc_derivation_epochs(&mut txn, &mut group, VC_DERIVATION_EPOCH_RETENTION_WINDOW)?;

        assert!(epoch_state_exists(&mut txn, &untracked_epoch()).await?);
        assert_eq!(hold_count(&mut txn).await?, 1);

        hold_untracked_epoch(&mut txn, Utc::now() - chrono::Duration::days(1)).await?;
        sweep_vc_derivation_epochs(&mut txn, &mut group, VC_DERIVATION_EPOCH_RETENTION_WINDOW)?;

        assert!(!epoch_state_exists(&mut txn, &untracked_epoch()).await?);
        assert_eq!(hold_count(&mut txn).await?, 0);

        txn.commit().await?;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sweep_sets_keep_all_policy() -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let signer = LeafSigningKey::SelfGroup(SelfGroupSigningKey::generate(Uuid::new_v4())?);

        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;

        let mut group = create_self_group(&mut txn, &signer)?;
        {
            let provider = AirOpenMlsProvider::new(txn.as_mut());
            group
                .mls_group_mut()
                .set_vc_derivation_epoch_retention_policy(
                    &provider,
                    VcDerivationEpochRetentionPolicy::MaxEpochs(5),
                )?;
        }

        sweep_vc_derivation_epochs(&mut txn, &mut group, VC_DERIVATION_EPOCH_RETENTION_WINDOW)?;

        assert_eq!(
            group.mls_group().vc_derivation_epoch_retention_policy(),
            &VcDerivationEpochRetentionPolicy::KeepAll
        );

        txn.commit().await?;
        Ok(())
    }
}
