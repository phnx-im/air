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
use serde::{Deserialize, Serialize};

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

/// An update to one or more synchronized user settings.
///
/// ## CDDL Definition
///
/// ```cddl
/// SettingsUpdate = {
///   ? send_read_receipts: bool .tag 1
/// }
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, SerializeTaggedMap, DeserializeTaggedMap)]
pub struct SettingsUpdate {
    #[tag(1)]
    pub send_read_receipts: Option<bool>,
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
        })])
    }

    // 1. `SettingsUpdate` encode/decode and wire shape.

    #[test]
    fn settings_update_roundtrip_and_wire_shape() {
        let set = SettingsUpdate {
            send_read_receipts: Some(true),
        };
        let bytes = PersistenceCodec::to_vec(&set).unwrap();
        let decoded: SettingsUpdate = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(set, decoded);
        // `{1: true}`: map(1), key 1, true.
        assert_eq!(&bytes[1..], &[0xA1, 0x01, 0xF5]);

        let empty = SettingsUpdate {
            send_read_receipts: None,
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
        });
        let known_b = SelfGroupMessageV2::SettingsUpdate(SettingsUpdate {
            send_read_receipts: Some(false),
        });
        let newer = vec![known_a, SelfGroupMessageV2::Something(9), known_b];
        let bytes = PersistenceCodec::to_vec(&newer).unwrap();

        let decoded: SelfGroupMessages = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(
            decoded.0,
            vec![
                SelfGroupMessage::SettingsUpdate(SettingsUpdate {
                    send_read_receipts: Some(true),
                }),
                SelfGroupMessage::Unknown,
                SelfGroupMessage::SettingsUpdate(SettingsUpdate {
                    send_read_receipts: Some(false),
                }),
            ]
        );
    }
}
