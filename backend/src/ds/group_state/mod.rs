// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use aircommon::{
    codec::PersistenceCodec,
    credentials::VerifiableClientCredential,
    crypto::{
        ear::{
            Ciphertext, EarDecryptable, EarEncryptable,
            keys::{EncryptedUserProfileKey, GroupStateEarKey},
        },
        errors::{DecryptionError, EncryptionError},
    },
    identifiers::{QsReference, SealedClientReference, UserId},
    messages::client_ds::WelcomeInfoParams,
    time::TimeStamp,
};
use mimi_room_policy::{MimiProposal, RoleIndex, VerifiedRoomState};
use mls_assist::{
    MlsAssistRustCrypto,
    group::Group,
    openmls::{
        group::GroupId,
        prelude::{GroupEpoch, LeafNodeIndex},
        treesync::RatchetTree,
    },
    provider_traits::MlsAssistProvider,
};
use sqlx::PgExecutor;
use thiserror::Error;
use tls_codec::{Serialize as _, TlsDeserializeBytes, TlsSerialize, TlsSize, VLBytes};
use tracing::error;
use uuid::Uuid;

use crate::errors::{CborMlsAssistStorage, StorageError};

use super::{GROUP_STATE_EXPIRATION, ReservedGroupId, process::ExternalCommitInfo};

pub(super) mod persistence;

#[derive(Debug, TlsSize, TlsDeserializeBytes, TlsSerialize)]
pub(super) struct MemberProfile {
    pub(super) leaf_index: LeafNodeIndex,
    pub(super) client_queue_config: QsReference,
    pub(super) activity_time: TimeStamp,
    pub(super) activity_epoch: GroupEpoch,
    pub(super) encrypted_user_profile_key: EncryptedUserProfileKey,
}

/// The `DsGroupState` is the per-group state that the DS persists.
/// It is encrypted-at-rest with a roster key.
///
/// TODO: Past group states are now included in mls-assist. However, we might
/// have to store client credentials externally.
pub(crate) struct DsGroupState {
    pub(super) room_state: VerifiedRoomState,
    pub(super) group: Group,
    pub(super) provider: MlsAssistRustCrypto<PersistenceCodec>,
    pub(super) member_profiles: BTreeMap<LeafNodeIndex, MemberProfile>,
    pub(super) proposals: Vec<Vec<u8>>,
}

impl DsGroupState {
    pub(crate) fn new(
        provider: MlsAssistRustCrypto<PersistenceCodec>,
        group: Group,
        creator_encrypted_user_profile_key: EncryptedUserProfileKey,
        creator_queue_config: QsReference,
        room_state: VerifiedRoomState,
    ) -> Self {
        let creator_client_profile = MemberProfile {
            client_queue_config: creator_queue_config,
            activity_time: TimeStamp::now(),
            activity_epoch: 0u64.into(),
            leaf_index: LeafNodeIndex::new(0u32),
            encrypted_user_profile_key: creator_encrypted_user_profile_key,
        };

        let client_profiles = [(LeafNodeIndex::new(0u32), creator_client_profile)].into();
        Self {
            provider,
            group,
            room_state,
            member_profiles: client_profiles,
            proposals: Vec::new(),
        }
    }

    pub(crate) fn room_state_change_role(
        &mut self,
        sender: &UserId,
        target: &UserId,
        role: RoleIndex,
    ) -> Option<()> {
        let Ok(sender) = sender.tls_serialize_detached() else {
            return None;
        };

        let Ok(target) = target.tls_serialize_detached() else {
            return None;
        };

        match self
            .room_state
            .apply_regular_proposals(&sender, &[MimiProposal::ChangeRole { target, role }])
        {
            Ok(_) => Some(()),
            Err(e) => {
                error!(%e, "Change role proposal failed");
                None
            }
        }
    }

    /// Get a reference to the public group state.
    pub(crate) fn group(&self) -> &Group {
        &self.group
    }

    /// Get a mutable reference to the public group state.
    pub(crate) fn group_mut(&mut self) -> &mut Group {
        &mut self.group
    }

    pub(super) fn welcome_info(
        &mut self,
        welcome_info_params: WelcomeInfoParams,
    ) -> Option<&RatchetTree> {
        self.group_mut().past_group_state(
            &welcome_info_params.epoch,
            &welcome_info_params.sender.into(),
        )
    }

    pub(super) fn external_commit_info(&self) -> ExternalCommitInfo {
        let group_info = self.group().group_info().clone();
        let ratchet_tree = self.group().export_ratchet_tree();
        let proposals = self.proposals.clone();
        let encrypted_user_profile_keys = self.encrypted_user_profile_keys();
        ExternalCommitInfo {
            group_info,
            ratchet_tree,
            room_state: self.room_state.clone(),
            encrypted_user_profile_keys,
            proposals,
        }
    }

    /// Create a vector of encrypted user profile keys from the current list of
    /// client records.
    pub(super) fn encrypted_user_profile_keys(&self) -> Vec<EncryptedUserProfileKey> {
        self.member_profiles
            .values()
            .map(|client_profile| client_profile.encrypted_user_profile_key.clone())
            .collect()
    }

    pub(super) fn encrypt(
        self,
        ear_key: &GroupStateEarKey,
    ) -> Result<EncryptedDsGroupState, DsGroupStateEncryptionError> {
        let encrypted =
            EncryptableDsGroupState::from(SerializableDsGroupStateV2::from_group_state(self)?)
                .encrypt(ear_key)?;
        Ok(encrypted)
    }

    pub(super) fn decrypt(
        encrypted_group_state: &EncryptedDsGroupState,
        ear_key: &GroupStateEarKey,
    ) -> Result<Self, DsGroupStateDecryptionError> {
        let encryptable = EncryptableDsGroupState::decrypt(ear_key, encrypted_group_state)?;
        let group_state = SerializableDsGroupStateV2::into_group_state(encryptable.into())?;
        Ok(group_state)
    }

    pub(crate) fn destination_clients(&self) -> impl Iterator<Item = QsReference> {
        self.member_profiles
            .values()
            .map(|client_profile| client_profile.client_queue_config.clone())
    }

    pub(crate) fn other_destination_clients(
        &self,
        sender_index: LeafNodeIndex,
    ) -> impl Iterator<Item = QsReference> {
        self.member_profiles
            .iter()
            .filter_map(move |(client_index, client_profile)| {
                if client_index == &sender_index {
                    None
                } else {
                    Some(client_profile.client_queue_config.clone())
                }
            })
    }
}

#[derive(Debug, Error)]
pub(super) enum DsGroupStateEncryptionError {
    #[error("Error decrypting group state: {0}")]
    EncryptionError(#[from] EncryptionError),
    #[error("Error deserializing group state: {0}")]
    DeserializationError(#[from] aircommon::codec::Error),
}

impl From<DsGroupStateEncryptionError> for tonic::Status {
    fn from(error: DsGroupStateEncryptionError) -> Self {
        error!(%error, "failed to encrypt group state");
        Self::internal("failed to encrypt group state")
    }
}

#[derive(Debug, Error)]
pub(super) enum DsGroupStateDecryptionError {
    #[error("Error decrypting group state: {0}")]
    DecryptionError(#[from] DecryptionError),
    #[error("Error deserializing group state: {0}")]
    DeserializationError(#[from] aircommon::codec::Error),
}

impl From<DsGroupStateDecryptionError> for tonic::Status {
    fn from(error: DsGroupStateDecryptionError) -> Self {
        error!(%error, "failed to decrypt group state");
        Self::internal("failed to decrypt group state")
    }
}

#[derive(Debug)]
pub struct EncryptedDsGroupStateCtype;
pub type EncryptedDsGroupState = Ciphertext<EncryptedDsGroupStateCtype>;

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub(super) struct StorableDsGroupData {
    group_id: Uuid,
    pub(super) encrypted_group_state: EncryptedDsGroupState,
    last_used: TimeStamp,
    deleted_queues: Vec<SealedClientReference>,
}

impl StorableDsGroupData {
    pub(super) async fn new_and_store<'a>(
        connection: impl PgExecutor<'a>,
        group_id: ReservedGroupId,
        encrypted_group_state: EncryptedDsGroupState,
    ) -> Result<Self, StorageError> {
        let group_data = Self {
            group_id: group_id.0,
            encrypted_group_state,
            last_used: TimeStamp::now(),
            deleted_queues: vec![],
        };
        group_data.store(connection).await?;
        Ok(group_data)
    }

    pub(super) fn has_expired(&self) -> bool {
        self.last_used.has_expired(GROUP_STATE_EXPIRATION)
    }
}

#[derive(TlsSize, TlsDeserializeBytes, TlsSerialize)]
pub(crate) struct SerializableDsGroupStateV1 {
    group_id: GroupId,
    serialized_provider: VLBytes,
    room_state: VLBytes,
    member_profiles: Vec<(LeafNodeIndex, MemberProfile)>,
}

impl From<SerializableDsGroupStateV1> for SerializableDsGroupStateV2 {
    fn from(v1: SerializableDsGroupStateV1) -> Self {
        Self {
            group_id: v1.group_id,
            serialized_provider: v1.serialized_provider,
            room_state: v1.room_state,
            member_profiles: v1.member_profiles,
            proposals: vec![],
        }
    }
}

#[derive(TlsSize, TlsDeserializeBytes, TlsSerialize)]
pub(crate) struct SerializableDsGroupStateV2 {
    group_id: GroupId,
    serialized_provider: VLBytes,
    room_state: VLBytes,
    member_profiles: Vec<(LeafNodeIndex, MemberProfile)>,
    // Proposals that are valid in external commits in TLS-serialized form
    proposals: Vec<Vec<u8>>,
}

impl SerializableDsGroupStateV2 {
    pub(super) fn from_group_state(
        group_state: DsGroupState,
    ) -> Result<Self, aircommon::codec::Error> {
        let group_id = group_state
            .group()
            .group_info()
            .group_context()
            .group_id()
            .clone();
        let client_profiles = group_state.member_profiles.into_iter().collect();
        let serialized_provider = group_state.provider.storage().serialize()?.into();
        let room_state = PersistenceCodec::to_vec(group_state.room_state.unverified())?.into();
        Ok(Self {
            group_id,
            serialized_provider,
            member_profiles: client_profiles,
            room_state,
            proposals: group_state.proposals,
        })
    }

    pub(super) fn into_group_state(self) -> Result<DsGroupState, aircommon::codec::Error> {
        let storage = CborMlsAssistStorage::deserialize(self.serialized_provider.as_slice())?;
        // We unwrap here, because the constructor ensures that `self` always stores a group
        let group = Group::load(&storage, &self.group_id)?.unwrap();
        let client_profiles = self.member_profiles.into_iter().collect();
        let provider = MlsAssistRustCrypto::from(storage);

        let room_state = PersistenceCodec::from_slice(self.room_state.as_slice())
            .inspect_err(|error| {
                error!(%error, "Failed to load room state. Falling back to default room state.");
            })
            .ok()
            .and_then(|state| {
                VerifiedRoomState::verify(state).inspect_err(|error| {
                error!(%error, "Failed to verify room state. Falling back to default room state.");
            }).ok()
            })
            .unwrap_or_else(|| fallback_room_state(group.members()));

        Ok(DsGroupState {
            provider,
            group,
            member_profiles: client_profiles,
            room_state,
            proposals: self.proposals,
        })
    }
}

fn fallback_room_state(
    members: impl Iterator<Item = mls_assist::openmls::prelude::Member>,
) -> VerifiedRoomState {
    let mut member_ids = Vec::new();
    for member in members {
        let credential = match VerifiableClientCredential::try_from(member.credential) {
            Ok(credential) => credential,
            Err(error) => {
                error!(%error, "Failed to convert credential; skipping member");
                continue;
            }
        };
        let user_id = match credential.user_id().tls_serialize_detached() {
            Ok(bytes) => bytes,
            Err(error) => {
                error!(%error, "Failed to serialize user id; skipping member");
                continue;
            }
        };
        member_ids.push(user_id);
    }
    VerifiedRoomState::fallback_room(member_ids)
}

#[derive(TlsSize, TlsDeserializeBytes, TlsSerialize)]
#[repr(u8)]
pub(super) enum EncryptableDsGroupState {
    V1(SerializableDsGroupStateV1),
    V2(SerializableDsGroupStateV2),
}

impl From<EncryptableDsGroupState> for SerializableDsGroupStateV2 {
    fn from(encryptable: EncryptableDsGroupState) -> Self {
        match encryptable {
            EncryptableDsGroupState::V1(serializable) => serializable.into(),
            EncryptableDsGroupState::V2(serializable) => serializable,
        }
    }
}

impl From<SerializableDsGroupStateV2> for EncryptableDsGroupState {
    fn from(serializable: SerializableDsGroupStateV2) -> Self {
        EncryptableDsGroupState::V2(serializable)
    }
}

impl EarEncryptable<GroupStateEarKey, EncryptedDsGroupStateCtype> for EncryptableDsGroupState {}
impl EarDecryptable<GroupStateEarKey, EncryptedDsGroupStateCtype> for EncryptableDsGroupState {}

#[cfg(test)]
mod test {
    use std::sync::LazyLock;

    use mls_assist::openmls::prelude::HpkeCiphertext;

    use super::*;

    #[test]
    fn test_encrypted_ds_group_state_serde_codec() {
        let state = EncryptedDsGroupState::dummy();
        insta::assert_binary_snapshot!(".cbor", PersistenceCodec::to_vec(&state).unwrap());
    }

    #[test]
    fn test_encrypted_ds_group_state_serde_json() {
        let state = EncryptedDsGroupState::dummy();
        insta::assert_json_snapshot!(state);
    }

    static DELETED_QUEUES: LazyLock<Vec<SealedClientReference>> = LazyLock::new(|| {
        vec![
            SealedClientReference::from(HpkeCiphertext {
                kem_output: vec![1, 2, 3].into(),
                ciphertext: vec![4, 5, 6].into(),
            }),
            SealedClientReference::from(HpkeCiphertext {
                kem_output: vec![7, 8, 9].into(),
                ciphertext: vec![10, 11, 12].into(),
            }),
        ]
    });

    #[test]
    fn test_deleted_queues_serde_codec() {
        insta::assert_binary_snapshot!(
            ".cbor",
            PersistenceCodec::to_vec(&*DELETED_QUEUES).unwrap()
        );
    }

    #[test]
    fn test_deleted_queues_serde_json() {
        insta::assert_json_snapshot!(&*DELETED_QUEUES);
    }
}
