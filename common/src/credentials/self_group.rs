// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Credential type for self-group leaves.

use airmacros::{DeserializeTaggedMap, SerializeTaggedMap};
use mls_assist::openmls::prelude::{Credential, CredentialType};
use uuid::Uuid;

use crate::codec::{self, PersistenceCodec};

/// MLS credential type of a [`SelfGroupCredential`].
///
/// In the IANA private-use range (0xF000-0xFFFF), following the workspace convention of `0xFFxx`
/// for private MLS codepoints.
pub const SELF_GROUP_CREDENTIAL_TYPE: u16 = 0xff02;

/// Credential carried by self-group leaves.
///
/// Unlike a [`UserCredential`](super::UserCredential), it carries no key material and no
/// signature: DS requests from self-group members are verified against the leaf's MLS signature
/// key, and the creation of a self-group is authenticated by a [`UserCredential`] sent alongside
/// the request.
///
/// Serialized as a tagged CBOR map so fields can be added without breaking older clients.
#[derive(Debug, Clone, PartialEq, Eq, SerializeTaggedMap, DeserializeTaggedMap)]
pub struct SelfGroupCredential {
    /// Identifies the client (device) that owns the leaf. Generated at client creation and unique
    /// per client, in contrast to the per-user identity in a [`UserCredential`](super::UserCredential).
    #[tag(1)]
    client_id: Uuid,
}

impl SelfGroupCredential {
    pub fn new(client_id: Uuid) -> Self {
        Self { client_id }
    }

    pub fn client_id(&self) -> Uuid {
        self.client_id
    }

    pub fn to_credential(&self) -> Result<Credential, codec::Error> {
        Ok(Credential::new(
            CredentialType::Other(SELF_GROUP_CREDENTIAL_TYPE),
            PersistenceCodec::to_vec(self)?,
        ))
    }

    pub fn from_credential(credential: &Credential) -> Result<Self, SelfGroupCredentialError> {
        let CredentialType::Other(SELF_GROUP_CREDENTIAL_TYPE) = credential.credential_type() else {
            return Err(SelfGroupCredentialError::WrongCredentialType);
        };
        let credential: Self = PersistenceCodec::from_slice(credential.serialized_content())?;
        // The tagged-map codec defaults absent fields, so a credential without a client id
        // decodes as the nil UUID. Reject it to uphold the unique-per-client invariant.
        if credential.client_id.is_nil() {
            return Err(SelfGroupCredentialError::NilClientId);
        }
        Ok(credential)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SelfGroupCredentialError {
    #[error("wrong credential type")]
    WrongCredentialType,
    #[error("nil client id")]
    NilClientId,
    #[error(transparent)]
    Codec(#[from] codec::Error),
}

#[cfg(test)]
mod test {
    use mls_assist::openmls::prelude::BasicCredential;

    use super::*;

    #[test]
    fn credential_type_is_private_and_not_grease() {
        assert!((0xf000..=0xffff).contains(&SELF_GROUP_CREDENTIAL_TYPE));
        // GREASE values have the form 0x?A?A.
        assert_ne!(SELF_GROUP_CREDENTIAL_TYPE & 0x0f0f, 0x0a0a);
    }

    #[test]
    fn credential_round_trip() {
        let credential = SelfGroupCredential::new(Uuid::new_v4());
        let mls_credential = credential.to_credential().unwrap();
        assert_eq!(
            mls_credential.credential_type(),
            CredentialType::Other(SELF_GROUP_CREDENTIAL_TYPE)
        );
        let parsed = SelfGroupCredential::from_credential(&mls_credential).unwrap();
        assert_eq!(parsed, credential);
    }

    #[test]
    fn wrong_credential_type_is_rejected() {
        let basic = BasicCredential::new(b"identity".to_vec()).into();
        let error = SelfGroupCredential::from_credential(&basic).unwrap_err();
        assert!(matches!(
            error,
            SelfGroupCredentialError::WrongCredentialType
        ));
    }

    /// The tagged-map codec defaults absent fields, so an empty map would otherwise decode as
    /// the nil UUID.
    #[test]
    fn credential_without_client_id_is_rejected() {
        // 0x01 is the persistence codec version, 0xa0 is an empty CBOR map.
        let credential = Credential::new(
            CredentialType::Other(SELF_GROUP_CREDENTIAL_TYPE),
            vec![0x01, 0xa0],
        );
        let error = SelfGroupCredential::from_credential(&credential).unwrap_err();
        assert!(matches!(error, SelfGroupCredentialError::NilClientId));
    }

    #[test]
    fn nil_client_id_is_rejected() {
        let mls_credential = SelfGroupCredential::new(Uuid::nil())
            .to_credential()
            .unwrap();
        let error = SelfGroupCredential::from_credential(&mls_credential).unwrap_err();
        assert!(matches!(error, SelfGroupCredentialError::NilClientId));
    }

    /// The serialized form is a wire format and must stay stable.
    #[test]
    fn serialization_stability() {
        let credential =
            SelfGroupCredential::new(Uuid::from_u128(0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10));
        let bytes = PersistenceCodec::to_vec(&credential).unwrap();
        // The first byte is the persistence codec version, the rest is CBOR.
        let diag = cbor_diag::parse_bytes(&bytes[1..]).unwrap().to_hex();
        insta::assert_snapshot!(diag);
    }
}
