// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Installing a group that a sibling emulator client of the same virtual
//! client created or joined via an external commit.
//!
//! Both entry points reconstruct the MLS state from the DS epoch snapshot of
//! the operation's epoch and the shared emulation epoch.

use aircommon::{
    identifiers::UserId,
    messages::{client_as::ConnectionOfferHash, client_ds_out::EpochSnapshotIn},
    mls_group_config::default_mls_group_join_config,
    time::TimeStamp,
};
use airprotos::client::component::AirComponent;
use anyhow::{Context, Result, bail, ensure};
use apqmls::{
    ApqMlsGroup,
    messages::{ApqRatchetTreeIn, VerifiableApqGroupInfo},
    validation::validate_apq_session_at_construction,
};
use mimi_room_policy::VerifiedRoomState;
use openmls::{
    components::vc_derivation_info::EpochId,
    group::MlsGroup,
    prelude::{MlsMessageBodyIn, group_info::VerifiableGroupInfo},
};

use crate::{
    clients::{api_clients::ApiClients, own_client_info::OwnClientInfo},
    db::access::WriteDbTransaction,
    groups::{
        Group, apq_group::PqGroup, group_bootstrap::GroupBootstrapContents,
        openmls_provider::AirOpenMlsProvider, store_connection_offer_psk,
    },
};

use super::{
    ensure_room_state_users_are_members, verify_member_credentials, verify_pq_signature_keys,
};

impl Group {
    /// Installs a group a sibling emulator client created.
    ///
    /// `snapshot` is the DS epoch snapshot of the creation epoch and
    /// `epoch_id` the emulation epoch the creating sibling derived from.
    pub(crate) async fn vc_join_at_creation(
        txn: &mut WriteDbTransaction<'_>,
        api_clients: &ApiClients,
        snapshot: EpochSnapshotIn,
        contents: &GroupBootstrapContents,
        epoch_id: EpochId,
        own_user_id: &UserId,
    ) -> Result<Self> {
        let EpochSnapshotIn {
            verifiable_group_info,
            ratchet_tree_in,
            room_state,
            pq,
            join_commit,
        } = snapshot;
        ensure!(
            join_commit.is_none(),
            "creation snapshot carries an external commit"
        );
        ensure_snapshot_group_ids(
            &verifiable_group_info,
            pq.as_ref().map(|pq| &pq.verifiable_group_info),
            contents,
        )?;

        let join_config = default_mls_group_join_config();
        let (mls_group, pq_group) = {
            let provider = AirOpenMlsProvider::new(txn.as_mut());
            match pq {
                Some(pq) => {
                    let group_info = VerifiableApqGroupInfo::new(
                        verifiable_group_info,
                        pq.verifiable_group_info,
                    );
                    let ratchet_tree = ApqRatchetTreeIn::new(ratchet_tree_in, pq.ratchet_tree_in);
                    let (t_group, pq_group) = ApqMlsGroup::vc_join_at_creation(
                        &provider,
                        &join_config,
                        group_info,
                        Some(ratchet_tree),
                        epoch_id,
                    )?
                    .into_groups();
                    (t_group, Some(pq_group))
                }
                None => (
                    MlsGroup::vc_join_at_creation(
                        &provider,
                        &join_config,
                        verifiable_group_info,
                        Some(ratchet_tree_in),
                        epoch_id,
                    )?,
                    None,
                ),
            }
        };

        Self::finish_sibling_join(
            txn,
            api_clients,
            mls_group,
            pq_group,
            room_state,
            contents,
            own_user_id,
        )
        .await
    }

    /// Installs a group a sibling emulator client joined via an external
    /// commit, by applying that commit on top of the pre-commit state in
    /// `snapshot`.
    ///
    /// APQ is out of scope for now. The DS proposal allowlist of
    /// `join_connection_group` rejects the `AppDataUpdate` proposal an APQ
    /// external commit carries.
    pub(crate) async fn vc_join_via_sibling_external_commit(
        txn: &mut WriteDbTransaction<'_>,
        api_clients: &ApiClients,
        snapshot: EpochSnapshotIn,
        contents: &GroupBootstrapContents,
        epoch_id: EpochId,
        own_user_id: &UserId,
        connection_offer_hash: Option<ConnectionOfferHash>,
    ) -> Result<Self> {
        let EpochSnapshotIn {
            verifiable_group_info,
            ratchet_tree_in,
            room_state,
            pq,
            join_commit,
        } = snapshot;
        ensure!(
            pq.is_none() && contents.pq_group_id.is_none(),
            "APQ group in a sibling external commit join"
        );
        ensure_snapshot_group_ids(&verifiable_group_info, None, contents)?;

        let join_commit = join_commit.context("join snapshot without the accepted commit")?;
        let MlsMessageBodyIn::PublicMessage(join_commit) = join_commit.extract() else {
            bail!("the accepted external commit is not a public message");
        };

        let mls_group = {
            let provider = AirOpenMlsProvider::new(txn.as_mut());
            // Only the accepting sibling received the connection offer, so the
            // PSK its commit references has to be installed from the blob.
            if let Some(hash) = connection_offer_hash {
                store_connection_offer_psk(&provider, verifiable_group_info.ciphersuite(), hash)?;
            }

            let mut staged = MlsGroup::vc_external_commit_join_builder()
                .with_config(default_mls_group_join_config())
                .skip_lifetime_validation()
                .with_ratchet_tree(ratchet_tree_in)
                .process_commit(&provider, verifiable_group_info, join_commit, epoch_id)?;
            ensure!(
                staged.app_data_update_proposals().next().is_none(),
                "unexpected AppDataUpdate proposal in a connection group external commit"
            );
            staged.with_app_data_dictionary_updates(None);
            staged.into_group(&provider)?
        };

        Self::finish_sibling_join(
            txn,
            api_clients,
            mls_group,
            None,
            room_state,
            contents,
            own_user_id,
        )
        .await
    }

    /// Validates and persists the reconstructed group, mirroring what the
    /// acting client's own join path checks.
    async fn finish_sibling_join(
        txn: &mut WriteDbTransaction<'_>,
        api_clients: &ApiClients,
        mls_group: MlsGroup,
        pq_group: Option<MlsGroup>,
        room_state: VerifiedRoomState,
        contents: &GroupBootstrapContents,
        own_user_id: &UserId,
    ) -> Result<Self> {
        // A bootstrap never installs the self group. The self group is the
        // emulation group, which a sibling joins through device linking.
        ensure!(
            !AirComponent::is_self_group_context(mls_group.extensions())
                && !pq_group.as_ref().is_some_and(|pq_group| {
                    AirComponent::is_self_group_context(pq_group.extensions())
                }),
            "group bootstrap for a group flagged as a self group"
        );
        ensure!(
            !OwnClientInfo::is_own_self_group(&mut *txn, mls_group.group_id()).await?,
            "group bootstrap for our own self group"
        );

        if let Some(pq_group) = &pq_group {
            validate_apq_session_at_construction(&mls_group, pq_group, |_, _| true)
                .context("invalid APQ session")?;
            verify_pq_signature_keys(&mls_group, pq_group)
                .context("T and PQ membership is not bound by matching signature keys")?;
        }

        let credentials = verify_member_credentials(&mut *txn, api_clients, &mls_group, false)
            .await
            .context("failed to verify the member credentials")?;
        ensure_room_state_users_are_members(&room_state, &mls_group)?;

        let now = TimeStamp::now();
        let group = Self {
            identity_link_wrapper_key: contents.identity_link_wrapper_key.clone(),
            group_state_ear_key: contents.group_state_ear_key.clone(),
            mls_group,
            room_state,
            pending_diff: None,
            self_updated_at: Some(now),
            pq: pq_group.map(|mls_group| PqGroup {
                mls_group,
                self_updated_at: Some(now),
            }),
            pending_commit_failed: false,
            send_message_collision_key: None,
            own_user_id: own_user_id.clone(),
        };

        group.store(&mut *txn).await?;
        for credential in &credentials {
            credential.store(&mut *txn).await?;
        }

        Ok(group)
    }
}

/// Binds the blob's group ids to the group infos the DS served.
fn ensure_snapshot_group_ids(
    group_info: &VerifiableGroupInfo,
    pq_group_info: Option<&VerifiableGroupInfo>,
    contents: &GroupBootstrapContents,
) -> Result<()> {
    ensure!(
        group_info.group_id() == &contents.group_id,
        "group bootstrap and snapshot disagree on the group id"
    );
    match (pq_group_info, &contents.pq_group_id) {
        (Some(pq_group_info), Some(pq_group_id)) => ensure!(
            pq_group_info.group_id() == pq_group_id,
            "group bootstrap and snapshot disagree on the pq group id"
        ),
        (None, None) => (),
        (Some(_), None) | (None, Some(_)) => {
            bail!("group bootstrap and snapshot disagree on whether the group is an APQ group")
        }
    }
    Ok(())
}
