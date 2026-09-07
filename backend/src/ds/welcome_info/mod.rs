// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! What a joiner needs about the epoch its Welcome refers to.
//!
//! One row per (group, epoch), each holding a padded-AEAD blob under the
//! group's [`GroupStateEarKey`]. This used to live inside the group state --
//! the retained ratchet trees dominated it, so every request paid for data only
//! `welcome_info` ever reads.

use aircommon::{
    crypto::{
        aead::{
            Ciphertext, PaddedAeadDecryptable, PaddedAeadEncryptable,
            keys::{EncryptedUserProfileKey, GroupStateEarKey},
        },
        errors::{DecryptionError, EncryptionError},
    },
    time::TimeStamp,
};
use airmacros::{DeserializeTaggedMap, SerializeTaggedMap};
use mimi_room_policy::{RoomState, VerifiedRoomState};
use mls_assist::{
    group::{LegacyPastGroupState, RetainedWelcomeInfo},
    openmls::{
        prelude::{GroupEpoch, LeafNodeIndex, SignaturePublicKey},
        treesync::RatchetTree,
    },
};
use sqlx::PgConnection;
use thiserror::Error;
use tracing::error;
use uuid::Uuid;

use crate::errors::StorageError;

pub(super) mod persistence;

pub(super) type WelcomeInfoParts = (
    RatchetTree,
    Vec<(LeafNodeIndex, EncryptedUserProfileKey)>,
    VerifiedRoomState,
);

#[derive(Debug)]
pub struct EncryptedWelcomeInfoCtype;
pub type EncryptedWelcomeInfo = Ciphertext<EncryptedWelcomeInfoCtype>;

/// The welcome information for one epoch of one group.
#[derive(Debug, Default, PartialEq, SerializeTaggedMap, DeserializeTaggedMap)]
pub(super) struct DsWelcomeInfo {
    /// The ratchet tree as of this epoch.
    #[tag(1)]
    pub(super) ratchet_tree: Option<RatchetTree>,
    /// The only signature keys allowed to fetch this record.
    #[tag(2)]
    pub(super) potential_joiners: Vec<SignaturePublicKey>,
    /// The profile keys as of this epoch. Empty for the PQ leg of an APQ group,
    /// which only ever needs the tree.
    #[tag(3)]
    pub(super) profiles: Vec<(LeafNodeIndex, EncryptedUserProfileKey)>,
    #[tag(4)]
    pub(super) room_state: Option<RoomState>,
}

/// Binds a record to the group and epoch it was stored under. The EAR key is
/// the same for every epoch of a group, so without this a row could be moved
/// between epochs, or between the T and PQ legs, undetected.
#[derive(Debug, SerializeTaggedMap)]
struct WelcomeInfoAad {
    #[tag(1)]
    group_id: Uuid,
    #[tag(2)]
    epoch: u64,
}

impl PaddedAeadEncryptable<GroupStateEarKey, EncryptedWelcomeInfoCtype> for DsWelcomeInfo {}
impl PaddedAeadDecryptable<GroupStateEarKey, EncryptedWelcomeInfoCtype> for DsWelcomeInfo {}

impl DsWelcomeInfo {
    /// A record for the epoch `retained` refers to, with the group's current
    /// profile keys and room state.
    pub(super) fn new(
        retained: RetainedWelcomeInfo,
        profiles: Vec<(LeafNodeIndex, EncryptedUserProfileKey)>,
        room_state: &VerifiedRoomState,
    ) -> (GroupEpoch, Self) {
        let record = Self {
            ratchet_tree: Some(retained.ratchet_tree),
            potential_joiners: retained.potential_joiners,
            profiles,
            room_state: Some(room_state.unverified().clone()),
        };
        (retained.epoch, record)
    }

    /// A record built from a legacy in-group-state entry, carrying its original
    /// creation time. However, we expire the old entry with the new shorter retention window.
    pub(super) fn from_legacy(
        legacy: LegacyPastGroupState,
        profiles: Vec<(LeafNodeIndex, EncryptedUserProfileKey)>,
        room_state: Option<RoomState>,
    ) -> (GroupEpoch, TimeStamp, Self) {
        let record = Self {
            ratchet_tree: Some(legacy.ratchet_tree),
            potential_joiners: legacy.potential_joiners,
            profiles,
            room_state,
        };
        (legacy.epoch, legacy.creation_time.into(), record)
    }

    pub(super) fn encrypt(
        &self,
        ear_key: &GroupStateEarKey,
        group_id: Uuid,
        epoch: GroupEpoch,
    ) -> Result<EncryptedWelcomeInfo, EncryptionError> {
        self.encrypt_padded_with_aad(ear_key, &WelcomeInfoAad::new(group_id, epoch))
    }

    pub(super) fn decrypt(
        ear_key: &GroupStateEarKey,
        ciphertext: &EncryptedWelcomeInfo,
        group_id: Uuid,
        epoch: GroupEpoch,
    ) -> Result<Self, DecryptionError> {
        Self::decrypt_padded_with_aad(ear_key, ciphertext, &WelcomeInfoAad::new(group_id, epoch))
    }

    /// Returns true if `joiner` is allowed to fetch this record.
    pub(super) fn is_authorized(&self, joiner: &SignaturePublicKey) -> bool {
        self.potential_joiners.contains(joiner)
    }

    /// The ratchet tree and room state, or `None` if either is missing or the
    /// room state fails to verify.
    pub(super) fn into_parts(self) -> Option<WelcomeInfoParts> {
        let ratchet_tree = self.ratchet_tree.or_else(|| {
            error!("welcome info record without a ratchet tree");
            None
        })?;
        let room_state = self.room_state.or_else(|| {
            error!("welcome info record without a room state");
            None
        })?;
        let room_state = VerifiedRoomState::verify(room_state)
            .inspect_err(|error| error!(%error, "failed to verify stored room state"))
            .ok()?;
        Some((ratchet_tree, self.profiles, room_state))
    }
}

impl WelcomeInfoAad {
    fn new(group_id: Uuid, epoch: GroupEpoch) -> Self {
        Self {
            group_id,
            epoch: epoch.as_u64(),
        }
    }
}

/// The welcome information one request produced for one group, to be written
/// when that group's state is persisted.
#[derive(Debug, Default)]
pub(super) struct WelcomeInfoOutbox(Vec<(GroupEpoch, TimeStamp, DsWelcomeInfo)>);

impl WelcomeInfoOutbox {
    /// Stage a welcome info for persistence at the same time as an update to [`DsGroupState`].
    pub(super) fn stage(&mut self, epoch: GroupEpoch, created_at: TimeStamp, info: DsWelcomeInfo) {
        self.0.push((epoch, created_at, info));
    }

    /// Takes a single entry and discards the rest. This is used in the legacy path, where all entries
    /// were in a single blob.
    pub(super) fn take(self, epoch: GroupEpoch) -> Option<DsWelcomeInfo> {
        self.0
            .into_iter()
            .find_map(|(welcome_info_epoch, _, info)| {
                if epoch == welcome_info_epoch {
                    Some(info)
                } else {
                    None
                }
            })
    }

    /// Encrypt and store every staged welcome infos.
    pub(super) async fn write(
        self,
        connection: &mut PgConnection,
        group_id: Uuid,
        ear_key: &GroupStateEarKey,
    ) -> Result<(), WelcomeInfoWriteError> {
        for (epoch, created_at, info) in self.0 {
            let ciphertext = info.encrypt(ear_key, group_id, epoch)?;
            DsWelcomeInfo::store(&mut *connection, group_id, epoch, &ciphertext, created_at)
                .await?;
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub(super) enum WelcomeInfoWriteError {
    #[error("Error encrypting welcome info: {0}")]
    Encryption(#[from] EncryptionError),
    #[error("Error storing welcome info: {0}")]
    Storage(#[from] StorageError),
}

impl From<WelcomeInfoWriteError> for tonic::Status {
    fn from(error: WelcomeInfoWriteError) -> Self {
        error!(%error, "failed to write welcome info");
        Self::internal("failed to write welcome info")
    }
}

#[cfg(test)]
mod test {
    use aircommon::codec::PersistenceCodec;

    use super::*;

    fn record() -> DsWelcomeInfo {
        DsWelcomeInfo {
            ratchet_tree: None,
            potential_joiners: vec![SignaturePublicKey::from(vec![1u8, 2, 3])],
            profiles: vec![(LeafNodeIndex::new(1), EncryptedUserProfileKey::dummy())],
            room_state: None,
        }
    }

    #[test]
    fn welcome_info_record_serde_codec() {
        let bytes = PersistenceCodec::to_vec(&record()).unwrap();
        let diag = cbor_diag::parse_bytes(&bytes[1..]).unwrap().to_hex();
        insta::assert_snapshot!(diag);
    }

    #[test]
    fn welcome_info_record_round_trip() {
        let ear_key = GroupStateEarKey::random().unwrap();
        let group_id = Uuid::new_v4();
        let epoch = GroupEpoch::from(7);

        let ciphertext = record().encrypt(&ear_key, group_id, epoch).unwrap();
        let decrypted = DsWelcomeInfo::decrypt(&ear_key, &ciphertext, group_id, epoch).unwrap();

        assert_eq!(decrypted, record());
    }

    /// Only the joiners the commit added may fetch the record.
    #[test]
    fn welcome_info_record_authorization() {
        let record = record();

        assert!(record.is_authorized(&SignaturePublicKey::from(vec![1u8, 2, 3])));
        assert!(!record.is_authorized(&SignaturePublicKey::from(vec![4u8, 5, 6])));
        assert!(
            !DsWelcomeInfo::default().is_authorized(&SignaturePublicKey::from(vec![1u8, 2, 3]))
        );
    }

    /// The AAD binds a record to its group and epoch, so a row moved to another
    /// epoch (or another group) must not decrypt.
    #[test]
    fn welcome_info_record_aad_is_bound() {
        let ear_key = GroupStateEarKey::random().unwrap();
        let group_id = Uuid::new_v4();
        let epoch = GroupEpoch::from(7);
        let ciphertext = record().encrypt(&ear_key, group_id, epoch).unwrap();

        assert!(
            DsWelcomeInfo::decrypt(&ear_key, &ciphertext, group_id, GroupEpoch::from(8)).is_err()
        );
        assert!(DsWelcomeInfo::decrypt(&ear_key, &ciphertext, Uuid::new_v4(), epoch).is_err());
    }

    /// Unknown tags are ignored, so a record written by a later version that
    /// added a field still reads.
    #[test]
    fn welcome_info_record_ignores_unknown_tags() {
        #[derive(SerializeTaggedMap)]
        struct FutureRecord {
            #[tag(2)]
            potential_joiners: Vec<SignaturePublicKey>,
            #[tag(9)]
            added_later: u64,
        }

        let future = FutureRecord {
            potential_joiners: vec![SignaturePublicKey::from(vec![1u8, 2, 3])],
            added_later: 42,
        };
        let bytes = PersistenceCodec::to_vec(&future).unwrap();
        let decoded: DsWelcomeInfo = PersistenceCodec::from_slice(&bytes).unwrap();

        assert_eq!(decoded.potential_joiners, future.potential_joiners);
        assert_eq!(decoded.profiles, vec![]);
    }
}
