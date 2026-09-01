// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use aircommon::{
    codec::PersistenceCodec,
    credentials::{LeafCredential, RoomPolicyIdentity},
    crypto::{
        aead::{
            AeadDecryptable, AeadEncryptable, Ciphertext,
            keys::{EncryptedUserProfileKey, EncryptedUserProfileKeyCtype, GroupStateEarKey},
        },
        errors::{DecryptionError, EncryptionError},
    },
    identifiers::{QsReference, SealedClientReference},
    mls_group_config::leaf_node_is_virtual_client,
    time::TimeStamp,
    utils::removed_clients,
};
use airprotos::client::component::AirComponent;
use apqmls::extension::ApqInfo;
use mimi_room_policy::{MimiProposal, RoleIndex, RoomState, VerifiedRoomState};
use mls_assist::{
    MlsAssistRustCrypto,
    group::{Group, RetainedWelcomeInfo},
    openmls::{
        group::GroupId,
        prelude::{GroupEpoch, LeafNodeIndex, Proposal, Sender, StagedCommit},
    },
    provider_traits::MlsAssistProvider,
};
use sqlx::{PgExecutor, PgTransaction};
use thiserror::Error;
use tls_codec::{TlsDeserializeBytes, TlsSerialize, TlsSize, VLBytes};
use tracing::error;
use uuid::Uuid;

use crate::{
    ds::{WELCOME_INFO_EXPIRATION, welcome_info::WelcomeInfoWriteError},
    errors::{CborMlsAssistStorage, StorageError},
};

use super::{
    GROUP_STATE_EXPIRATION, ReservedGroupId,
    process::ExternalCommitInfo,
    welcome_info::{DsWelcomeInfo, WelcomeInfoOutbox},
};

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
/// TODO: We might have to store user credentials externally.
pub(crate) struct DsGroupState {
    pub(super) room_state: VerifiedRoomState,
    pub(super) group: Group,
    pub(super) provider: MlsAssistRustCrypto<PersistenceCodec>,
    pub(super) member_profiles: BTreeMap<LeafNodeIndex, MemberProfile>,
    pub(super) proposals: Vec<Vec<u8>>,

    /// Transient container for welcome infos (produced on `Add` proposals)
    /// that we need to persist for a later client to fetch.
    welcome_info_outbox: WelcomeInfoOutbox,
}

/// What a joiner needs about one epoch, as V3 stored it.
#[derive(Debug, TlsSize, TlsDeserializeBytes, TlsSerialize)]
pub(super) struct PastMemberProfiles {
    pub(super) created_at: TimeStamp,
    pub(super) profiles: Vec<(LeafNodeIndex, EncryptedUserProfileKey)>,
    /// The room state as of this epoch, serialized with [`PersistenceCodec`].
    pub(super) room_state: VLBytes,
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
            welcome_info_outbox: WelcomeInfoOutbox::default(),
        }
    }

    /// Apply a role change to the room state.
    ///
    /// Returns `None` (and logs) if either identity fails to serialize or if the room policy
    /// rejects the change.
    pub(super) fn room_state_change_role(
        &mut self,
        sender: &RoomPolicyIdentity,
        target: &RoomPolicyIdentity,
        role: RoleIndex,
    ) -> Option<()> {
        let sender = sender
            .to_bytes()
            .map_err(|error| error!(%error, "Failed to serialize sender identity"))
            .ok()?;
        let target = target
            .to_bytes()
            .map_err(|error| error!(%error, "Failed to serialize target identity"))
            .ok()?;
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

    /// Extract and parse the credential of the leaf at `index`.
    ///
    /// Returns `None` (and logs) if the leaf is missing or its credential is invalid.
    pub(crate) fn leaf_credential(&self, index: LeafNodeIndex) -> Option<LeafCredential> {
        let leaf = self.group().leaf(index).or_else(|| {
            error!(%index, "Leaf node not found");
            None
        })?;
        LeafCredential::from_credential(leaf.credential())
            .map_err(|error| error!(%error, "Credential is invalid"))
            .ok()
    }

    /// Returns true if the group context carries the APQMLS component, i.e. this group is a leg of
    /// an APQ group.
    pub(crate) fn is_apq(&self) -> bool {
        self.apq_info().is_some()
    }

    /// Extracts the APQMLS component from the group context extensions, if present.
    pub(crate) fn apq_info(&self) -> Option<ApqInfo> {
        let extensions = self.group().group_info().group_context().extensions();
        ApqInfo::from_extensions(extensions).ok().flatten()
    }

    /// Get a reference to the public group state.
    pub(crate) fn group(&self) -> &Group {
        &self.group
    }

    /// Stage the welcome information for the epoch `retained` refers to.
    ///
    /// Must be called after the commit's membership changes have been applied,
    /// so the recorded profile keys are the ones a joiner at that epoch needs.
    pub(super) fn stage_welcome_info(&mut self, retained: Option<RetainedWelcomeInfo>) {
        let Some(retained) = retained else { return };
        let (epoch, info) = DsWelcomeInfo::new(
            retained,
            self.current_member_profiles().collect(),
            &self.room_state,
        );
        self.welcome_info_outbox
            .stage(epoch, TimeStamp::now(), info);
    }

    /// Same as [`stage_welcome_info`] but
    /// without member profile keys as an optimisation.
    ///
    /// The client never uses them from the PQ leg of an APQ group.
    pub(super) fn stage_welcome_info_without_profile_keys(
        &mut self,
        retained: Option<RetainedWelcomeInfo>,
    ) {
        let Some(retained) = retained else { return };
        let (epoch, info) = DsWelcomeInfo::new(retained, Vec::new(), &self.room_state);
        self.welcome_info_outbox
            .stage(epoch, TimeStamp::now(), info);
    }

    pub(super) async fn write_staged_welcome_infos(
        &mut self,
        txn: &mut PgTransaction<'_>,
        group_id: Uuid,
        ear_key: &GroupStateEarKey,
    ) -> Result<(), WelcomeInfoWriteError> {
        let outbox = std::mem::take(&mut self.welcome_info_outbox);
        outbox.write(txn, group_id, ear_key).await
    }

    /// Serve welcome information from a V3 group state, for a joiner whose
    /// Welcome predates the move to `ds_welcome_info`.
    pub(super) fn legacy_welcome_info(self, epoch: GroupEpoch) -> Option<DsWelcomeInfo> {
        self.welcome_info_outbox.take(epoch)
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
            EncryptableDsGroupState::from(SerializableDsGroupStateV4::from_group_state(self)?)
                .encrypt(ear_key)?;
        Ok(encrypted)
    }

    pub(super) fn decrypt(
        encrypted_group_state: &EncryptedDsGroupState,
        ear_key: &GroupStateEarKey,
    ) -> Result<Self, DsGroupStateDecryptionError> {
        let encryptable = EncryptableDsGroupState::decrypt(ear_key, encrypted_group_state)?;
        let group_state = DecodedDsGroupState::from(encryptable).into_group_state()?;
        Ok(group_state)
    }

    pub(crate) fn destination_clients(&self) -> impl Iterator<Item = QsReference> {
        self.member_profiles
            .values()
            .map(|client_profile| client_profile.client_queue_config.clone())
    }

    /// The members that receive a message sent from `sender_index`.
    fn other_member_profiles(
        &self,
        sender_index: LeafNodeIndex,
    ) -> impl Iterator<Item = (&LeafNodeIndex, &MemberProfile)> {
        let is_sender_virtual_client = self.leaf_is_virtual_client(sender_index);
        self.member_profiles
            .iter()
            .filter(move |(client_index, _)| {
                *client_index != &sender_index || is_sender_virtual_client
            })
    }

    pub(crate) fn other_destination_clients(
        &self,
        sender_index: LeafNodeIndex,
    ) -> impl Iterator<Item = QsReference> {
        self.other_member_profiles(sender_index)
            .map(|(_, client_profile)| client_profile.client_queue_config.clone())
    }

    /// Returns `true` if the group context's [`AirComponent`] marks this group as a
    /// self-group. The flag is fixed at group creation.
    pub(crate) fn is_self_group(&self) -> bool {
        AirComponent::is_self_group_context(self.group().group_info().group_context().extensions())
    }

    /// If the group context's [`AirComponent`] marks this group
    /// as a virtual-client self-group, disable virtual-client broadcasting.
    pub(crate) fn broadcast_to_all_client_queues(&self) -> bool {
        !self.is_self_group()
    }

    /// The leaves that sent a self-remove proposal the DS has accepted but not
    /// seen committed yet.
    fn pending_self_remove_leaves(&self) -> Result<Vec<LeafNodeIndex>, ProposalStoreError> {
        let proposals = self
            .group()
            .queued_proposals(self.provider.storage())
            .map_err(|error| {
                error!(%error, "Failed to read the proposal store");
                ProposalStoreError
            })?;
        Ok(proposals
            .iter()
            .filter_map(|proposal| match (proposal.proposal(), proposal.sender()) {
                (Proposal::SelfRemove, Sender::Member(leaf_index)) => Some(*leaf_index),
                _ => None,
            })
            .collect())
    }

    /// The next commit after a self-remove proposal has to remove the proposing
    /// leaf, otherwise a group could keep a leaving member in indefinitely by
    /// committing around its proposal.
    ///
    /// Returns the leaves with a pending self-remove that `staged_commit`
    /// leaves in the group.
    pub(super) fn uncommitted_self_removes(
        &self,
        staged_commit: &StagedCommit,
    ) -> Result<Vec<LeafNodeIndex>, ProposalStoreError> {
        let removed = removed_clients(staged_commit);
        Ok(self
            .pending_self_remove_leaves()?
            .into_iter()
            .filter(|leaf_index| !removed.contains(leaf_index))
            .collect())
    }

    /// The self-group flag in the group context's [`AirComponent`] is fixed at
    /// group creation. Returns `true` if merging `staged_commit` keeps it
    /// unchanged.
    pub(crate) fn self_group_flag_unchanged(&self, staged_commit: &StagedCommit) -> bool {
        let current_extensions = self.group().group_info().group_context().extensions();
        AirComponent::is_self_group_context(staged_commit.group_context().extensions())
            == AirComponent::is_self_group_context(current_extensions)
    }

    /// Returns `true` if the leaf declares a `VC_COMPONENT_ID` entry in its `AppDataDictionary` extension.
    pub(super) fn leaf_is_virtual_client(&self, leaf_index: LeafNodeIndex) -> bool {
        self.group()
            .leaf(leaf_index)
            .is_some_and(leaf_node_is_virtual_client)
    }

    /// The queue reference recorded for `leaf_index`, if any.
    pub(super) fn queue_config_at(&self, leaf_index: LeafNodeIndex) -> Option<QsReference> {
        self.member_profiles
            .get(&leaf_index)
            .map(|profile| profile.client_queue_config.clone())
    }

    pub(crate) fn qs_client_ref_by_index(
        &self,
        member_index: LeafNodeIndex,
    ) -> Option<QsReference> {
        self.member_profiles
            .get(&member_index)
            .map(|cp| cp.client_queue_config.clone())
    }

    fn current_member_profiles(
        &self,
    ) -> impl Iterator<Item = (LeafNodeIndex, Ciphertext<EncryptedUserProfileKeyCtype>)> {
        self.member_profiles
            .iter()
            .map(|(index, profile)| (*index, profile.encrypted_user_profile_key.clone()))
    }
}

/// The proposal store could not be read, so the DS cannot tell whether a commit
/// leaves a self-remove behind.
#[derive(Debug, Error)]
#[error("Failed to read the proposal store")]
pub(super) struct ProposalStoreError;

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
pub(super) struct StorableDsGroupData<const LOADED_FOR_UPDATE: bool> {
    group_id: Uuid,
    pub(super) encrypted_group_state: EncryptedDsGroupState,
    last_used: TimeStamp,
    deleted_queues: Vec<SealedClientReference>,
}

impl StorableDsGroupData<false> {
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
}

impl<const LOADED_FOR_UPDATE: bool> StorableDsGroupData<LOADED_FOR_UPDATE> {
    #[allow(unused)]
    pub(super) fn has_expired(&self) -> bool {
        self.last_used.has_expired(GROUP_STATE_EXPIRATION)
    }

    pub(super) fn group_uuid(&self) -> Uuid {
        self.group_id
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

impl From<SerializableDsGroupStateV2> for SerializableDsGroupStateV3 {
    fn from(v2: SerializableDsGroupStateV2) -> Self {
        Self {
            group_id: v2.group_id,
            serialized_provider: v2.serialized_provider,
            room_state: v2.room_state,
            member_profiles: v2.member_profiles,
            proposals: v2.proposals,
            // No snapshots were retained before V3.
            past_member_profiles: Vec::new(),
        }
    }
}

#[derive(TlsSize, TlsDeserializeBytes, TlsSerialize)]
pub(crate) struct SerializableDsGroupStateV3 {
    group_id: GroupId,
    serialized_provider: VLBytes,
    room_state: VLBytes,
    member_profiles: Vec<(LeafNodeIndex, MemberProfile)>,
    // Proposals that are valid in external commits in TLS-serialized form
    proposals: Vec<Vec<u8>>,
    // Profile keys per epoch, so welcome info can answer at the epoch asked for
    past_member_profiles: Vec<(GroupEpoch, PastMemberProfiles)>,
}

impl From<SerializableDsGroupStateV3> for DecodedDsGroupState {
    fn from(v3: SerializableDsGroupStateV3) -> Self {
        Self {
            state: SerializableDsGroupStateV4 {
                group_id: v3.group_id,
                serialized_provider: v3.serialized_provider,
                room_state: v3.room_state,
                member_profiles: v3.member_profiles,
                proposals: v3.proposals,
            },
            // Kept for migration: a joiner whose Welcome
            // predates `ds_welcome_info` is still served from these.
            legacy_past_member_profiles: v3.past_member_profiles,
        }
    }
}

/// Welcome information lives in `ds_welcome_info` from V4 on.
#[derive(TlsSize, TlsDeserializeBytes, TlsSerialize)]
pub(crate) struct SerializableDsGroupStateV4 {
    group_id: GroupId,
    serialized_provider: VLBytes,
    room_state: VLBytes,
    member_profiles: Vec<(LeafNodeIndex, MemberProfile)>,
    // Proposals that are valid in external commits in TLS-serialized form
    proposals: Vec<Vec<u8>>,
}

/// A decoded group state, plus whatever legacy welcome information
/// the blob came with.
pub(super) struct DecodedDsGroupState {
    state: SerializableDsGroupStateV4,
    legacy_past_member_profiles: Vec<(GroupEpoch, PastMemberProfiles)>,
}

impl SerializableDsGroupStateV4 {
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
}

impl DecodedDsGroupState {
    pub(super) fn into_group_state(self) -> Result<DsGroupState, aircommon::codec::Error> {
        let state = self.state;
        let storage = CborMlsAssistStorage::deserialize(state.serialized_provider.as_slice())?;
        // We unwrap here, because the constructor ensures that `self` always stores a group
        let mut group = Group::load(&storage, &state.group_id)?.unwrap();
        let client_profiles: BTreeMap<LeafNodeIndex, MemberProfile> =
            state.member_profiles.into_iter().collect();
        let provider = MlsAssistRustCrypto::from(storage);

        let room_state = PersistenceCodec::from_slice(state.room_state.as_slice())
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

        let mut legacy_past_member_profiles: BTreeMap<_, _> =
            self.legacy_past_member_profiles.into_iter().collect();

        let mut staged_welcome_info = WelcomeInfoOutbox::default();
        for legacy in group.take_legacy_past_group_states() {
            if TimeStamp::from(legacy.creation_time).has_expired(WELCOME_INFO_EXPIRATION) {
                continue;
            }
            // Absent snapshots fall back to the current profiles and room
            // state, which is what serving this epoch did before.
            let (profiles, room_state) = match legacy_past_member_profiles.remove(&legacy.epoch) {
                Some(snapshot) => (snapshot.profiles, decode_room_state(&snapshot.room_state)),
                None => {
                    let current_member_profiles = client_profiles
                        .iter()
                        .map(|(index, profile)| {
                            (*index, profile.encrypted_user_profile_key.clone())
                        })
                        .collect();

                    (
                        current_member_profiles,
                        Some(room_state.unverified().clone()),
                    )
                }
            };

            let (epoch, created_at, info) =
                DsWelcomeInfo::from_legacy(legacy, profiles, room_state);

            staged_welcome_info.stage(epoch, created_at, info);
        }

        Ok(DsGroupState {
            provider,
            group,
            member_profiles: client_profiles,
            room_state,
            proposals: state.proposals,
            welcome_info_outbox: staged_welcome_info,
        })
    }
}

/// Decode a room state snapshotted by V3.
fn decode_room_state(bytes: &VLBytes) -> Option<RoomState> {
    PersistenceCodec::from_slice(bytes.as_slice())
        .inspect_err(|error| error!(%error, "failed to load snapshotted room state"))
        .ok()
}

/// Check that a leaf credential matches the group kind.
///
/// A self-group leaf must carry a [`LeafCredential::SelfGroup`], a regular-group leaf a
/// [`LeafCredential::User`]. Returns `true` if `credential` is consistent with `is_self_group`.
pub(super) fn leaf_credential_matches_flag(
    credential: &LeafCredential,
    is_self_group: bool,
) -> bool {
    matches!(credential, LeafCredential::SelfGroup(_)) == is_self_group
}

fn fallback_room_state(
    members: impl Iterator<Item = mls_assist::openmls::prelude::Member>,
) -> VerifiedRoomState {
    let mut member_ids = Vec::new();
    for member in members {
        let credential = match LeafCredential::from_credential(&member.credential) {
            Ok(credential) => credential,
            Err(error) => {
                error!(%error, "Failed to convert credential; skipping member");
                continue;
            }
        };
        match credential.room_policy_identity().to_bytes() {
            Ok(identity) => member_ids.push(identity),
            Err(error) => {
                error!(%error, "Failed to serialize room policy identity; skipping member");
            }
        }
    }
    VerifiedRoomState::fallback_room(member_ids)
}

#[derive(TlsSize, TlsDeserializeBytes, TlsSerialize)]
#[repr(u8)]
pub(super) enum EncryptableDsGroupState {
    V1(SerializableDsGroupStateV1),
    V2(SerializableDsGroupStateV2),
    V3(SerializableDsGroupStateV3),
    V4(SerializableDsGroupStateV4),
}

impl From<EncryptableDsGroupState> for DecodedDsGroupState {
    fn from(encryptable: EncryptableDsGroupState) -> Self {
        match encryptable {
            EncryptableDsGroupState::V1(serializable) => {
                SerializableDsGroupStateV3::from(SerializableDsGroupStateV2::from(serializable))
                    .into()
            }
            EncryptableDsGroupState::V2(serializable) => {
                SerializableDsGroupStateV3::from(serializable).into()
            }
            EncryptableDsGroupState::V3(serializable) => serializable.into(),
            EncryptableDsGroupState::V4(serializable) => Self {
                state: serializable,
                legacy_past_member_profiles: Vec::new(),
            },
        }
    }
}

impl From<SerializableDsGroupStateV4> for EncryptableDsGroupState {
    fn from(serializable: SerializableDsGroupStateV4) -> Self {
        EncryptableDsGroupState::V4(serializable)
    }
}

impl AeadEncryptable<GroupStateEarKey, EncryptedDsGroupStateCtype> for EncryptableDsGroupState {}
impl AeadDecryptable<GroupStateEarKey, EncryptedDsGroupStateCtype> for EncryptableDsGroupState {}

#[cfg(test)]
mod test {
    use std::sync::LazyLock;

    use mls_assist::openmls::prelude::HpkeCiphertext;

    use super::*;

    #[test]
    fn test_encrypted_ds_group_state_serde_codec() {
        let state = EncryptedDsGroupState::dummy();
        let bytes = PersistenceCodec::to_vec(&state).unwrap();
        let diag = cbor_diag::parse_bytes(&bytes[1..]).unwrap().to_hex();
        insta::assert_snapshot!(diag);
    }

    #[test]
    fn test_encrypted_ds_group_state_serde_json() {
        let state = EncryptedDsGroupState::dummy();
        insta::assert_json_snapshot!(state);
    }

    #[test]
    fn v3_welcome_info_survives_decoding() {
        let epoch = GroupEpoch::from(4);
        let v3 = SerializableDsGroupStateV3 {
            group_id: GroupId::from_slice(b"group"),
            serialized_provider: vec![].into(),
            room_state: vec![].into(),
            member_profiles: Vec::new(),
            proposals: Vec::new(),
            past_member_profiles: vec![(
                epoch,
                PastMemberProfiles {
                    created_at: TimeStamp::now(),
                    profiles: vec![(LeafNodeIndex::new(0), EncryptedUserProfileKey::dummy())],
                    room_state: vec![1u8, 2, 3].into(),
                },
            )],
        };

        let decoded = DecodedDsGroupState::from(EncryptableDsGroupState::V3(v3));

        assert_eq!(decoded.legacy_past_member_profiles.len(), 1);
        assert_eq!(decoded.legacy_past_member_profiles[0].0, epoch);
    }

    /// V4 writes no welcome information, so there is nothing to migrate.
    #[test]
    fn v4_carries_no_legacy_welcome_info() {
        let v4 = SerializableDsGroupStateV4 {
            group_id: GroupId::from_slice(b"group"),
            serialized_provider: vec![].into(),
            room_state: vec![].into(),
            member_profiles: Vec::new(),
            proposals: Vec::new(),
        };

        let decoded = DecodedDsGroupState::from(EncryptableDsGroupState::V4(v4));

        assert!(decoded.legacy_past_member_profiles.is_empty());
    }

    #[test]
    fn test_past_member_profiles_tls_stability() {
        use tls_codec::Serialize as _;

        let profiles = PastMemberProfiles {
            created_at: TimeStamp::from(0i64),
            profiles: vec![(LeafNodeIndex::new(0), EncryptedUserProfileKey::dummy())],
            room_state: vec![1u8, 2, 3].into(),
        };
        let serialized = profiles.tls_serialize_detached().unwrap();
        insta::assert_binary_snapshot!("past_member_profiles.tls", serialized);
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
        let bytes = PersistenceCodec::to_vec(&*DELETED_QUEUES).unwrap();
        let diag = cbor_diag::parse_bytes(&bytes[1..]).unwrap().to_hex();
        insta::assert_snapshot!(diag);
    }

    #[test]
    fn test_deleted_queues_serde_json() {
        insta::assert_json_snapshot!(&*DELETED_QUEUES);
    }

    /// Build a user leaf credential. The signature is empty, which is fine here since the helper
    /// only inspects the credential variant.
    fn user_leaf() -> LeafCredential {
        use aircommon::{
            credentials::{
                UserCredentialCsr, UserCredentialPayload, VerifiableUserCredential,
                keys::AsIntermediateSignature,
            },
            crypto::hash::Hash,
            identifiers::UserId,
        };
        use mls_assist::openmls::prelude::SignatureScheme;

        let user_id = UserId::new(Uuid::new_v4(), "example.com".parse().unwrap());
        let (csr, _prelim_key) = UserCredentialCsr::new(user_id, SignatureScheme::ED25519).unwrap();
        let payload = UserCredentialPayload::new(csr, None, Hash::from_bytes([0u8; 32]));
        LeafCredential::User(VerifiableUserCredential::new(
            payload,
            AsIntermediateSignature::empty(),
        ))
    }

    fn self_group_leaf() -> LeafCredential {
        use aircommon::credentials::SelfGroupCredential;
        LeafCredential::SelfGroup(SelfGroupCredential::new(Uuid::new_v4()))
    }

    #[test]
    fn leaf_credential_flag_consistency_matrix() {
        // A regular-group leaf must carry a user credential.
        assert!(leaf_credential_matches_flag(&user_leaf(), false));
        // A self-group leaf must carry a self-group credential.
        assert!(leaf_credential_matches_flag(&self_group_leaf(), true));
        // A user credential in a self-group is rejected.
        assert!(!leaf_credential_matches_flag(&user_leaf(), true));
        // A self-group credential in a regular group is rejected.
        assert!(!leaf_credential_matches_flag(&self_group_leaf(), false));
    }
}
