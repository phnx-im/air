// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::HashMap, ops::Deref};

use aircommon::{
    credentials::{
        AsIntermediateCredential, AsIntermediateCredentialBody, ClientCredential,
        VerifiableClientCredential,
    },
    crypto::{
        hash::Hash,
        signatures::{private_keys::VerifyingKeyRef, signable::Verifiable},
    },
    identifiers::UserId,
};
use anyhow::{Context, Result, anyhow, ensure};
use openmls::{
    group::GroupId,
    prelude::{LeafNodeIndex, SignaturePublicKey},
};
use sqlx::SqliteExecutor;

pub(crate) mod persistence;

#[derive(Debug, Clone)]
pub(crate) struct StorableClientCredential {
    client_credential: ClientCredential,
}

impl From<ClientCredential> for StorableClientCredential {
    fn from(client_credential: ClientCredential) -> Self {
        Self { client_credential }
    }
}

impl From<StorableClientCredential> for ClientCredential {
    fn from(storable_client_credential: StorableClientCredential) -> Self {
        storable_client_credential.client_credential
    }
}

impl Deref for StorableClientCredential {
    type Target = ClientCredential;

    fn deref(&self) -> &Self::Target {
        &self.client_credential
    }
}

impl StorableClientCredential {
    pub(crate) fn new(client_credential: ClientCredential) -> Self {
        Self { client_credential }
    }

    pub(crate) fn verify(
        verifiable_client_credential: VerifiableClientCredential,
        as_credentials: &HashMap<Hash<AsIntermediateCredentialBody>, AsIntermediateCredential>,
    ) -> Result<Self> {
        let as_credential = as_credentials
            .get(verifiable_client_credential.signer_fingerprint())
            .context("Missing AS credential")?;
        let client_credential =
            verifiable_client_credential.verify(as_credential.verifying_key())?;
        Ok(Self { client_credential })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct GroupMembership {
    user_id: UserId,
    group_id: GroupId,
    leaf_index: LeafNodeIndex,
}

impl GroupMembership {
    pub(super) fn new(user_id: UserId, group_id: GroupId, leaf_index: LeafNodeIndex) -> Self {
        Self {
            user_id,
            group_id,
            leaf_index,
        }
    }

    // Computes free indices based on existing leaf indices and staged removals.
    // Not that staged additions are not considered.
    pub(super) async fn free_indices(
        executor: impl SqliteExecutor<'_>,
        group_id: &GroupId,
    ) -> Result<impl Iterator<Item = LeafNodeIndex> + 'static> {
        let leaf_indices = Self::member_indices(executor, group_id).await?;
        let highest_index = leaf_indices
            .last()
            .cloned()
            .unwrap_or(LeafNodeIndex::new(0));
        let free_indices = (0..highest_index.u32())
            .filter(move |index| !leaf_indices.contains(&LeafNodeIndex::new(*index)))
            .chain(highest_index.u32() + 1..)
            .map(LeafNodeIndex::new);
        Ok(free_indices)
    }

    /// Set the group member's leaf index. This can be required for resync
    /// operations.
    pub(super) fn set_leaf_index(&mut self, leaf_index: LeafNodeIndex) {
        self.leaf_index = leaf_index;
    }

    pub(crate) fn user_id(&self) -> &UserId {
        &self.user_id
    }
}

pub(super) struct ClientAuthInfo {
    client_credential: StorableClientCredential,
    group_membership: GroupMembership,
}

pub(super) struct ClientVerificationInfo {
    pub(super) leaf_index: LeafNodeIndex,
    pub(super) credential: VerifiableClientCredential,
    pub(super) leaf_key: SignaturePublicKey,
}

impl ClientAuthInfo {
    pub(super) fn new(
        client_credential: impl Into<StorableClientCredential>,
        group_membership: GroupMembership,
    ) -> Self {
        Self {
            client_credential: client_credential.into(),
            group_membership,
        }
    }

    /// Verify the given credentials
    pub(super) fn verify_new_credentials(
        group_id: &GroupId,
        client_credentials: impl IntoIterator<Item = ClientVerificationInfo>,
        as_credentials: &HashMap<Hash<AsIntermediateCredentialBody>, AsIntermediateCredential>,
    ) -> Result<Vec<Self>> {
        let mut client_auth_infos = Vec::new();
        for ClientVerificationInfo {
            leaf_index,
            credential,
            leaf_key,
        } in client_credentials
        {
            let client_auth_info = Self::verify_credential(
                group_id,
                leaf_index,
                credential,
                leaf_key,
                None,
                as_credentials,
            )?;
            client_auth_infos.push(client_auth_info);
        }
        Ok(client_auth_infos)
    }

    /// Verify the given credential
    pub(super) fn verify_credential(
        group_id: &GroupId,
        leaf_index: LeafNodeIndex,
        credential: VerifiableClientCredential,
        leaf_signature_key: SignaturePublicKey,
        old_credential: Option<VerifiableClientCredential>,
        as_credentials: &HashMap<Hash<AsIntermediateCredentialBody>, AsIntermediateCredential>,
    ) -> Result<Self> {
        // Verify the leaf credential
        let client_credential = StorableClientCredential::verify(credential, as_credentials)?;
        // Check if the client credential matches the given public key
        ensure!(
            client_credential.verifying_key().as_ref()
                == VerifyingKeyRef::from(&leaf_signature_key),
            "Client credential does not match leaf public key"
        );
        // If it's an update, ensure that the UserId in the new credential
        // matches the UserId in the old credential
        if let Some(old_credential) = old_credential {
            ensure!(
                client_credential.identity() == old_credential.user_id(),
                "UserId in new credential does not match UserId in old credential"
            );
        }

        let group_membership = GroupMembership::new(
            client_credential.identity().clone(),
            group_id.clone(),
            leaf_index,
        );
        let client_auth_info = ClientAuthInfo {
            client_credential,
            group_membership,
        };
        Ok(client_auth_info)
    }

    pub(super) async fn stage_update(
        &self,
        connection: &mut sqlx::SqliteConnection,
    ) -> sqlx::Result<()> {
        self.client_credential.store(&mut *connection).await?;
        self.group_membership.stage_update(&mut *connection).await?;
        Ok(())
    }

    pub(super) async fn stage_add(
        &self,
        connection: &mut sqlx::SqliteConnection,
    ) -> sqlx::Result<()> {
        self.client_credential.store(&mut *connection).await?;
        self.group_membership.stage_add(&mut *connection).await?;
        Ok(())
    }

    pub(crate) async fn store(&self, connection: &mut sqlx::SqliteConnection) -> Result<()> {
        self.client_credential.store(&mut *connection).await?;
        self.group_membership.store(&mut *connection).await?;
        Ok(())
    }

    pub(super) async fn load(
        connection: &mut sqlx::SqliteConnection,
        group_id: &GroupId,
        leaf_index: LeafNodeIndex,
    ) -> Result<Option<Self>> {
        let Some(group_membership) =
            GroupMembership::load(&mut *connection, group_id, leaf_index).await?
        else {
            return Ok(None);
        };
        let client_credential =
            StorableClientCredential::load_by_user_id(&mut *connection, &group_membership.user_id)
                .await?
                .ok_or_else(|| {
                    anyhow!("Found a matching Groupmembership, but no matching ClientCredential")
                })?;
        Ok(Some(Self::new(client_credential, group_membership)))
    }

    pub(super) async fn load_staged(
        connection: &mut sqlx::SqliteConnection,
        group_id: &GroupId,
        leaf_index: LeafNodeIndex,
    ) -> Result<Option<Self>> {
        let Some(group_membership) =
            GroupMembership::load_staged(&mut *connection, group_id, leaf_index).await?
        else {
            return Ok(None);
        };
        let client_credential =
            StorableClientCredential::load_by_user_id(connection, &group_membership.user_id)
                .await?
                .ok_or_else(|| {
                    anyhow!("Found a matching Groupmembership, but no matching ClientCredential")
                })?;
        Ok(Some(Self::new(client_credential, group_membership)))
    }

    pub(super) fn client_credential(&self) -> &StorableClientCredential {
        &self.client_credential
    }

    pub(super) fn into_client_credential(self) -> ClientCredential {
        self.client_credential.into()
    }

    pub(super) fn group_membership(&self) -> &GroupMembership {
        &self.group_membership
    }

    pub(super) fn group_membership_mut(&mut self) -> &mut GroupMembership {
        &mut self.group_membership
    }
}
