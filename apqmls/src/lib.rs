// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! # APQMLS
//!
//! This crate provides an implementation of
//! [APQMLS](https://datatracker.ietf.org/doc/draft-ietf-mls-combiner/), a
//! mechanism that combines two MLS groups, one with a traditional ciphersuite
//! and one with a post-quantum ciphersuite. An [APQMLS group](ApqMlsGroup)
//! provides PQ security and "full" updates, as well as updates to group
//! membership provide PQ FS and PCS. In addition, the traditional group can be
//! used independently, except for membership updates, e.g. Independent use of
//! the traditional group, for example, allows for cheaper non-PQ PCS updates.
//!
//! A APQMLS group can be run in one of two modes: "confidentiality and
//! authentication" mode, where the post-quantum ciphersuite provides both both
//! PQ confidentiality and PQ authentication, and "confidentiality-only" mode,
//! where the post-quantum ciphersuite only provides PQ confidentiality with
//! traditional authentication.

use openmls::{
    group::{GroupEpoch, GroupId, MlsGroup},
    prelude::{Ciphersuite, LeafNodeIndex, OpenMlsRand},
    storage::StorageProvider,
};
use serde::{Deserialize, Serialize};
use tls_codec::{TlsSerialize, TlsSize};

use crate::{authentication::ApqVerifyingKey, group_builder::GroupBuilder};

pub mod authentication;
pub mod commit_builder;
mod export;
pub mod extension;
pub mod external_commit_builder;
pub mod group_builder;
pub mod key_package;
mod merging;
pub mod messages;
pub mod processing;
mod psk;
pub mod public_group;
mod secret;
pub mod welcome;

/// The combined ciphersuite of a [`ApqMlsGroup`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Copy)]
pub struct ApqCiphersuite {
    t_ciphersuite: Ciphersuite,
    pq_ciphersuite: Ciphersuite,
}

impl ApqCiphersuite {
    pub const fn new(t_ciphersuite: Ciphersuite, pq_ciphersuite: Ciphersuite) -> Self {
        Self {
            t_ciphersuite,
            pq_ciphersuite,
        }
    }

    pub const fn default_pq_conf_and_auth() -> Self {
        Self {
            t_ciphersuite: Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519,
            pq_ciphersuite: Ciphersuite::MLS_256_MLKEM1024_AES256GCM_SHA512_MLDSA87,
        }
    }

    pub const fn default_pq_conf() -> Self {
        Self {
            t_ciphersuite: Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519,
            pq_ciphersuite: Ciphersuite::MLS_192_MLKEM1024_AES256GCM_SHA384_P384,
        }
    }
}

/// The group ID of a [`ApqMlsGroup`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TlsSize, TlsSerialize)]
pub struct ApqGroupId {
    t_group_id: GroupId,
    pq_group_id: GroupId,
}

impl ApqGroupId {
    pub fn random(rng: &impl OpenMlsRand) -> Self {
        Self {
            t_group_id: GroupId::random(rng),
            pq_group_id: GroupId::random(rng),
        }
    }

    pub fn t_group_id(&self) -> &GroupId {
        &self.t_group_id
    }

    pub fn pq_group_id(&self) -> &GroupId {
        &self.pq_group_id
    }
}

/// An APQMLS group, consisting of a traditional MLS group and a post-quantum MLS group.
///
/// The two groups can be used independently, except for membership updates.
#[derive(Debug)]
pub struct ApqMlsGroup {
    pq_group: MlsGroup,
    pub t_group: MlsGroup,
}

/// Same as [`ApqMlsGroup`], but references MLS groups instead of owning them.
pub struct ApqMlsGroupMut<'a> {
    t_group: &'a mut MlsGroup,
    pq_group: &'a mut MlsGroup,
}

impl<'a> ApqMlsGroupMut<'a> {
    /// A non-owning version of [`ApqMlsGroupMut::from_groups`].
    pub fn from_groups(t_group: &'a mut MlsGroup, pq_group: &'a mut MlsGroup) -> Self {
        Self { t_group, pq_group }
    }
}

impl ApqMlsGroup {
    /// Create a new APQMLS group from the traditional and post-quantum MLS groups.
    pub fn from_groups(t_group: MlsGroup, pq_group: MlsGroup) -> Self {
        Self { t_group, pq_group }
    }

    pub fn as_mut(&mut self) -> ApqMlsGroupMut<'_> {
        ApqMlsGroupMut::from_groups(&mut self.t_group, &mut self.pq_group)
    }

    pub fn into_groups(self) -> (MlsGroup, MlsGroup) {
        (self.t_group, self.pq_group)
    }

    /// Returns a reference to the post-quantum group.
    pub fn pq_group(&self) -> &MlsGroup {
        &self.pq_group
    }

    /// Build a new APQMLS group.
    pub fn builder() -> GroupBuilder {
        GroupBuilder::new()
    }

    /// Creates a commit builder for the APQMLS group. This builder can be used
    /// to affect membership changes and issue full updates.
    pub fn commit_builder(&mut self) -> commit_builder::CommitBuilder<'_> {
        commit_builder::CommitBuilder::new(self)
    }

    /// Returns the group ID of the APQMLS group.
    pub fn group_id(&self) -> ApqGroupId {
        ApqGroupId {
            t_group_id: self.t_group.group_id().clone(),
            pq_group_id: self.pq_group.group_id().clone(),
        }
    }

    pub fn ciphersuite(&self) -> ApqCiphersuite {
        ApqCiphersuite {
            t_ciphersuite: self.t_group.ciphersuite(),
            pq_ciphersuite: self.pq_group.ciphersuite(),
        }
    }

    /// Returns the current epoch of the traditional group.
    pub fn t_epoch(&self) -> GroupEpoch {
        self.t_group.epoch()
    }

    /// Returns the current epoch of the post-quantum group.
    pub fn pq_epoch(&self) -> GroupEpoch {
        self.pq_group.epoch()
    }

    /// Returns the [`ApqVerifyingKey`] of the member at the given index.
    pub fn verifying_key_at(&self, index: LeafNodeIndex) -> Option<ApqVerifyingKey> {
        let t_member = self.t_group.member_at(index)?;
        let pq_member = self.pq_group.member_at(index)?;
        Some(ApqVerifyingKey {
            t_verifying_key: t_member.signature_key.into(),
            pq_verifying_key: pq_member.signature_key.into(),
        })
    }

    /// Load an APQMLS group from storage.
    pub fn load<Storage: StorageProvider>(
        provider: &Storage,
        group_id: &ApqGroupId,
    ) -> Result<Option<Self>, Storage::Error> {
        let t_group = MlsGroup::load(provider, &group_id.t_group_id)?;
        let pq_group = MlsGroup::load(provider, &group_id.pq_group_id)?;

        Ok(t_group
            .zip(pq_group)
            .map(|(t_group, pq_group)| Self { pq_group, t_group }))
    }

    /// Delete the APQMLS group from storage.
    pub fn delete<Storage: StorageProvider>(
        &mut self,
        provider: &Storage,
    ) -> Result<(), Storage::Error> {
        self.t_group.delete(provider)?;
        self.pq_group.delete(provider)?;
        Ok(())
    }
}
