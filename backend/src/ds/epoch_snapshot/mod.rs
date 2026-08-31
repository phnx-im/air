// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! What a sibling emulator client needs to join a group at a past epoch.
//!
//! One row per (group, epoch), each holding a padded-AEAD blob under the
//! group's [`GroupStateEarKey`], like [`super::welcome_info`]. The DS writes a
//! row when a virtual client creates a group (epoch 0) or joins one by external
//! commit (the pre-commit epoch).

use aircommon::{
    crypto::{
        aead::{Ciphertext, PaddedAeadDecryptable, PaddedAeadEncryptable, keys::GroupStateEarKey},
        errors::{DecryptionError, EncryptionError},
    },
    time::TimeStamp,
};
use airmacros::{DeserializeTaggedMap, SerializeTaggedMap};
use mimi_room_policy::{RoomState, VerifiedRoomState};
use mls_assist::{
    messages::SerializedMlsMessage,
    openmls::{
        prelude::{GroupEpoch, group_info::GroupInfo},
        treesync::RatchetTree,
    },
};
use sqlx::PgConnection;
use thiserror::Error;
use tracing::error;
use uuid::Uuid;

use crate::errors::StorageError;

pub(super) mod persistence;

#[derive(Debug)]
pub struct EncryptedEpochSnapshotCtype;
pub type EncryptedEpochSnapshot = Ciphertext<EncryptedEpochSnapshotCtype>;

/// Marks the AAD of an epoch snapshot. See [`EpochSnapshotAad`].
const EPOCH_SNAPSHOT_RECORD: u8 = 1;

/// The state the DS served at one epoch of one group.
#[derive(Debug, Default, PartialEq, SerializeTaggedMap, DeserializeTaggedMap)]
pub(super) struct DsEpochSnapshot {
    #[tag(1)]
    pub(super) group_info: Option<GroupInfo>,
    #[tag(2)]
    pub(super) ratchet_tree: Option<RatchetTree>,
    #[tag(3)]
    pub(super) room_state: Option<RoomState>,
    #[tag(4)]
    pub(super) pq_group_info: Option<GroupInfo>,
    #[tag(5)]
    pub(super) pq_ratchet_tree: Option<RatchetTree>,
    /// The external commit accepted at this epoch, present iff the snapshot was
    /// written at an external join.
    #[tag(6)]
    pub(super) join_commit: Option<Vec<u8>>,
}

/// Binds a record to the group and epoch it was stored under
#[derive(Debug, SerializeTaggedMap)]
struct EpochSnapshotAad {
    #[tag(1)]
    record: u8,
    #[tag(2)]
    group_id: Uuid,
    #[tag(3)]
    epoch: u64,
}

impl PaddedAeadEncryptable<GroupStateEarKey, EncryptedEpochSnapshotCtype> for DsEpochSnapshot {}
impl PaddedAeadDecryptable<GroupStateEarKey, EncryptedEpochSnapshotCtype> for DsEpochSnapshot {}

impl DsEpochSnapshot {
    /// A record of the T leg at one epoch.
    pub(super) fn new(
        group_info: GroupInfo,
        ratchet_tree: RatchetTree,
        room_state: &VerifiedRoomState,
    ) -> Self {
        Self {
            group_info: Some(group_info),
            ratchet_tree: Some(ratchet_tree),
            room_state: Some(room_state.unverified().clone()),
            pq_group_info: None,
            pq_ratchet_tree: None,
            join_commit: None,
        }
    }

    /// Add the PQ leg of an APQ group.
    pub(super) fn with_pq_leg(mut self, group_info: GroupInfo, ratchet_tree: RatchetTree) -> Self {
        self.pq_group_info = Some(group_info);
        self.pq_ratchet_tree = Some(ratchet_tree);
        self
    }

    /// Add the external commit the DS accepted at this epoch.
    pub(super) fn with_join_commit(mut self, commit: &SerializedMlsMessage) -> Self {
        self.join_commit = Some(commit.0.clone());
        self
    }

    pub(super) fn encrypt(
        &self,
        ear_key: &GroupStateEarKey,
        group_id: Uuid,
        epoch: GroupEpoch,
    ) -> Result<EncryptedEpochSnapshot, EncryptionError> {
        self.encrypt_padded_with_aad(ear_key, &EpochSnapshotAad::new(group_id, epoch))
    }

    pub(super) fn decrypt(
        ear_key: &GroupStateEarKey,
        ciphertext: &EncryptedEpochSnapshot,
        group_id: Uuid,
        epoch: GroupEpoch,
    ) -> Result<Self, DecryptionError> {
        Self::decrypt_padded_with_aad(ear_key, ciphertext, &EpochSnapshotAad::new(group_id, epoch))
    }

    /// Encrypt and store the snapshot under (`group_id`, `epoch`).
    pub(super) async fn encrypt_and_store(
        &self,
        connection: &mut PgConnection,
        group_id: Uuid,
        epoch: GroupEpoch,
        ear_key: &GroupStateEarKey,
    ) -> Result<(), EpochSnapshotWriteError> {
        let ciphertext = self.encrypt(ear_key, group_id, epoch)?;
        Self::store(connection, group_id, epoch, &ciphertext, TimeStamp::now()).await?;
        Ok(())
    }

    /// The stored state, or `None` if the record is incomplete.
    pub(super) fn into_parts(self) -> Option<EpochSnapshotParts> {
        let group_info = self.group_info.or_else(|| {
            error!("epoch snapshot record without a group info");
            None
        })?;
        let ratchet_tree = self.ratchet_tree.or_else(|| {
            error!("epoch snapshot record without a ratchet tree");
            None
        })?;
        let room_state = self.room_state.or_else(|| {
            error!("epoch snapshot record without a room state");
            None
        })?;
        let pq = match (self.pq_group_info, self.pq_ratchet_tree) {
            (Some(group_info), Some(ratchet_tree)) => Some((group_info, ratchet_tree)),
            (None, None) => None,
            _ => {
                error!("epoch snapshot record with half a PQ leg");
                return None;
            }
        };
        Some(EpochSnapshotParts {
            group_info,
            ratchet_tree,
            room_state,
            pq,
            join_commit: self.join_commit,
        })
    }
}

pub(super) struct EpochSnapshotParts {
    pub(super) group_info: GroupInfo,
    pub(super) ratchet_tree: RatchetTree,
    pub(super) room_state: RoomState,
    /// Present iff the snapshot is of an APQ group.
    pub(super) pq: Option<(GroupInfo, RatchetTree)>,
    pub(super) join_commit: Option<Vec<u8>>,
}

impl EpochSnapshotAad {
    fn new(group_id: Uuid, epoch: GroupEpoch) -> Self {
        Self {
            record: EPOCH_SNAPSHOT_RECORD,
            group_id,
            epoch: epoch.as_u64(),
        }
    }
}

/// The epoch snapshot one request produced, to be written when that group's
/// state is persisted.
#[derive(Debug, Default)]
pub(super) struct EpochSnapshotOutbox(Option<(GroupEpoch, DsEpochSnapshot)>);

impl EpochSnapshotOutbox {
    /// Stage a snapshot for persistence at the same time as an update to
    /// [`crate::ds::group_state::DsGroupState`].
    pub(super) fn stage(&mut self, epoch: GroupEpoch, snapshot: DsEpochSnapshot) {
        self.0 = Some((epoch, snapshot));
    }

    /// Encrypt and store the staged snapshot, if there is one.
    pub(super) async fn write(
        self,
        connection: &mut PgConnection,
        group_id: Uuid,
        ear_key: &GroupStateEarKey,
    ) -> Result<(), EpochSnapshotWriteError> {
        let Some((epoch, snapshot)) = self.0 else {
            return Ok(());
        };
        snapshot
            .encrypt_and_store(connection, group_id, epoch, ear_key)
            .await
    }
}

#[derive(Debug, Error)]
pub(super) enum EpochSnapshotWriteError {
    #[error("Error encrypting epoch snapshot: {0}")]
    Encryption(#[from] EncryptionError),
    #[error("Error storing epoch snapshot: {0}")]
    Storage(#[from] StorageError),
}

impl From<EpochSnapshotWriteError> for tonic::Status {
    fn from(error: EpochSnapshotWriteError) -> Self {
        error!(%error, "failed to write epoch snapshot");
        Self::internal("failed to write epoch snapshot")
    }
}

#[cfg(test)]
mod test {
    use aircommon::{codec::PersistenceCodec, crypto::aead::AeadCiphertext};

    use crate::ds::welcome_info::DsWelcomeInfo;

    use super::*;

    fn record() -> DsEpochSnapshot {
        DsEpochSnapshot {
            group_info: None,
            ratchet_tree: None,
            room_state: None,
            pq_group_info: None,
            pq_ratchet_tree: None,
            join_commit: Some(vec![1u8, 2, 3]),
        }
    }

    #[test]
    fn epoch_snapshot_record_serde_codec() {
        let bytes = PersistenceCodec::to_vec(&record()).unwrap();
        let diag = cbor_diag::parse_bytes(&bytes[1..]).unwrap().to_hex();
        insta::assert_snapshot!(diag);
    }

    #[test]
    fn epoch_snapshot_record_round_trip() {
        let ear_key = GroupStateEarKey::random().unwrap();
        let group_id = Uuid::new_v4();
        let epoch = GroupEpoch::from(7);

        let ciphertext = record().encrypt(&ear_key, group_id, epoch).unwrap();
        let decrypted = DsEpochSnapshot::decrypt(&ear_key, &ciphertext, group_id, epoch).unwrap();

        assert_eq!(decrypted, record());
    }

    /// The AAD binds a record to its group and epoch, so a row moved to another
    /// epoch (or another group) must not decrypt.
    #[test]
    fn epoch_snapshot_record_aad_is_bound() {
        let ear_key = GroupStateEarKey::random().unwrap();
        let group_id = Uuid::new_v4();
        let epoch = GroupEpoch::from(7);
        let ciphertext = record().encrypt(&ear_key, group_id, epoch).unwrap();

        assert!(
            DsEpochSnapshot::decrypt(&ear_key, &ciphertext, group_id, GroupEpoch::from(8)).is_err()
        );
        assert!(DsEpochSnapshot::decrypt(&ear_key, &ciphertext, Uuid::new_v4(), epoch).is_err());
    }

    /// Welcome info is encrypted under the same key for the same (group,
    /// epoch), so only the record marker in the AAD keeps the two apart.
    #[test]
    fn welcome_info_does_not_decrypt_as_a_snapshot() {
        let ear_key = GroupStateEarKey::random().unwrap();
        let group_id = Uuid::new_v4();
        let epoch = GroupEpoch::from(7);

        let welcome_info = DsWelcomeInfo::default()
            .encrypt(&ear_key, group_id, epoch)
            .unwrap();
        let reinterpreted: EncryptedEpochSnapshot = AeadCiphertext::from(welcome_info).into();

        assert!(DsEpochSnapshot::decrypt(&ear_key, &reinterpreted, group_id, epoch).is_err());
    }

    /// Unknown tags are ignored, so a record written by a later version that
    /// added a field still reads.
    #[test]
    fn epoch_snapshot_record_ignores_unknown_tags() {
        #[derive(SerializeTaggedMap)]
        struct FutureRecord {
            #[tag(6)]
            join_commit: Option<Vec<u8>>,
            #[tag(9)]
            added_later: u64,
        }

        let future = FutureRecord {
            join_commit: Some(vec![1u8, 2, 3]),
            added_later: 42,
        };
        let bytes = PersistenceCodec::to_vec(&future).unwrap();
        let decoded: DsEpochSnapshot = PersistenceCodec::from_slice(&bytes).unwrap();

        assert_eq!(decoded.join_commit, future.join_commit);
        assert_eq!(decoded.group_info, None);
    }
}
