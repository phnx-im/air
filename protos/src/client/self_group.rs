// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Wire format for data synchronized across a user's own clients through the
//! self-group.
//!
//! Settings updates travel as `AppEphemeral` proposals with component id
//! `AIR_COMPONENT_ID` inside self-group commits. The proposal data decodes to
//! an [`AppEphemeralPayload`], whose [`EncryptedSelfGroupMessages`] variant
//! carries a padded-AEAD-encrypted [`SelfGroupMessages`] payload. Every enum in
//! this module is a tagged union with an `#[unknown]` catch-all, so a client can
//! adopt new tags before all of a user's devices understand them.

use aircommon::crypto::aead::{
    Ciphertext, PaddedAeadDecryptable, PaddedAeadEncryptable, keys::SelfGroupMessageKey,
};
use airmacros::{
    DeserializeTaggedMap, DeserializeTaggedUnion, SerializeTaggedMap, SerializeTaggedUnion,
};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

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
/// }
/// ```
#[derive(Debug, Clone, PartialEq, SerializeTaggedUnion, DeserializeTaggedUnion)]
pub enum SelfGroupMessage {
    #[tag(1)]
    SettingsUpdate(SettingsUpdate),
    /// A message kind this client does not understand; skipped on receive.
    #[unknown]
    Unknown,
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

#[cfg(test)]
mod test {
    use aircommon::{
        codec::PersistenceCodec,
        crypto::{
            aead::{AeadCiphertext, keys::SelfGroupMessageKey},
            kdf::{KdfDerivable, keys::SelfGroupExporterSecret},
        },
    };

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

    // 2. `SelfGroupMessage` forward compatibility: an unknown tag decodes to
    //    `Unknown`.

    /// A "newer" message enum with a variant unknown to [`SelfGroupMessage`].
    #[derive(Debug, Clone, PartialEq, SerializeTaggedUnion, DeserializeTaggedUnion)]
    enum SelfGroupMessageV2 {
        #[tag(1)]
        SettingsUpdate(SettingsUpdate),
        #[tag(2)]
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
