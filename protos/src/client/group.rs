// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Client protocol types related to groups.

use std::borrow::Cow;

use aircommon::{
    codec::{self, PersistenceCodec},
    crypto::{
        ear::{AEAD_NONCE_SIZE, AeadCiphertext, EarKey, Payload, keys::IdentityLinkWrapperKey},
        errors::{DecryptionError, EncryptionError},
    },
    padme::padme_padding_len,
};
use airmacros::{DeserializeTaggedMap, SerializeTaggedMap};
use mimi_content::content_container::{EncryptionAlgorithm, HashAlgorithm};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Data stored in the group data extension as blob.
///
/// Warning: This type is serialized and stored in the group context, and was introduced before we
/// had support for tagged maps. So it serialization format must be stable with default derives
/// from `serde`.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct GroupData {
    pub title: String,
    /// Encrypted group title
    ///
    /// It is encrypted with the same key and algorithm as the external group profile. It is
    /// included in this data to be able to use the group title immediately without having to fetch
    /// the external group profile.
    pub encrypted_title: Option<EncryptedGroupTitle>,
    /// A pointer to an external encrypted group profile
    ///
    /// Using this data, it is possible to retrieve the group profile from the object storage.
    pub external_group_profile: Option<ExternalGroupProfile>,
}

/// External encrypted group profile in the object storage.
///
/// This type is similar to the `ExternalPart` in the [MIMI Message Content draft].
///
/// [MIMI Message Content draft]: https://www.ietf.org/archive/id/draft-ietf-mimi-content-07.html
///
/// ## CDDL Definition
///
/// ```cddl
/// ExternalGroupProfile = {
///   object_id: bytes .size 16 .tag 1,
///   size: uint .size 8 .tag 2,
///   encAlg: uint .size 2 .tag 3,
///   nonce: bstr .tag 4,
///   aad: bstr .tag 5,
///   hashAlg: uint .size 1 .tag 6,
///   contentHash: bstr .tag 7,
/// }
/// ```
#[derive(Debug, Clone, Eq, PartialEq, SerializeTaggedMap, DeserializeTaggedMap)]
pub struct ExternalGroupProfile {
    /// Object ID in the object storage
    ///
    /// Via this ID, the chat attributes can be retrieved from the object storage.
    #[tag(1)]
    pub object_id: Uuid,
    /// Size of the content in bytes
    #[tag(2)]
    pub size: u64,
    /// An IANA AEAD Algorithm
    #[tag(3)]
    pub enc_alg: Option<EncryptionAlgorithm>,
    /// AEAD nonce
    #[tag(4)]
    pub nonce: [u8; AEAD_NONCE_SIZE],
    /// AEAD additional authentication data
    #[tag(5)]
    pub aad: Vec<u8>,
    /// An IANA Named Information Hash Algorithm
    #[tag(6)]
    pub hash_alg: HashAlgorithm,
    /// Hash of the original content (non-encrypted)
    #[tag(7)]
    pub content_hash: Vec<u8>,
}

/// A group title with padding
///
/// ## CDDL Definition
///
/// ```cddl
/// GroupTitle = {
///   title: tstr .tag 1,
///   padding: bytes .tag 2,
/// }
/// ```
#[derive(Debug, Clone, Default, Eq, PartialEq, SerializeTaggedMap, DeserializeTaggedMap)]
struct GroupTitle<'a> {
    #[tag(1)]
    title: Cow<'a, str>,
    #[tag(2)]
    padding: Vec<u8>,
}

/// Ciphertext of a group title
///
/// ## CDDL Definition
///
/// ```cddl
/// EncryptedGroupTitle = {
///   ciphertext: bytes .tag 1,
///   nonce: bytes .size 12 .tag 2,
///   aad: bytes .tag 3,
/// }
/// ```
#[derive(Debug, Clone, Default, Eq, PartialEq, SerializeTaggedMap, DeserializeTaggedMap)]
pub struct EncryptedGroupTitle {
    /// Ciphertext of a utf-8 encoded string
    #[tag(1)]
    pub ciphertext: Vec<u8>,
    #[tag(2)]
    pub nonce: [u8; AEAD_NONCE_SIZE],
    #[tag(3)]
    pub aad: Vec<u8>,
}

/// Group profile stored as encrypted blob in the object storage.
#[derive(Debug, Clone, Eq, PartialEq, Hash, SerializeTaggedMap, DeserializeTaggedMap)]
pub struct GroupProfile<'a> {
    #[tag(1)]
    pub title: String,
    #[tag(2)]
    pub description: Option<String>,
    #[tag(3)]
    pub picture: Option<Cow<'a, [u8]>>,
    #[tag(4)]
    padding: Vec<u8>,
}

/// A build helper for the [`GroupProfile`] type.
///
/// Requires to set the `object_id` field for already filled out [`ExternalGroupProfile`].
pub struct ExternalGroupProfileBuilder {
    inner: ExternalGroupProfile,
}

impl ExternalGroupProfileBuilder {
    #[inline]
    pub fn build(mut self, object_id: Uuid) -> ExternalGroupProfile {
        self.inner.object_id = object_id;
        self.inner
    }
}

const AIR_GROUP_PROFILE_ENCRYPTION_ALG: EncryptionAlgorithm = EncryptionAlgorithm::Aes256Gcm;
const AIR_GROUP_PROFILE_HASH_ALG: HashAlgorithm = HashAlgorithm::Sha256;

impl<'a> GroupProfile<'a> {
    pub fn new(title: String, description: Option<String>, picture: Option<Cow<'a, [u8]>>) -> Self {
        let len = title.len()
            + description.as_ref().map(|d| d.len()).unwrap_or(0)
            + picture.as_ref().map(|p| p.len()).unwrap_or(0);
        let padding = padme_padding_len(len);
        Self {
            title,
            description,
            picture,
            padding: vec![0; padding],
        }
    }

    pub fn encrypt(
        &self,
        identity_link_wrapper_key: &IdentityLinkWrapperKey,
    ) -> Result<(Vec<u8>, ExternalGroupProfileBuilder), GroupProfileEncryptionError> {
        let plaintext = PersistenceCodec::to_vec(self)?;

        let aad = b"group-profile";
        let payload = Payload {
            msg: plaintext.as_slice(),
            aad,
        };

        let aead_ciphertext = identity_link_wrapper_key.encrypt(payload)?;
        let (ciphertext, nonce) = aead_ciphertext.into_parts();
        let size = ciphertext
            .len()
            .try_into()
            .map_err(|_| GroupProfileEncryptionError::UsizeOverflow)?;

        let content_hash = Sha256::digest(&plaintext);

        let external = ExternalGroupProfile {
            object_id: Uuid::nil(),
            size,
            enc_alg: Some(AIR_GROUP_PROFILE_ENCRYPTION_ALG),
            nonce,
            aad: aad.to_vec(),
            hash_alg: AIR_GROUP_PROFILE_HASH_ALG,
            content_hash: content_hash.to_vec(),
        };
        let builder = ExternalGroupProfileBuilder { inner: external };

        Ok((ciphertext, builder))
    }

    /// Decypts the group profile from the given ciphertext.
    ///
    /// Also validates the size, encryption algorithm, hash algorithm and checksum.
    pub fn decrypt(
        identity_link_wrapper_key: &IdentityLinkWrapperKey,
        external_group_profile: &ExternalGroupProfile,
        ciphertext: Vec<u8>,
    ) -> Result<Self, GroupProfileDecryptionError> {
        let ciphertext_len: u64 = ciphertext
            .len()
            .try_into()
            .map_err(|_| GroupProfileDecryptionError::UsizeOverflow)?;
        if external_group_profile.size != ciphertext_len {
            return Err(GroupProfileDecryptionError::SizeMismatch);
        }
        if external_group_profile.enc_alg != Some(AIR_GROUP_PROFILE_ENCRYPTION_ALG) {
            return Err(GroupProfileDecryptionError::UnexpectedEncryptionAlgorithm(
                external_group_profile.enc_alg,
            ));
        }
        if external_group_profile.hash_alg != AIR_GROUP_PROFILE_HASH_ALG {
            return Err(GroupProfileDecryptionError::UnexpectedHashAlgorithm(
                external_group_profile.hash_alg,
            ));
        }

        let aead_ciphertext = AeadCiphertext::new(ciphertext, external_group_profile.nonce);
        let plaintext = identity_link_wrapper_key
            .decrypt_with_aad(&aead_ciphertext, &external_group_profile.aad)?;

        let sha256 = Sha256::digest(&plaintext);
        if sha256.as_slice() != external_group_profile.content_hash.as_slice() {
            return Err(GroupProfileDecryptionError::ChecksumMismatch);
        }

        Ok(PersistenceCodec::from_slice(&plaintext)?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GroupProfileEncryptionError {
    #[error(transparent)]
    Codec(#[from] codec::Error),
    #[error(transparent)]
    Encryption(#[from] EncryptionError),
    #[error("usize overflow")]
    UsizeOverflow,
}

#[derive(Debug, thiserror::Error)]
pub enum GroupProfileDecryptionError {
    #[error("usize overflow")]
    UsizeOverflow,
    #[error("size mismatch")]
    SizeMismatch,
    #[error("unexpected encryption algorithm: {0:?}")]
    UnexpectedEncryptionAlgorithm(Option<EncryptionAlgorithm>),
    #[error("unexpected hash algorithm: {0:?}")]
    UnexpectedHashAlgorithm(HashAlgorithm),
    #[error("checksum mismatch")]
    ChecksumMismatch,
    #[error(transparent)]
    Decryption(#[from] DecryptionError),
    #[error(transparent)]
    Codec(#[from] codec::Error),
}

impl<'a> GroupTitle<'a> {
    fn new(title: &'a str) -> Self {
        let size = title.len();
        let padding = padme_padding_len(size);
        Self {
            title: Cow::Borrowed(title),
            padding: vec![0; padding],
        }
    }
}

impl EncryptedGroupTitle {
    pub fn encrypt(
        plaintext: &str,
        identity_link_wrapper_key: &IdentityLinkWrapperKey,
    ) -> Result<EncryptedGroupTitle, GroupTitleEncryptionError> {
        let padded_title = GroupTitle::new(plaintext);
        let plaintext = PersistenceCodec::to_vec(&padded_title)?;
        let aad = b"group-title";
        let payload = Payload {
            msg: plaintext.as_slice(),
            aad,
        };
        let aead_ciphertext = identity_link_wrapper_key.encrypt(payload)?;
        let (ciphertext, nonce) = aead_ciphertext.into_parts();
        Ok(EncryptedGroupTitle {
            ciphertext,
            nonce,
            aad: aad.to_vec(),
        })
    }

    pub fn decrypt(
        self,
        identity_link_wrapper_key: &IdentityLinkWrapperKey,
    ) -> Result<String, GroupTitleDecryptionError> {
        let aead_ciphertext = AeadCiphertext::new(self.ciphertext, self.nonce);
        let plaintext = identity_link_wrapper_key.decrypt_with_aad(&aead_ciphertext, &self.aad)?;
        let padded_title: GroupTitle = PersistenceCodec::from_slice(&plaintext)?;
        Ok(padded_title.title.into_owned())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GroupTitleEncryptionError {
    #[error(transparent)]
    Encryption(#[from] EncryptionError),
    #[error(transparent)]
    Codec(#[from] codec::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum GroupTitleDecryptionError {
    #[error(transparent)]
    Decryption(#[from] DecryptionError),
    #[error(transparent)]
    Codec(#[from] codec::Error),
}

#[cfg(test)]
mod test {
    use aircommon::codec::PersistenceCodec;
    use uuid::uuid;

    use super::*;

    fn test_group_data() -> GroupData {
        GroupData {
            title: "Group Title".to_string(),
            encrypted_title: Some(EncryptedGroupTitle {
                ciphertext: b"title-ciphertext".to_vec(),
                nonce: [0xAA; _],
                aad: b"group-title".to_vec(),
            }),
            external_group_profile: Some(ExternalGroupProfile {
                object_id: uuid!("89fea7df-3823-4688-8915-00ab38db1577"),
                size: 42,
                enc_alg: Some(EncryptionAlgorithm::Aes256Gcm),
                nonce: [0xBB; _],
                aad: b"group-profile".to_vec(),
                hash_alg: HashAlgorithm::Sha256,
                content_hash: [0xCC; 32].to_vec(),
            }),
        }
    }

    #[test]
    fn group_data_stability() {
        let bytes = PersistenceCodec::to_vec(&test_group_data()).unwrap();
        let diag = cbor_diag::parse_bytes(&bytes[1..]).unwrap().to_hex();
        insta::assert_snapshot!(diag);
    }

    #[test]
    fn group_profile_stability() {
        let profile = GroupProfile::new(
            "Group Title".to_string(),
            Some("Group Description".to_string()),
            Some(vec![1, 2, 3].into()),
        );
        let bytes = PersistenceCodec::to_vec(&profile).unwrap();
        let diag = cbor_diag::parse_bytes(&bytes[1..]).unwrap().to_hex();
        insta::assert_snapshot!(diag);
    }

    #[test]
    fn group_title_roundtrip() {
        let title = GroupTitle::new("Hello Group");
        let bytes = PersistenceCodec::to_vec(&title).unwrap();
        let decoded: GroupTitle = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(title, decoded);
    }

    #[test]
    fn group_title_stability() {
        let title = GroupTitle::new("Hello Group");
        let bytes = PersistenceCodec::to_vec(&title).unwrap();
        let diag = cbor_diag::parse_bytes(&bytes[1..]).unwrap().to_hex();
        insta::assert_snapshot!(diag);
    }

    #[test]
    fn encrypted_group_title_roundtrip() {
        let key = IdentityLinkWrapperKey::random().unwrap();
        let original = "Hello encrypted title";
        let encrypted = EncryptedGroupTitle::encrypt(original, &key).unwrap();
        let decrypted = encrypted.decrypt(&key).unwrap();
        assert_eq!(decrypted, original);
    }

    #[test]
    fn group_profile_encrypt_decrypt_roundtrip() {
        let key = IdentityLinkWrapperKey::random().unwrap();
        let profile = GroupProfile::new(
            "My Group".to_string(),
            Some("A test group".to_string()),
            Some(vec![0xDE, 0xAD, 0xBE, 0xEF].into()),
        );
        let (ciphertext, builder) = profile.encrypt(&key).unwrap();
        let external = builder.build(Uuid::new_v4());
        let decrypted = GroupProfile::decrypt(&key, &external, ciphertext).unwrap();
        assert_eq!(decrypted, profile);
    }

    #[test]
    fn group_data_backward_compatibility() {
        #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
        struct OldGroupData {
            title: String,
            picture: Option<Vec<u8>>,
        }

        let group_data = test_group_data();
        let old_group_data = OldGroupData {
            title: group_data.title.clone(),
            picture: None,
        };

        let bytes = PersistenceCodec::to_vec(&group_data).unwrap();
        let value: OldGroupData = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(value, old_group_data);

        let bytes = PersistenceCodec::to_vec(&old_group_data).unwrap();
        let value: GroupData = PersistenceCodec::from_slice(&bytes).unwrap();
        assert_eq!(
            value,
            GroupData {
                encrypted_title: None,
                external_group_profile: None,
                ..group_data
            }
        );
    }
}
