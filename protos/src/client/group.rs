// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Client protocol types related to groups.

use std::borrow::Cow;

use aircommon::{
    codec::{self, PersistenceCodec},
    crypto::{
        ear::{AEAD_NONCE_SIZE, AeadCiphertext, EarKey, keys::IdentityLinkWrapperKey},
        errors::{DecryptionError, EncryptionError},
    },
};
use airmacros::{Deserialize_tagged_map, Serialize_tagged_map};
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
    pub picture: Option<Vec<u8>>,
    /// Encrypted group title
    ///
    /// It is encrypted with the same key and algorithm as the external group profile. It is
    /// included in this data to be able to use the group title immediately without having to fetch
    /// the external group profile.
    pub encrypted_title: Option<EncryptedGroupTitle>,
    /// A pointer to an external encrypted group profile
    ///
    /// Using this data, it is possible to retrieve the group profile from the object storage.
    #[serde(default)]
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
/// ExtenalGroupProfile = {
///   object_id: bytes .size 16 .tag 1,
///   encrypted_title: EncryptedGroupTitle .tag 2,
///   size: uint .size 8 .tag 3,
///   encAlg: uint .size 2 .tag 4,
///   nonce: bstr .tag 5,
///   aad: bstr .tag 6,
///   hashAlg: uint .size 1 .tag 7,
///   contentHash: bstr .tag 8,
/// }
/// ```
#[derive(Debug, Clone, Eq, PartialEq, Serialize_tagged_map, Deserialize_tagged_map)]
pub struct ExternalGroupProfile {
    /// Object ID in the object storage
    ///
    /// Via this ID, the chat attributes can be retrieved from the object storage.
    #[tag(1)]
    pub object_id: Uuid,
    /// Size of the content in bytes
    #[tag(3)]
    pub size: u64,
    /// An IANA AEAD Algorithm, not `None`
    #[tag(4)]
    pub enc_alg: EncryptionAlgorithm,
    /// AEAD nonce
    #[tag(5)]
    pub nonce: [u8; AEAD_NONCE_SIZE],
    /// AEAD additional authentication data
    #[tag(6)]
    pub aad: Vec<u8>,
    /// An IANA Named Information Hash Algorithm
    #[tag(7)]
    pub hash_alg: HashAlgorithm,
    /// Hash of the content (which one: encrypted or plaintext?)
    #[tag(8)]
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
#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize_tagged_map, Deserialize_tagged_map)]
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
/// }
/// ```
#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize_tagged_map, Deserialize_tagged_map)]
pub struct EncryptedGroupTitle {
    /// Ciphertext of a utf-8 encoded string
    #[tag(1)]
    pub ciphertext: Vec<u8>,
    #[tag(2)]
    pub nonce: [u8; AEAD_NONCE_SIZE],
}

/// Group profile stored as encrypted blob in the object storage.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize_tagged_map, Deserialize_tagged_map)]
pub struct GroupProfile {
    #[tag(1)]
    pub title: String,
    #[tag(2)]
    pub description: Option<String>,
    #[tag(3)]
    pub picture: Option<Vec<u8>>,
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

impl GroupProfile {
    pub fn encrypt(
        &self,
        identity_link_wrapper_key: &IdentityLinkWrapperKey,
    ) -> Result<(Vec<u8>, ExternalGroupProfileBuilder), GroupProfileEncryptionError> {
        const AIR_GROUP_PROFILE_ENCRYPTION_ALG: EncryptionAlgorithm =
            EncryptionAlgorithm::Aes256Gcm;
        const AIR_GROUP_PROFILE_HASH_ALG: HashAlgorithm = HashAlgorithm::Sha256;

        let plaintext = PersistenceCodec::to_vec(self)?;
        let size = plaintext
            .len()
            .try_into()
            .map_err(|_| GroupProfileEncryptionError::UsizeOverflow)?;
        let content_hash = Sha256::digest(&plaintext);

        let aead_ciphertext = identity_link_wrapper_key.encrypt(plaintext.as_slice())?;
        let (ciphertext, nonce) = aead_ciphertext.into_parts();

        let external = ExternalGroupProfile {
            object_id: Uuid::nil(),
            size,
            enc_alg: AIR_GROUP_PROFILE_ENCRYPTION_ALG,
            nonce,
            aad: Vec::new(),
            hash_alg: AIR_GROUP_PROFILE_HASH_ALG,
            content_hash: content_hash.to_vec(),
        };
        let builder = ExternalGroupProfileBuilder { inner: external };

        Ok((ciphertext, builder))
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

impl<'a> GroupTitle<'a> {
    fn new(title: &'a str) -> Self {
        const AIR_GROUP_TITLE_PADDING: usize = 32;
        let size = title.len();
        let padding = AIR_GROUP_TITLE_PADDING - (size % AIR_GROUP_TITLE_PADDING);
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
        let aead_ciphertext = identity_link_wrapper_key.encrypt(plaintext.as_slice())?;
        let (ciphertext, nonce) = aead_ciphertext.into_parts();
        Ok(EncryptedGroupTitle { ciphertext, nonce })
    }

    pub fn decrypt(
        self,
        identity_link_wrapper_key: &IdentityLinkWrapperKey,
    ) -> Result<String, GroupTitleDecryptionError> {
        let aead_ciphertext = AeadCiphertext::new(self.ciphertext, self.nonce);
        let plaintext = identity_link_wrapper_key.decrypt(&aead_ciphertext)?;
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
            picture: Some(vec![1, 2, 3]),
            encrypted_title: Some(EncryptedGroupTitle {
                ciphertext: b"title-ciphertext".to_vec(),
                nonce: [0xAA; _],
            }),
            external_group_profile: Some(ExternalGroupProfile {
                object_id: uuid!("89fea7df-3823-4688-8915-00ab38db1577"),
                size: 42,
                enc_alg: EncryptionAlgorithm::Aes256Gcm,
                nonce: [0xBB; _],
                aad: Vec::new(),
                hash_alg: HashAlgorithm::Sha256,
                content_hash: [0xCC; 32].to_vec(),
            }),
        }
    }

    #[test]
    fn group_data_stability() {
        let bytes = PersistenceCodec::to_vec(&test_group_data()).unwrap();
        insta::assert_binary_snapshot!(".cbor", bytes);
    }

    #[test]
    fn group_profile_stability() {
        let profile = GroupProfile {
            title: "Group Title".to_string(),
            description: Some("Group Description".to_string()),
            picture: Some(vec![1, 2, 3]),
        };
        let bytes = PersistenceCodec::to_vec(&profile).unwrap();
        insta::assert_binary_snapshot!(".cbor", bytes);
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
        insta::assert_binary_snapshot!(".cbor", bytes);
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
            picture: group_data.picture.clone(),
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
