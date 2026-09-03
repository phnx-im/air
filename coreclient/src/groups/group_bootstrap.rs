// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Sealing and opening of group bootstrap blobs on the self group.
//!
//! When one emulator client creates a group or joins one via external commit,
//! it hands its siblings the group's keys and connection context in a
//! `GroupBootstrap` (see `airprotos::client::group_bootstrap`). The blob key
//! is derived from one application-ratchet generation of the self group's
//! newest emulation epoch, whose coordinates travel in the clear inside the
//! blob.

use aircommon::crypto::{
    aead::{AEAD_KEY_SIZE, keys::GroupBootstrapKey},
    kdf::{KdfDerivable, keys::VcApplicationSecret},
};
use airprotos::client::group_bootstrap::{
    GroupBootstrap, GroupBootstrapBlob, GroupBootstrapCarrier,
};
use anyhow::{Context, Result, anyhow, ensure};
use openmls::prelude::GroupId;

use crate::{
    db::access::WriteDbTransaction,
    groups::{Group, openmls_provider::AirOpenMlsProvider},
};

/// Domain separation of the application-ratchet derivation that feeds the
/// group bootstrap key.
const OPERATION_CONTEXT: &[u8] = b"group bootstrap";

impl Group {
    /// Seals `bootstrap` for the sibling emulator clients of this self group.
    ///
    /// Errors if called on a group that is not the self group.
    pub(crate) fn seal_group_bootstrap(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        bootstrap: &GroupBootstrap,
        carrier: GroupBootstrapCarrier,
        group_id: &GroupId,
    ) -> Result<GroupBootstrapBlob> {
        ensure!(
            self.is_self_group(),
            "seal_group_bootstrap must only be called on the self group"
        );
        let provider = AirOpenMlsProvider::new(txn.as_mut());
        let (info, secret) = self
            .mls_group()
            .next_vc_application_secret(&provider, OPERATION_CONTEXT)?;
        let key = bootstrap_key(secret)?;
        let sealed = bootstrap.seal(&key, carrier, group_id)?;
        Ok(GroupBootstrapBlob::new(&info, sealed))
    }

    /// Opens a blob a sibling emulator client sealed on this self group.
    ///
    /// Errors if called on a group that is not the self group, or if the blob
    /// names this client's own ratchet.
    pub(crate) fn open_group_bootstrap(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        blob: &GroupBootstrapBlob,
        carrier: GroupBootstrapCarrier,
        group_id: &GroupId,
    ) -> Result<GroupBootstrap> {
        ensure!(
            self.is_self_group(),
            "open_group_bootstrap must only be called on the self group"
        );
        let info = blob
            .secret_info()
            .context("group bootstrap blob without secret coordinates")?;
        let ciphertext = blob
            .encrypted_bootstrap
            .as_ref()
            .context("group bootstrap blob without ciphertext")?;
        let provider = AirOpenMlsProvider::new(txn.as_mut());
        let secret =
            self.mls_group()
                .derive_vc_application_secret(&provider, &info, OPERATION_CONTEXT)?;
        let key = bootstrap_key(secret)?;
        Ok(GroupBootstrap::open(&key, ciphertext, carrier, group_id)?)
    }
}

fn bootstrap_key(secret: Vec<u8>) -> Result<GroupBootstrapKey> {
    let secret: [u8; AEAD_KEY_SIZE] = secret
        .try_into()
        .map_err(|_| anyhow!("unexpected vc application secret length"))?;
    let secret = VcApplicationSecret::from_bytes(secret);
    // `secret` is zeroized as it is dropped (its inner secret is
    // `ZeroizeOnDrop`).
    Ok(GroupBootstrapKey::derive(&secret, &Vec::new())?)
}

#[cfg(test)]
mod tests {
    use aircommon::{
        credentials::{
            keys::{LeafSigningKey, SelfGroupSigningKey},
            test_utils::create_test_credentials,
        },
        crypto::aead::keys::IdentityLinkWrapperKey,
        identifiers::{QualifiedGroupId, UserId},
        mls_group_config::AppComponent,
    };
    use airprotos::client::component::AirComponent;
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

    /// Creates a fresh single-member APQ group. When `is_self_group` is set,
    /// the group is created as an emulation group, so it has a derivation
    /// epoch to seal from.
    fn create_group(
        txn: &mut WriteDbTransaction<'_>,
        is_self_group: bool,
    ) -> anyhow::Result<Group> {
        let user_id = UserId::random("example.com".parse()?);
        let signer = if is_self_group {
            LeafSigningKey::SelfGroup(SelfGroupSigningKey::generate(Uuid::new_v4())?)
        } else {
            let (_as_key, client_signer) = create_test_credentials(user_id.clone());
            LeafSigningKey::User(client_signer)
        };
        let air_component = if is_self_group {
            AirComponent::default_for_self_group()
        } else {
            AirComponent::default_for_leaf_or_key_package()
        };
        let (group, _params) = Group::create_apq_group(
            &mut *txn,
            &signer,
            user_id,
            IdentityLinkWrapperKey::random()?,
            random_group_id(),
            random_group_id(),
            GroupDataBytes::from(b"test-group-data".to_vec()),
            None,
            air_component,
        )?;
        Ok(group)
    }

    fn carried_group_id() -> GroupId {
        GroupId::from_slice(b"carried-group-id")
    }

    fn sample_bootstrap(group_id: &GroupId) -> GroupBootstrap {
        GroupBootstrap {
            group_id: Some(group_id.as_slice().to_vec()),
            pq_group_id: None,
            group_state_ear_key: None,
            identity_link_wrapper_key: None,
            connection: None,
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn seal_produces_blob_with_fresh_coordinates() -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;

        let group = create_group(&mut txn, true)?;
        let bootstrap = sample_bootstrap(&carried_group_id());

        let blob = group.seal_group_bootstrap(
            &mut txn,
            &bootstrap,
            GroupBootstrapCarrier::CreationEcho,
            &carried_group_id(),
        )?;
        let info = blob.secret_info().expect("coordinates must be present");
        assert_eq!(info.leaf_index, group.mls_group().own_leaf_index());
        assert_eq!(info.generation, 0);
        assert!(blob.encrypted_bootstrap.is_some());

        // A second seal consumes the next generation.
        let blob = group.seal_group_bootstrap(
            &mut txn,
            &bootstrap,
            GroupBootstrapCarrier::CreationEcho,
            &carried_group_id(),
        )?;
        assert_eq!(blob.secret_info().unwrap().generation, 1);

        txn.commit().await?;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn seal_requires_self_group() -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;

        let group = create_group(&mut txn, false)?;
        let bootstrap = sample_bootstrap(&carried_group_id());

        let result = group.seal_group_bootstrap(
            &mut txn,
            &bootstrap,
            GroupBootstrapCarrier::CreationEcho,
            &carried_group_id(),
        );
        assert!(result.is_err());

        txn.commit().await?;
        Ok(())
    }

    /// A blob names its sender's ratchet, so the sender itself must not be
    /// able to open it: rederiving from the own leaf would burn the ratchet
    /// head.
    #[tokio::test(flavor = "multi_thread")]
    async fn open_rejects_own_blob() -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;

        let group = create_group(&mut txn, true)?;
        let bootstrap = sample_bootstrap(&carried_group_id());

        let blob = group.seal_group_bootstrap(
            &mut txn,
            &bootstrap,
            GroupBootstrapCarrier::CreationEcho,
            &carried_group_id(),
        )?;
        let result = group.open_group_bootstrap(
            &mut txn,
            &blob,
            GroupBootstrapCarrier::CreationEcho,
            &carried_group_id(),
        );
        assert!(result.is_err());

        txn.commit().await?;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn open_requires_coordinates_and_ciphertext() -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;

        let group = create_group(&mut txn, true)?;
        let blob = GroupBootstrapBlob {
            epoch_id: Some(b"emulation-epoch-id".to_vec()),
            sender_leaf_index: None,
            generation: Some(0),
            encrypted_bootstrap: None,
        };
        let result = group.open_group_bootstrap(
            &mut txn,
            &blob,
            GroupBootstrapCarrier::CreationEcho,
            &carried_group_id(),
        );
        assert!(result.is_err());

        txn.commit().await?;
        Ok(())
    }
}
