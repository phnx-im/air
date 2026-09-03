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

use std::time::Duration;

use aircommon::mls_group_config::VC_DERIVATION_EPOCH_MAX_COUNT;
use openmls::group::{VcDerivationEpochDeletion, VcDerivationEpochRetentionPolicy};
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

#[cfg(test)]
mod tests {
    use aircommon::{
        codec::PersistenceCodec,
        credentials::keys::{LeafSigningKey, SelfGroupSigningKey},
        crypto::aead::keys::IdentityLinkWrapperKey,
        identifiers::{QualifiedGroupId, UserId},
        mls_group_config::{AppComponent, VC_DERIVATION_EPOCH_RETENTION_WINDOW},
    };
    use airprotos::client::component::AirComponent;
    use openmls::{
        components::vc_derivation_info::{EpochId, VC_COMPONENT_ID},
        group::GroupId,
    };
    use openmls_traits::OpenMlsProvider;
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
