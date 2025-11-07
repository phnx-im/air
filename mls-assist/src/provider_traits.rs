// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::storage::PublicStorageProvider;
use openmls_traits::{
    crypto::OpenMlsCrypto,
    random::OpenMlsRand,
    storage::{CURRENT_VERSION, traits::GroupId},
};
use serde::{Serialize, de::DeserializeOwned};

use crate::group::errors::StorageError;

pub trait MlsAssistStorageProvider: PublicStorageProvider {
    fn write_past_group_states(
        &self,
        group_id: &impl GroupId<CURRENT_VERSION>,
        past_group_states: &impl Serialize,
    ) -> Result<(), StorageError<Self>>;

    fn read_past_group_states<PastGroupStates: DeserializeOwned>(
        &self,
        group_id: &impl GroupId<CURRENT_VERSION>,
    ) -> Result<Option<PastGroupStates>, StorageError<Self>>;

    fn delete_past_group_states(
        &self,
        group_id: &impl GroupId<CURRENT_VERSION>,
    ) -> Result<(), StorageError<Self>>;

    fn write_group_info(
        &self,
        group_id: &impl GroupId<CURRENT_VERSION>,
        group_info: &impl Serialize,
    ) -> Result<(), StorageError<Self>>;

    fn read_group_info<GroupInfo: DeserializeOwned>(
        &self,
        group_id: &impl GroupId<CURRENT_VERSION>,
    ) -> Result<Option<GroupInfo>, StorageError<Self>>;

    fn delete_group_info(
        &self,
        group_id: &impl GroupId<CURRENT_VERSION>,
    ) -> Result<(), StorageError<Self>>;
}

/// A storage provider for MLS-assist.
pub trait MlsAssistProvider {
    type Storage: MlsAssistStorageProvider;
    type Crypto: OpenMlsCrypto;
    type Rand: OpenMlsRand;

    fn storage(&self) -> &Self::Storage;

    fn crypto(&self) -> &Self::Crypto;

    fn rand(&self) -> &Self::Rand;
}
