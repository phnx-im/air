// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Wire format for data synchronized across a user's own clients through the
//! self-group.
//!
//! [`SelfGroupMessage`] travels on self-group commits as `AppEphemeral`
//! proposals with component id `AIR_COMPONENT_ID`. The proposal data decodes
//! to an [`AppEphemeralPayload`], whose [`EncryptedSelfGroupMessages`] variant
//! carries a padded-AEAD-encrypted [`SelfGroupMessages`] payload. Updates that
//! need the commit order go here.
//!
//! [`SelfGroupAppMessage`] travels on plain MLS application messages under a
//! MIMI content extension. Updates that commute go here and save the commit.
//!
//! Every enum in this module is a tagged union with an `#[unknown]` catch-all,
//! so a client can adopt new tags before all of a user's devices understand
//! them.

use aircommon::crypto::{
    aead::{Ciphertext, PaddedAeadDecryptable, PaddedAeadEncryptable, keys::SelfGroupMessageKey},
    errors::RandomnessError,
    secrets::Secret,
};
use airmacros::{
    DeserializeTaggedMap, DeserializeTaggedUnion, SerializeTaggedMap, SerializeTaggedUnion,
};
use mimi_content::{Disposition, MimiContent, NestedPart, content_container::ExtensionName};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use openmls::group::GroupId;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tracing::warn;
use uuid::Uuid;

use super::group_bootstrap::{PeerUserId, group_id_as_bytes};
use crate::auth_service::v1::OperationType;

/// Marker for the ciphertext of [`SelfGroupMessages`].
#[derive(Debug)]
pub struct SelfGroupMessagesCtype;

/// Padded-AEAD ciphertext of a [`SelfGroupMessages`] payload.
pub type EncryptedSelfGroupMessages = Ciphertext<SelfGroupMessagesCtype>;

/// Payload of an `AppEphemeralProposal` with component id `AIR_COMPONENT_ID`.
///
/// ## CDDL Definition
///
/// ```cddl
/// AppEphemeralPayload = {
///   1: EncryptedSelfGroupMessages    ; tagged union, exactly one entry
/// }
/// ```
#[derive(Debug, Clone, PartialEq, SerializeTaggedUnion, DeserializeTaggedUnion)]
pub enum AppEphemeralPayload {
    #[tag(1)]
    EncryptedSelfGroupMessages(EncryptedSelfGroupMessages),
    /// A payload type this client does not understand; ignored on receive.
    #[unknown]
    Unknown,
}

/// Plaintext of an [`EncryptedSelfGroupMessages`].
///
/// Padded-AEAD-encrypted under the per-epoch [`SelfGroupMessageKey`].
///
/// ## CDDL Definition
///
/// ```cddl
/// SelfGroupMessages = [* SelfGroupMessage]
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SelfGroupMessages(pub Vec<SelfGroupMessage>);

impl PaddedAeadEncryptable<SelfGroupMessageKey, SelfGroupMessagesCtype> for SelfGroupMessages {}
impl PaddedAeadDecryptable<SelfGroupMessageKey, SelfGroupMessagesCtype> for SelfGroupMessages {}

/// A single message carried inside a [`SelfGroupMessages`] payload.
///
/// ## CDDL Definition
///
/// ```cddl
/// SelfGroupMessage = {
///   1: SettingsUpdate                ; tagged union; unknown tags are skipped
///   2: TokenSeed
///   3: BlockedContactsUpdate
///   4: DeletedChat
/// }
/// ```
#[derive(Debug, Clone, PartialEq, SerializeTaggedUnion, DeserializeTaggedUnion)]
pub enum SelfGroupMessage {
    #[tag(1)]
    SettingsUpdate(SettingsUpdate),
    #[tag(2)]
    TokenSeed(TokenSeed),
    #[tag(3)]
    BlockedContactsUpdate(BlockedContactsUpdate),
    #[tag(4)]
    DeletedChat(DeletedChat),
    /// A message kind this client does not understand; skipped on receive.
    #[unknown]
    Unknown,
}

/// Key of the MIMI content extension that carries a [`SelfGroupAppMessage`].
///
/// draft-ietf-mimi-content reserves negative keys for private use.
pub const SELF_GROUP_APP_MESSAGE_EXTENSION: i64 = -1;

/// A message carried as a plain MLS application message in the self group.
///
/// Sent as a body-less [`MimiContent`] with the message as the value of the
/// [`SELF_GROUP_APP_MESSAGE_EXTENSION`] extension. draft-ietf-mimi-content
/// section 6.3 gives an extension value three nested levels. The union map
/// and the payload map take two, so a payload field is a scalar, a byte
/// string, or a flat array or map of scalars.
///
/// ## CDDL Definition
///
/// ```cddl
/// SelfGroupAppMessage = {
///   1: RedeemedTokens    ; tagged union, exactly one entry
/// }
/// ```
#[derive(Debug, Clone, PartialEq, SerializeTaggedUnion, DeserializeTaggedUnion)]
pub enum SelfGroupAppMessage {
    #[tag(1)]
    RedeemedTokens(RedeemedTokens),
    /// A message kind this client does not understand, skipped on receive.
    #[unknown]
    Unknown,
}

impl SelfGroupAppMessage {
    /// Wraps the message into the MIMI content that carries it.
    pub fn to_mimi_content(&self) -> Result<MimiContent, SelfGroupAppMessageError> {
        let content = MimiContent {
            salt: Secret::<16>::random()?.secret().to_vec(),
            // Explicit, since older siblings render a body-less message as nothing.
            nested_part: NestedPart::NullPart {
                disposition: Disposition::Unspecified,
                language: Default::default(),
            },
            ..Default::default()
        };
        Ok(content.with_extension(
            ExtensionName::Number(SELF_GROUP_APP_MESSAGE_EXTENSION),
            self,
        )?)
    }

    /// Extracts the message from a [`MimiContent`], or `None` if the content
    /// is not a self-group application message.
    pub fn from_mimi_content(content: &MimiContent) -> Option<Self> {
        if !matches!(content.nested_part, NestedPart::NullPart { .. }) {
            return None;
        }
        content
            .extension(&ExtensionName::Number(SELF_GROUP_APP_MESSAGE_EXTENSION))
            .unwrap_or_else(|error| {
                warn!(%error, "undecodable self group application message");
                Some(Self::Unknown)
            })
    }
}

/// Error converting a [`SelfGroupAppMessage`] to or from its MIMI content.
#[derive(Debug, thiserror::Error)]
pub enum SelfGroupAppMessageError {
    #[error(transparent)]
    Salt(#[from] RandomnessError),
    #[error(transparent)]
    Extension(#[from] mimi_content::Error),
}

/// The Privacy Pass token seed of one (operation type, VOPRF key).
///
/// All of a user's devices derive their token requests from the same seed, which
/// is what lets the AS answer a repeat of a request for free and gives every
/// device the same tokens. The seed is set-once per key: the AS locks an
/// allowance epoch to the first request hash it sees, so a device deriving from
/// another seed gets a conflict instead of tokens.
///
/// Not a [`SettingsUpdate`] field. Settings are user-editable values with
/// last-writer-wins semantics, and any settings snapshot would cover an
/// in-flight seed proposal. A seed is set-once with first-writer-wins.
///
/// ## CDDL Definition
///
/// ```cddl
/// TokenSeed = {
///   1: int,            ; operation_type, the proto enum value
///   2: bstr .size 32,  ; key_fingerprint, SHA-256 of the serialized public key
///   3: bstr .size 32,  ; seed
/// }
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, SerializeTaggedMap, DeserializeTaggedMap)]
pub struct TokenSeed {
    #[tag(1)]
    pub operation_type: OperationType,
    #[tag(2)]
    pub key_fingerprint: [u8; 32],
    #[tag(3)]
    pub seed: [u8; 32],
}

/// The positions of the Privacy Pass tokens the sender redeemed at the AS.
///
/// Travels as a [`SelfGroupAppMessage`] since the redeemed set only grows and
/// needs no commit order.
///
/// ## CDDL Definition
///
/// ```cddl
/// RedeemedTokens = {
///   1: int,            ; operation_type, the proto enum value
///   2: bstr .size 32,  ; key_fingerprint, SHA-256 of the serialized public key
///   3: uint,           ; allowance_epoch
///   4: [* uint],       ; token_indices, ascending, no duplicates
/// }
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, SerializeTaggedMap, DeserializeTaggedMap)]
pub struct RedeemedTokens {
    #[tag(1)]
    pub operation_type: OperationType,
    #[tag(2)]
    pub key_fingerprint: [u8; 32],
    #[tag(3)]
    pub allowance_epoch: u32,
    #[tag(4)]
    pub token_indices: Vec<u16>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
pub enum LinkedDevicePlatform {
    #[default]
    Unknown = 0,
    Android = 1,
    Ios = 2,
    Macos = 3,
    Windows = 4,
    Linux = 5,
}

impl LinkedDevicePlatform {
    pub fn label(&self) -> &str {
        match self {
            LinkedDevicePlatform::Unknown => "Unknown",
            LinkedDevicePlatform::Android => "Android",
            LinkedDevicePlatform::Ios => "iOS",
            LinkedDevicePlatform::Macos => "macOS",
            LinkedDevicePlatform::Windows => "Windows",
            LinkedDevicePlatform::Linux => "Linux",
        }
    }
}

impl Serialize for LinkedDevicePlatform {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u8((*self).into())
    }
}

impl<'de> Deserialize<'de> for LinkedDevicePlatform {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = u8::deserialize(deserializer)?;
        LinkedDevicePlatform::try_from(v)
            .map_err(|_| serde::de::Error::custom(format!("invalid Status discriminant: {v}")))
    }
}

/// One of the user's devices, as advertised to its siblings.
///
/// The `client_id` matches the [`SelfGroupCredential`] of the device's self-group
/// leaf, which is what ties an entry to a self-group member. `linked_at` comes
/// from the publishing device's own clock and is a display hint only.
///
/// [`SelfGroupCredential`]: aircommon::credentials::SelfGroupCredential
///
/// ## CDDL Definition
///
/// ```cddl
/// LinkedDevice = {
///   1: bstr .size 16,   ; client_id
///   2: tstr,            ; name
///   3: uint,            ; linked_at, unix epoch seconds (UTC)
///   4: uint,            ; platform
/// }
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, SerializeTaggedMap, DeserializeTaggedMap)]
pub struct LinkedDevice {
    #[tag(1)]
    pub client_id: Uuid,
    #[tag(2)]
    pub name: String,
    #[tag(3)]
    pub linked_at: u64,
    #[tag(4)]
    pub platform: LinkedDevicePlatform,
}

/// The full state of the sender's synchronized user settings.
///
/// A settings update is a snapshot, not a diff. Senders fill in every synced
/// setting they have a stored value for. An absent field means the sender has
/// no value for that setting, for example because it is an older client that
/// does not know the tag. Receivers leave the local value of absent fields
/// unchanged.
///
/// The format carries no intent: it cannot express which fields the sender
/// meant to change, only which values it holds. A commit that changes one
/// setting therefore also covers a sibling device's in-flight change to an
/// unrelated setting, and cancels it. DS commit order decides which one wins.
/// Fixing that would take a per-field intent tag, worth adding if the loss of
/// concurrent changes becomes a problem as more synced settings arrive.
///
/// ## CDDL Definition
///
/// ```cddl
/// SettingsUpdate = {
///   ? send_read_receipts: bool .tag 1
///   ? linked_devices: [* LinkedDevice] .tag 2
/// }
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, SerializeTaggedMap, DeserializeTaggedMap)]
pub struct SettingsUpdate {
    #[tag(1)]
    pub send_read_receipts: Option<bool>,
    /// Sorted by `client_id` so the encoding is canonical.
    #[tag(2)]
    pub linked_devices: Option<Vec<LinkedDevice>>,
}

/// The contacts whose blocked state the sender just changed. This is a diff and
/// not a snapshot.
///
/// ## CDDL Definition
///
/// ```cddl
/// BlockedContactsUpdate = {
///   ? contacts: [* BlockedContactEntry] .tag 1,
/// }
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, SerializeTaggedMap, DeserializeTaggedMap)]
pub struct BlockedContactsUpdate {
    #[tag(1)]
    pub contacts: Vec<BlockedContactEntry>,
}

/// The new blocked state of one contact.
///
/// ## CDDL Definition
///
/// ```cddl
/// BlockedContactEntry = {
///   1: ContactBlocked //
///   2: ContactUnblocked
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, SerializeTaggedUnion, DeserializeTaggedUnion)]
pub enum BlockedContactEntry {
    #[tag(1)]
    Blocked(ContactBlocked),
    #[tag(2)]
    Unblocked(ContactUnblocked),
    /// A state this client does not understand. The entry is ignored on
    /// receive.
    #[unknown]
    Unknown,
}
impl BlockedContactEntry {
    pub fn user_id(&self) -> Option<&PeerUserId> {
        match self {
            BlockedContactEntry::Blocked(ContactBlocked { user_id, .. }) => Some(user_id),
            BlockedContactEntry::Unblocked(ContactUnblocked { user_id }) => Some(user_id),
            BlockedContactEntry::Unknown => None,
        }
    }
}

/// The contact is blocked.
///
/// `blocked_at` comes from the blocking device's own clock. A receiver stores it
/// as-is, so the block shows the same time on every device.
///
/// ## CDDL Definition
///
/// ```cddl
/// ContactBlocked = {
///   user_id: PeerUserId .tag 1,
///   blocked_at: uint .tag 2,      ; unix epoch seconds (UTC)
///   last_display_name: tstr .tag 3,
/// }
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, SerializeTaggedMap, DeserializeTaggedMap)]
pub struct ContactBlocked {
    #[tag(1)]
    pub user_id: PeerUserId,
    #[tag(2)]
    pub blocked_at: u64,
    /// The display name the blocking device last saw. Labels the contact in the
    /// blocked list without keeping the rest of its profile.
    #[tag(3)]
    pub last_display_name: String,
}

/// The contact is not blocked.
///
/// ## CDDL Definition
///
/// ```cddl
/// ContactUnblocked = {
///   user_id: PeerUserId .tag 1,
/// }
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, SerializeTaggedMap, DeserializeTaggedMap)]
pub struct ContactUnblocked {
    #[tag(1)]
    pub user_id: PeerUserId,
}

/// A chat the sender deleted locally. Receivers erase their copy of it.
///
/// ## CDDL Definition
///
/// ```cddl
/// DeletedChat = {
///   group_id: bstr .tag 1,
/// }
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, SerializeTaggedMap, DeserializeTaggedMap)]
pub struct DeletedChat {
    /// Group id of the chat's group, the T leg for APQ groups.
    #[tag(1, with = "group_id_as_bytes")]
    pub group_id: Option<GroupId>,
}

#[cfg(test)]
mod test {
    use std::collections::BTreeMap;

    use aircommon::{
        codec::PersistenceCodec,
        crypto::{
            aead::{AeadCiphertext, keys::SelfGroupMessageKey},
            kdf::{KdfDerivable, keys::SelfGroupExporterSecret},
        },
    };
    use mimi_content::{Disposition, cbor::Value};

    use super::*;

    /// AES-GCM authentication tag length, in bytes.
    const AES_GCM_TAG_LEN: usize = 16;

    fn message_key_from(secret_bytes: [u8; 32]) -> SelfGroupMessageKey {
        let exporter = SelfGroupExporterSecret::from_bytes(secret_bytes);
        SelfGroupMessageKey::derive(&exporter, &Vec::new()).unwrap()
    }

    /// Length of the AEAD ciphertext (including the GCM tag) behind an
    /// [`EncryptedSelfGroupMessages`].
    fn ciphertext_len(ciphertext: &EncryptedSelfGroupMessages) -> usize {
        let (bytes, _nonce) = AeadCiphertext::from(ciphertext.clone()).into_parts();
        bytes.len()
    }

    fn sample_messages() -> SelfGroupMessages {
        SelfGroupMessages(vec![SelfGroupMessage::SettingsUpdate(SettingsUpdate {
            send_read_receipts: Some(true),
            linked_devices: None,
        })])
    }

    fn sample_device(n: u128, name: &str, platform: LinkedDevicePlatform) -> LinkedDevice {
        LinkedDevice {
            client_id: Uuid::from_u128(n),
            name: name.to_owned(),
            linked_at: n as u64,
            platform,
        }
    }

    // 0. `LinkedDevice` wire shape and `SettingsUpdate` forward compatibility.

    #[test]
    fn linked_device_roundtrip_and_wire_shape() {
        let device = LinkedDevice {
            client_id: Uuid::from_u128(0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10),
            name: "iPhone".to_owned(),
            linked_at: 1_767_225_600,
            platform: LinkedDevicePlatform::Ios,
        };
        let bytes = PersistenceCodec::to_vec(&device).unwrap();
        let decoded: LinkedDevice = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(device, decoded);

        // The first byte is the persistence codec version, then a 4-entry map.
        assert_eq!(bytes[1], 0xA4);
    }

    #[test]
    fn linked_device_stability() {
        let device = LinkedDevice {
            client_id: Uuid::from_u128(0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10),
            name: "iPhone".to_owned(),
            linked_at: 1_767_225_600,
            platform: LinkedDevicePlatform::Ios,
        };
        let bytes = PersistenceCodec::to_vec(&device).unwrap();
        let diag = cbor_diag::parse_bytes(&bytes[1..]).unwrap().to_hex();
        insta::assert_snapshot!(diag);
    }

    #[test]
    fn settings_update_with_linked_devices_roundtrip() {
        let update = SettingsUpdate {
            send_read_receipts: Some(true),
            linked_devices: Some(vec![sample_device(
                1,
                "Laptop",
                LinkedDevicePlatform::Linux,
            )]),
        };
        let bytes = PersistenceCodec::to_vec(&update).unwrap();
        let decoded: SettingsUpdate = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(update, decoded);
    }

    /// An older client sends `SettingsUpdate` without tag 2. The field must
    /// decode as absent, which means "the sender has no value", not "clear the
    /// value".
    #[test]
    fn settings_update_without_linked_devices_decodes_as_absent() {
        #[derive(Debug, Clone, Default, SerializeTaggedMap)]
        struct SettingsUpdateV1 {
            #[tag(1)]
            send_read_receipts: Option<bool>,
        }

        let old = SettingsUpdateV1 {
            send_read_receipts: Some(false),
        };
        let bytes = PersistenceCodec::to_vec(&old).unwrap();
        let decoded: SettingsUpdate = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(decoded.send_read_receipts, Some(false));
        assert_eq!(decoded.linked_devices, None);
    }

    #[test]
    fn settings_update_with_unknown_tag_is_skipped() {
        #[derive(Debug, Clone, Default, SerializeTaggedMap)]
        struct SettingsUpdateV3 {
            #[tag(1)]
            send_read_receipts: Option<bool>,
            #[tag(3)]
            something_new: Option<u64>,
        }

        let newer = SettingsUpdateV3 {
            send_read_receipts: Some(true),
            something_new: Some(7),
        };
        let bytes = PersistenceCodec::to_vec(&newer).unwrap();
        let decoded: SettingsUpdate = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(decoded.send_read_receipts, Some(true));
        assert_eq!(decoded.linked_devices, None);
    }

    // 1. `SettingsUpdate` encode/decode and wire shape.

    #[test]
    fn settings_update_roundtrip_and_wire_shape() {
        let set = SettingsUpdate {
            send_read_receipts: Some(true),
            linked_devices: None,
        };
        let bytes = PersistenceCodec::to_vec(&set).unwrap();
        let decoded: SettingsUpdate = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(set, decoded);
        // `{1: true}`: map(1), key 1, true.
        assert_eq!(&bytes[1..], &[0xA1, 0x01, 0xF5]);

        let empty = SettingsUpdate {
            send_read_receipts: None,
            linked_devices: None,
        };
        let bytes = PersistenceCodec::to_vec(&empty).unwrap();
        let decoded: SettingsUpdate = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(empty, decoded);
        // `{}`: map(0).
        assert_eq!(&bytes[1..], &[0xA0]);
    }

    #[test]
    fn settings_update_stability() {
        let update = SettingsUpdate {
            send_read_receipts: Some(true),
            linked_devices: Some(vec![sample_device(
                1,
                "Laptop",
                LinkedDevicePlatform::Linux,
            )]),
        };
        let bytes = PersistenceCodec::to_vec(&update).unwrap();
        let diag = cbor_diag::parse_bytes(&bytes[1..]).unwrap().to_hex();
        insta::assert_snapshot!(diag);
    }

    // 1b. `TokenSeed` encode/decode and wire shape.

    fn sample_seed() -> TokenSeed {
        TokenSeed {
            operation_type: OperationType::AddUsername,
            key_fingerprint: [0xab; 32],
            seed: [0xcd; 32],
        }
    }

    #[test]
    fn token_seed_roundtrip_and_wire_shape() {
        let seed = sample_seed();
        let bytes = PersistenceCodec::to_vec(&seed).unwrap();
        let decoded: TokenSeed = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(seed, decoded);

        // The first byte is the persistence codec version, then a 3-entry map.
        assert_eq!(bytes[1], 0xA3);
    }

    #[test]
    fn token_seed_stability() {
        let bytes = PersistenceCodec::to_vec(&sample_seed()).unwrap();
        let diag = cbor_diag::parse_bytes(&bytes[1..]).unwrap().to_hex();
        insta::assert_snapshot!(diag);
    }

    /// The wire shape of a [`TokenSeed`] without the checks its field types
    /// make on decode.
    #[derive(Debug, Clone, SerializeTaggedMap)]
    struct LooseTokenSeed {
        #[tag(1)]
        operation_type: u32,
        #[tag(2)]
        key_fingerprint: Vec<u8>,
        #[tag(3)]
        seed: Vec<u8>,
    }

    /// The fixed-size fields are length-checked on decode, so a seed of the
    /// wrong length is a decode error rather than a silently truncated seed.
    #[test]
    fn token_seed_rejects_wrong_length() {
        let loose = LooseTokenSeed {
            operation_type: 1,
            key_fingerprint: vec![0xab; 32],
            seed: vec![0xcd; 31],
        };
        let bytes = PersistenceCodec::to_vec(&loose).unwrap();
        assert!(PersistenceCodec::from_slice::<TokenSeed>(&bytes).is_err());
    }

    /// A newer sibling may send an operation type this version does not know.
    /// It decodes rather than failing the batch it travels in, and the caller
    /// rejects `Unspecified`.
    #[test]
    fn token_seed_with_a_newer_operation_type_decodes_to_unspecified() {
        let newer = LooseTokenSeed {
            operation_type: 99,
            key_fingerprint: vec![0xab; 32],
            seed: vec![0xcd; 32],
        };
        let bytes = PersistenceCodec::to_vec(&newer).unwrap();
        let decoded: TokenSeed = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(decoded.operation_type, OperationType::Unspecified);
    }

    #[test]
    fn token_seed_travels_as_a_self_group_message() {
        let messages = SelfGroupMessages(vec![SelfGroupMessage::TokenSeed(sample_seed())]);
        let key = message_key_from([5u8; 32]);
        let encrypted = messages.encrypt_padded(&key).unwrap();
        let decrypted = SelfGroupMessages::decrypt_padded(&key, &encrypted).unwrap();
        assert_eq!(messages, decrypted);
    }

    /// An old client that predates tag 2 skips a seed message instead of
    /// failing, and still reads the settings update next to it.
    #[test]
    fn token_seed_is_skipped_by_a_settings_only_client() {
        #[derive(Debug, Clone, PartialEq, DeserializeTaggedUnion)]
        enum SelfGroupMessageV1 {
            #[tag(1)]
            SettingsUpdate(SettingsUpdate),
            #[unknown]
            Unknown,
        }

        let update = SettingsUpdate {
            send_read_receipts: Some(true),
            linked_devices: None,
        };
        let newer = vec![
            SelfGroupMessage::TokenSeed(sample_seed()),
            SelfGroupMessage::SettingsUpdate(update.clone()),
        ];
        let bytes = PersistenceCodec::to_vec(&newer).unwrap();

        let decoded: Vec<SelfGroupMessageV1> = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(
            decoded,
            vec![
                SelfGroupMessageV1::Unknown,
                SelfGroupMessageV1::SettingsUpdate(update),
            ]
        );
    }

    // 1c. `BlockedContactsUpdate` encode/decode and forward compatibility.

    fn sample_peer_user_id(n: u128) -> PeerUserId {
        PeerUserId {
            uuid: Some(Uuid::from_u128(n)),
            domain: Some("example.com".to_owned()),
        }
    }

    fn sample_blocked_contacts_update() -> BlockedContactsUpdate {
        BlockedContactsUpdate {
            contacts: vec![
                BlockedContactEntry::Blocked(ContactBlocked {
                    user_id: sample_peer_user_id(1),
                    blocked_at: 1_767_225_600,
                    last_display_name: "Alice".to_owned(),
                }),
                BlockedContactEntry::Unblocked(ContactUnblocked {
                    user_id: sample_peer_user_id(2),
                }),
            ],
        }
    }

    #[test]
    fn blocked_contacts_update_roundtrip() {
        let update = sample_blocked_contacts_update();
        let bytes = PersistenceCodec::to_vec(&update).unwrap();
        let decoded: BlockedContactsUpdate = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(update, decoded);
    }

    #[test]
    fn blocked_contacts_update_stability() {
        let bytes = PersistenceCodec::to_vec(&sample_blocked_contacts_update()).unwrap();
        let diag = cbor_diag::parse_bytes(&bytes[1..]).unwrap().to_hex();
        insta::assert_snapshot!(diag);
    }

    #[test]
    fn blocked_contacts_update_travels_as_a_self_group_message() {
        let messages = SelfGroupMessages(vec![SelfGroupMessage::BlockedContactsUpdate(
            sample_blocked_contacts_update(),
        )]);
        let key = message_key_from([11u8; 32]);
        let encrypted = messages.encrypt_padded(&key).unwrap();
        let decrypted = SelfGroupMessages::decrypt_padded(&key, &encrypted).unwrap();
        assert_eq!(messages, decrypted);
    }

    #[test]
    fn blocked_contacts_update_is_skipped_by_an_older_client() {
        #[derive(Debug, Clone, PartialEq, DeserializeTaggedUnion)]
        enum SelfGroupMessageNoBlocking {
            #[tag(1)]
            SettingsUpdate(SettingsUpdate),
            #[tag(2)]
            TokenSeed(TokenSeed),
            #[unknown]
            Unknown,
        }

        let update = SettingsUpdate {
            send_read_receipts: Some(true),
            linked_devices: None,
        };
        let newer = vec![
            SelfGroupMessage::BlockedContactsUpdate(sample_blocked_contacts_update()),
            SelfGroupMessage::SettingsUpdate(update.clone()),
        ];
        let bytes = PersistenceCodec::to_vec(&newer).unwrap();

        let decoded: Vec<SelfGroupMessageNoBlocking> =
            PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(
            decoded,
            vec![
                SelfGroupMessageNoBlocking::Unknown,
                SelfGroupMessageNoBlocking::SettingsUpdate(update),
            ]
        );
    }

    /// An entry state added after this client shipped decodes to `Unknown`, so
    /// the receiver drops the one entry instead of the whole update.
    #[test]
    fn blocked_contact_entry_with_unknown_state_decodes_to_unknown() {
        #[derive(Debug, Clone, PartialEq, SerializeTaggedUnion)]
        enum BlockedContactEntryV2 {
            #[tag(2)]
            Unblocked(ContactUnblocked),
            #[tag(99)]
            Muted(u64),
        }

        #[derive(Debug, Clone, SerializeTaggedMap)]
        struct BlockedContactsUpdateV2 {
            #[tag(1)]
            contacts: Vec<BlockedContactEntryV2>,
        }

        let unblocked = ContactUnblocked {
            user_id: sample_peer_user_id(1),
        };
        let newer = BlockedContactsUpdateV2 {
            contacts: vec![
                BlockedContactEntryV2::Muted(7),
                BlockedContactEntryV2::Unblocked(unblocked.clone()),
            ],
        };
        let bytes = PersistenceCodec::to_vec(&newer).unwrap();

        let decoded: BlockedContactsUpdate = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(
            decoded.contacts,
            vec![
                BlockedContactEntry::Unknown,
                BlockedContactEntry::Unblocked(unblocked),
            ]
        );
    }

    // 1d. `RedeemedTokens` encode/decode and wire shape.

    fn sample_redeemed() -> RedeemedTokens {
        RedeemedTokens {
            operation_type: OperationType::AddUsername,
            key_fingerprint: [0xab; 32],
            allowance_epoch: 679,
            token_indices: vec![0, 3, 7],
        }
    }

    #[test]
    fn redeemed_tokens_roundtrip_and_wire_shape() {
        let redeemed = sample_redeemed();
        let bytes = PersistenceCodec::to_vec(&redeemed).unwrap();
        let decoded: RedeemedTokens = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(redeemed, decoded);

        // The first byte is the persistence codec version, then a 4-entry map.
        assert_eq!(bytes[1], 0xA4);
    }

    #[test]
    fn redeemed_tokens_stability() {
        let bytes = PersistenceCodec::to_vec(&sample_redeemed()).unwrap();
        let diag = cbor_diag::parse_bytes(&bytes[1..]).unwrap().to_hex();
        insta::assert_snapshot!(diag);
    }

    // 2a. `SelfGroupMessage` forward compatibility: an unknown tag decodes to
    //    `Unknown`.

    /// A "newer" message enum with a variant unknown to [`SelfGroupMessage`].
    ///
    /// The tag is far out of range so that a later message kind does not claim
    /// it and turn this into a known variant.
    #[derive(Debug, Clone, PartialEq, SerializeTaggedUnion, DeserializeTaggedUnion)]
    enum SelfGroupMessageV2 {
        #[tag(1)]
        SettingsUpdate(SettingsUpdate),
        #[tag(99)]
        Something(u64),
        #[unknown]
        Unknown,
    }

    #[test]
    fn self_group_message_unknown_tag_decodes_to_unknown() {
        let newer = SelfGroupMessageV2::Something(42);
        let bytes = PersistenceCodec::to_vec(&newer).unwrap();
        let decoded: SelfGroupMessage = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(decoded, SelfGroupMessage::Unknown);
    }

    // 2b. `SelfGroupAppMessage` forward compatibility: an unknown tag decodes
    //     to `Unknown`.

    /// A "newer" message enum with a variant unknown to
    /// [`SelfGroupAppMessage`].
    #[derive(Debug, Clone, PartialEq, SerializeTaggedUnion, DeserializeTaggedUnion)]
    enum SelfGroupAppMessageV2 {
        #[tag(99)]
        Something(u64),
        #[unknown]
        Unknown,
    }

    #[test]
    fn self_group_app_message_unknown_tag_decodes_to_unknown() {
        let newer = SelfGroupAppMessageV2::Something(42);
        let bytes = PersistenceCodec::to_vec(&newer).unwrap();
        let decoded: SelfGroupAppMessage = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(decoded, SelfGroupAppMessage::Unknown);
    }

    // 2c. The MIMI content framing around a [`SelfGroupAppMessage`].

    fn content_with_extension(value: Value) -> MimiContent {
        MimiContent {
            extensions: BTreeMap::from([(
                ExtensionName::Number(SELF_GROUP_APP_MESSAGE_EXTENSION),
                value,
            )]),
            ..Default::default()
        }
    }

    /// A [`SelfGroupAppMessageV2`] variant this version does not know.
    fn a_newer_kind() -> Value {
        Value::from_serde(SelfGroupAppMessageV2::Something(42)).unwrap()
    }

    #[test]
    fn self_group_app_message_stability() {
        let mut content = SelfGroupAppMessage::RedeemedTokens(sample_redeemed())
            .to_mimi_content()
            .unwrap();
        content.salt = vec![0; 16];
        let diag = cbor_diag::parse_bytes(content.serialize().unwrap())
            .unwrap()
            .to_hex();
        insta::assert_snapshot!(diag);
    }

    #[test]
    fn a_message_roundtrips_through_its_mimi_content() {
        let message = SelfGroupAppMessage::RedeemedTokens(sample_redeemed());
        let content = message.to_mimi_content().unwrap();
        assert_eq!(
            SelfGroupAppMessage::from_mimi_content(&content),
            Some(message)
        );
    }

    #[test]
    fn a_message_survives_the_mimi_encoding() {
        let message = SelfGroupAppMessage::RedeemedTokens(sample_redeemed());
        let bytes = message.to_mimi_content().unwrap().serialize().unwrap();
        let decoded = MimiContent::deserialize(&bytes).unwrap();
        assert_eq!(
            SelfGroupAppMessage::from_mimi_content(&decoded),
            Some(message)
        );
    }

    /// Distinct salts keep equal payloads from sharing a Mimi ID.
    #[test]
    fn each_message_gets_a_fresh_salt() {
        let message = SelfGroupAppMessage::RedeemedTokens(sample_redeemed());
        assert_ne!(
            message.to_mimi_content().unwrap().salt,
            message.to_mimi_content().unwrap().salt
        );
    }

    /// `Some(Unknown)` rather than `None`, so it is not stored as a chat
    /// message.
    #[test]
    fn a_kind_from_a_newer_sibling_is_unknown() {
        assert_eq!(
            SelfGroupAppMessage::from_mimi_content(&content_with_extension(a_newer_kind())),
            Some(SelfGroupAppMessage::Unknown)
        );
    }

    #[test]
    fn the_envelope_survives_the_mimi_encoding() {
        let bytes = content_with_extension(a_newer_kind()).serialize().unwrap();
        let decoded = MimiContent::deserialize(&bytes).unwrap();
        assert_eq!(
            SelfGroupAppMessage::from_mimi_content(&decoded),
            Some(SelfGroupAppMessage::Unknown)
        );
    }

    /// A known tag with a payload of the wrong shape.
    #[test]
    fn an_undecodable_payload_is_unknown() {
        let value = Value::Map(BTreeMap::from([(
            Value::Int(1),
            Value::Text("garbage".into()),
        )]));
        assert_eq!(
            SelfGroupAppMessage::from_mimi_content(&content_with_extension(value)),
            Some(SelfGroupAppMessage::Unknown)
        );
    }

    #[test]
    fn a_value_that_is_not_a_map_is_unknown() {
        for value in [
            Value::Int(7),
            Value::Bytes(b"garbage".to_vec()),
            Value::Null,
        ] {
            assert_eq!(
                SelfGroupAppMessage::from_mimi_content(&content_with_extension(value)),
                Some(SelfGroupAppMessage::Unknown)
            );
        }
    }

    /// A body makes it a chat message, whatever its extensions.
    #[test]
    fn a_message_with_a_body_is_not_ours() {
        let mut content = SelfGroupAppMessage::RedeemedTokens(sample_redeemed())
            .to_mimi_content()
            .unwrap();
        content.nested_part = NestedPart::SinglePart {
            disposition: Disposition::Render,
            language: Default::default(),
            content_type: "text/markdown".to_owned(),
            content: b"hello".to_vec(),
        };
        assert_eq!(SelfGroupAppMessage::from_mimi_content(&content), None);
    }

    #[test]
    fn other_content_is_not_ours() {
        let note = MimiContent::simple_markdown_message("a note to self".to_owned(), [0; 16]);
        assert_eq!(SelfGroupAppMessage::from_mimi_content(&note), None);
    }

    // 3. `SelfGroupMessages` encrypt/decrypt roundtrip with exact padded length.

    #[test]
    fn self_group_messages_encrypt_decrypt_roundtrip() {
        let key = message_key_from([7u8; 32]);
        let messages = sample_messages();
        let encrypted = messages.encrypt_padded(&key).unwrap();
        let decrypted = SelfGroupMessages::decrypt_padded(&key, &encrypted).unwrap();
        assert_eq!(messages, decrypted);
        // The padded plaintext is exactly `PAD_FLOOR` (128); the ciphertext adds
        // the 16-byte GCM tag.
        assert_eq!(ciphertext_len(&encrypted), 128 + AES_GCM_TAG_LEN);
    }

    // 4. Same-secret derivation consistency, different-secret failure.

    #[test]
    fn same_secret_keys_are_interchangeable() {
        let key_a = message_key_from([9u8; 32]);
        let key_b = message_key_from([9u8; 32]);
        let key_other = message_key_from([1u8; 32]);

        let messages = sample_messages();
        let encrypted = messages.encrypt_padded(&key_a).unwrap();

        // A key derived from an equal exporter secret decrypts the ciphertext.
        let decrypted = SelfGroupMessages::decrypt_padded(&key_b, &encrypted).unwrap();
        assert_eq!(messages, decrypted);

        // A key derived from a different exporter secret does not.
        assert!(SelfGroupMessages::decrypt_padded(&key_other, &encrypted).is_err());
    }

    // 5. `AppEphemeralPayload` roundtrip and unknown-tag decode.

    /// A "newer" payload enum with a variant unknown to [`AppEphemeralPayload`].
    #[derive(Debug, Clone, PartialEq, SerializeTaggedUnion, DeserializeTaggedUnion)]
    enum AppEphemeralPayloadV2 {
        #[tag(1)]
        EncryptedSelfGroupMessages(EncryptedSelfGroupMessages),
        #[tag(2)]
        Other(u64),
        #[unknown]
        Unknown,
    }

    #[test]
    fn app_ephemeral_payload_roundtrip() {
        let key = message_key_from([3u8; 32]);
        let encrypted = sample_messages().encrypt_padded(&key).unwrap();
        let payload = AppEphemeralPayload::EncryptedSelfGroupMessages(encrypted);

        let bytes = PersistenceCodec::to_vec(&payload).unwrap();
        let decoded: AppEphemeralPayload = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(payload, decoded);
    }

    #[test]
    fn app_ephemeral_payload_unknown_tag_decodes_to_unknown() {
        let newer = AppEphemeralPayloadV2::Other(5);
        let bytes = PersistenceCodec::to_vec(&newer).unwrap();
        let decoded: AppEphemeralPayload = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(decoded, AppEphemeralPayload::Unknown);
    }

    // 6. A `Vec<SelfGroupMessage>` with an unknown element decodes with that
    //    element as `Unknown` and the known ones intact.

    #[test]
    fn vec_of_messages_with_unknown_element() {
        let known_a = SelfGroupMessageV2::SettingsUpdate(SettingsUpdate {
            send_read_receipts: Some(true),
            linked_devices: None,
        });
        let known_b = SelfGroupMessageV2::SettingsUpdate(SettingsUpdate {
            send_read_receipts: Some(false),
            linked_devices: None,
        });
        let newer = vec![known_a, SelfGroupMessageV2::Something(9), known_b];
        let bytes = PersistenceCodec::to_vec(&newer).unwrap();

        let decoded: SelfGroupMessages = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(
            decoded.0,
            vec![
                SelfGroupMessage::SettingsUpdate(SettingsUpdate {
                    send_read_receipts: Some(true),
                    linked_devices: None,
                }),
                SelfGroupMessage::Unknown,
                SelfGroupMessage::SettingsUpdate(SettingsUpdate {
                    send_read_receipts: Some(false),
                    linked_devices: None,
                }),
            ]
        );
    }
}
