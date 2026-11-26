// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Credential type for self-group leaves.

use airmacros::{DeserializeTaggedMap, SerializeTaggedMap};
use mls_assist::openmls::prelude::{Credential, CredentialType};
use tls_codec::{DeserializeBytes, Serialize as _};
use uuid::Uuid;

use crate::{
    LibraryError,
    codec::{self, PersistenceCodec},
    crypto::signatures::{
        private_keys::SignatureVerificationError,
        signable::{Signable, SignedStruct, Verifiable, VerifiedStruct},
    },
    identifiers::UserId,
};

use super::{
    keys::{ClientKeyType, ClientSignature, ClientVerifyingKey, UserSigningKey},
    private_mod,
};

/// MLS credential type of a [`SelfGroupCredential`].
///
/// In the IANA private-use range (0xF000-0xFFFF), following the workspace convention of `0xFFxx`
/// for private MLS codepoints.
pub const SELF_GROUP_CREDENTIAL_TYPE: u16 = 0xff02;

/// Label of the user signature over a [`SelfGroupCredential`] payload.
const SELF_GROUP_CREDENTIAL_LABEL: &str = "SelfGroupCredential";

/// Credential carried by self-group leaves.
///
/// It contains the ID of the client that owns the leaf and can carry a payload signed with the
/// user signing key, which binds the client ID and the leaf's MLS signature key to the user.
///
/// Serialized as a tagged CBOR map so fields can be added without breaking older clients,
/// preceded by a persistence-codec version byte. The payload is a CBOR map without the version
/// byte, signed as the bytes under tag 2:
///
/// ```cddl
/// SelfGroupCredential = {
///   1: bstr .size 16,   ; client_id
///   ? 2: bstr,          ; payload, an encoded SelfGroupCredentialPayload
///   ? 3: bstr,          ; signature over the bytes of 2
/// }
///
/// SelfGroupCredentialPayload = {
///   1: bstr .size 16,   ; client_id
///   2: bstr,            ; user_id, TLS-serialized
///   3: bstr,            ; signature_key, the leaf's MLS signature key
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, SerializeTaggedMap, DeserializeTaggedMap)]
pub struct SelfGroupCredential {
    /// Identifies the client (device) that owns the leaf. Generated at client creation and unique
    /// per client, in contrast to the per-user identity in a [`UserCredential`](super::UserCredential).
    #[tag(1)]
    client_id: Uuid,
    #[tag(2)]
    payload: Option<Vec<u8>>,
    #[tag(3)]
    signature: Option<Vec<u8>>,
}

/// The state of tags 2 and 3 of a [`SelfGroupCredential`].
enum SignedPart<'a> {
    /// Both tags are absent or null.
    Unsigned,
    /// Both tags are non-empty byte strings.
    Signed {
        payload: &'a [u8],
        signature: &'a [u8],
    },
    /// Any other combination.
    Malformed,
}

impl SelfGroupCredential {
    /// Creates an unsigned credential.
    pub fn new(client_id: Uuid) -> Self {
        Self {
            client_id,
            payload: None,
            signature: None,
        }
    }

    /// Creates a credential signed with `user_signing_key`.
    ///
    /// The payload binds `client_id` and `leaf_signature_key`, the MLS signature key of the leaf
    /// that carries the credential, to the user of `user_signing_key`.
    pub fn signed(
        client_id: Uuid,
        leaf_signature_key: &[u8],
        user_signing_key: &UserSigningKey,
    ) -> Result<Self, LibraryError> {
        let payload = SelfGroupCredentialPayload {
            client_id,
            user_id: user_signing_key
                .credential()
                .user_id()
                .tls_serialize_detached()
                .map_err(LibraryError::missing_bound_check)?,
            signature_key: leaf_signature_key.to_vec(),
        };
        let payload_bytes = minicbor_serde::to_vec(&payload).map_err(|_| LibraryError)?;
        Self::sign_payload_bytes(client_id, payload_bytes, user_signing_key)
    }

    /// Creates a credential for `client_id` that carries `payload_bytes` as they are, signed
    /// with `user_signing_key`.
    fn sign_payload_bytes(
        client_id: Uuid,
        payload_bytes: Vec<u8>,
        user_signing_key: &UserSigningKey,
    ) -> Result<Self, LibraryError> {
        let SignedPayload {
            payload_bytes,
            signature,
        } = PayloadTbs { payload_bytes }.sign(&**user_signing_key)?;
        Ok(Self {
            client_id,
            payload: Some(payload_bytes),
            signature: Some(signature.into_bytes()),
        })
    }

    pub fn client_id(&self) -> Uuid {
        self.client_id
    }

    /// Returns `true` if both the payload and the signature are present.
    ///
    /// This says nothing about whether the signature is valid.
    pub fn is_signed(&self) -> bool {
        match self.signed_part() {
            SignedPart::Signed { .. } => true,
            SignedPart::Unsigned | SignedPart::Malformed => false,
        }
    }

    fn signed_part(&self) -> SignedPart<'_> {
        match (&self.payload, &self.signature) {
            (None, None) => SignedPart::Unsigned,
            (Some(payload), Some(signature)) if !payload.is_empty() && !signature.is_empty() => {
                SignedPart::Signed { payload, signature }
            }
            _ => SignedPart::Malformed,
        }
    }

    pub fn to_credential(&self) -> Result<Credential, codec::Error> {
        Ok(Credential::new(
            CredentialType::Other(SELF_GROUP_CREDENTIAL_TYPE),
            PersistenceCodec::to_vec(self)?,
        ))
    }

    /// Parses a self-group credential.
    ///
    /// Fails only if the credential type is wrong or the client ID can't be read. Any value under
    /// tags 2 and 3 is accepted, so that the client ID of a credential with a malformed signed
    /// part can still be read. [`SelfGroupCredential::verify`] rejects such a credential.
    pub fn from_credential(credential: &Credential) -> Result<Self, SelfGroupCredentialError> {
        let CredentialType::Other(SELF_GROUP_CREDENTIAL_TYPE) = credential.credential_type() else {
            return Err(SelfGroupCredentialError::WrongCredentialType);
        };
        let bytes = credential.serialized_content();
        let credential = match PersistenceCodec::from_slice::<Self>(bytes) {
            Ok(credential) => credential,
            // A value under tag 2 or 3 that isn't null or a byte string fails the decode. Read the
            // client ID alone, so that every credential readable before signed parts existed stays
            // readable, and mark the signed part as malformed with empty byte strings.
            Err(error) => {
                let ClientIdOnly { client_id } =
                    PersistenceCodec::from_slice(bytes).map_err(|_| error)?;
                Self {
                    client_id,
                    payload: Some(Vec::new()),
                    signature: Some(Vec::new()),
                }
            }
        };
        // The tagged-map codec defaults absent fields, so a credential without a client id
        // decodes as the nil UUID. Reject it to uphold the unique-per-client invariant.
        if credential.client_id.is_nil() {
            return Err(SelfGroupCredentialError::NilClientId);
        }
        Ok(credential)
    }

    /// Reads the payload of a signed credential without checking its signature.
    ///
    /// Returns `None` for an unsigned credential. Fails for a malformed credential and for a payload
    /// that doesn't decode or whose client ID differs from the credential's.
    ///
    /// Nothing in the result is authenticated.
    pub fn unverified_payload(
        &self,
    ) -> Result<Option<UnverifiedSelfGroupPayload>, SelfGroupCredentialVerificationError> {
        match self.signed_part() {
            SignedPart::Unsigned => Ok(None),
            SignedPart::Malformed => Err(SelfGroupCredentialVerificationError::Malformed),
            SignedPart::Signed { payload, .. } => self.decode_payload(payload).map(Some),
        }
    }

    /// Verifies the credential of a leaf whose MLS signature key is `leaf_signature_key`.
    ///
    /// Unsigned credentials pass for now. This will be disabled as soon as all unsigned credentials
    /// in self-groups have been replaced.
    pub fn verify(
        &self,
        leaf_signature_key: &[u8],
        expected_user_id: &UserId,
        user_verifying_key: &ClientVerifyingKey,
    ) -> Result<(), SelfGroupCredentialVerificationError> {
        let (payload, signature) = match self.signed_part() {
            SignedPart::Unsigned => return Ok(()),
            SignedPart::Malformed => return Err(SelfGroupCredentialVerificationError::Malformed),
            SignedPart::Signed { payload, signature } => (payload, signature),
        };
        let VerifiedPayloadBytes(payload) = VerifiablePayload { payload, signature }
            .verify::<VerifiedPayloadBytes>(user_verifying_key)
            .map_err(SelfGroupCredentialVerificationError::Signature)?;
        let UnverifiedSelfGroupPayload {
            user_id,
            signature_key,
        } = self.decode_payload(payload)?;
        if user_id != *expected_user_id {
            return Err(SelfGroupCredentialVerificationError::UserIdMismatch);
        }
        if signature_key != leaf_signature_key {
            return Err(SelfGroupCredentialVerificationError::SignatureKeyMismatch);
        }
        Ok(())
    }

    fn decode_payload(
        &self,
        payload: &[u8],
    ) -> Result<UnverifiedSelfGroupPayload, SelfGroupCredentialVerificationError> {
        use SelfGroupCredentialVerificationError::InvalidPayload;
        let SelfGroupCredentialPayload {
            client_id,
            user_id,
            signature_key,
        } = minicbor_serde::from_slice(payload).map_err(|_| InvalidPayload("undecodable"))?;
        if client_id.is_nil() {
            return Err(InvalidPayload("missing client ID"));
        }
        let user_id = UserId::tls_deserialize_exact_bytes(&user_id)
            .map_err(|_| InvalidPayload("invalid user ID"))?;
        if signature_key.is_empty() {
            return Err(InvalidPayload("missing signature key"));
        }
        if client_id != self.client_id {
            return Err(SelfGroupCredentialVerificationError::ClientIdMismatch);
        }
        Ok(UnverifiedSelfGroupPayload {
            user_id,
            signature_key,
        })
    }
}

/// Payload of a signed [`SelfGroupCredential`], read without checking the signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnverifiedSelfGroupPayload {
    user_id: UserId,
    signature_key: Vec<u8>,
}

impl UnverifiedSelfGroupPayload {
    /// The user ID in the payload. Not authenticated.
    pub fn user_id(&self) -> &UserId {
        &self.user_id
    }

    /// The leaf signature key in the payload. Not authenticated.
    pub fn signature_key(&self) -> &[u8] {
        &self.signature_key
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

#[derive(Debug, thiserror::Error)]
pub enum SelfGroupCredentialVerificationError {
    #[error("payload and signature must both be non-empty byte strings or both be absent")]
    Malformed,
    #[error("invalid signature: {0}")]
    Signature(SignatureVerificationError),
    #[error("invalid payload: {0}")]
    InvalidPayload(&'static str),
    #[error("payload contains another client ID than the credential")]
    ClientIdMismatch,
    #[error("payload contains another user ID")]
    UserIdMismatch,
    #[error("payload contains another leaf signature key")]
    SignatureKeyMismatch,
}

#[derive(SerializeTaggedMap, DeserializeTaggedMap)]
struct SelfGroupCredentialPayload {
    #[tag(1)]
    client_id: Uuid,
    #[tag(2)]
    user_id: Vec<u8>,
    #[tag(3)]
    signature_key: Vec<u8>,
}

/// Reads only the client ID of a [`SelfGroupCredential`] and skips every other tag.
#[derive(DeserializeTaggedMap)]
struct ClientIdOnly {
    #[tag(1)]
    client_id: Uuid,
}

#[derive(Debug)]
struct PayloadTbs {
    payload_bytes: Vec<u8>,
}

impl Signable for PayloadTbs {
    type SignedOutput = SignedPayload;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.payload_bytes.clone())
    }

    fn label(&self) -> &str {
        SELF_GROUP_CREDENTIAL_LABEL
    }
}

struct SignedPayload {
    payload_bytes: Vec<u8>,
    signature: ClientSignature,
}

impl SignedStruct<PayloadTbs, ClientKeyType> for SignedPayload {
    fn from_payload(tbs: PayloadTbs, signature: ClientSignature) -> Self {
        Self {
            payload_bytes: tbs.payload_bytes,
            signature,
        }
    }
}

#[derive(Debug)]
struct VerifiablePayload<'a> {
    payload: &'a [u8],
    signature: &'a [u8],
}

impl Verifiable for VerifiablePayload<'_> {
    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        Ok(self.payload.to_vec())
    }

    fn signature(&self) -> impl AsRef<[u8]> {
        self.signature
    }

    fn label(&self) -> &str {
        SELF_GROUP_CREDENTIAL_LABEL
    }
}

/// Payload bytes whose signature verified.
struct VerifiedPayloadBytes<'a>(&'a [u8]);

impl<'a> VerifiedStruct<VerifiablePayload<'a>> for VerifiedPayloadBytes<'a> {
    type SealingType = private_mod::Seal;

    fn from_verifiable(verifiable: VerifiablePayload<'a>, _seal: Self::SealingType) -> Self {
        Self(verifiable.payload)
    }
}

#[cfg(test)]
mod test {
    use mls_assist::openmls::prelude::BasicCredential;

    use crate::{
        credentials::test_utils::{
            create_test_credentials, malformed_self_group_credential, other_key_of,
            random_user_signing_key,
        },
        crypto::signatures::private_keys::SigningKey,
    };

    use super::*;

    fn leaf_key() -> Vec<u8> {
        SigningKey::<ClientKeyType>::generate()
            .unwrap()
            .verifying_key()
            .as_slice()
            .to_vec()
    }

    /// A CBOR value under a tag of a test credential.
    enum Value<'a> {
        Bytes(&'a [u8]),
        Null,
        Text,
        Int,
        Array,
        Tagged,
        Undefined,
    }

    impl Value<'_> {
        fn encode(&self, encoder: &mut minicbor::Encoder<&mut Vec<u8>>) {
            match self {
                Value::Bytes(bytes) => encoder.bytes(bytes).unwrap(),
                Value::Null => encoder.null().unwrap(),
                Value::Text => encoder.str("text").unwrap(),
                Value::Int => encoder.u8(7).unwrap(),
                Value::Array => encoder.array(2).unwrap().u8(1).unwrap().str("x").unwrap(),
                Value::Tagged => encoder
                    .tag(minicbor::data::Tag::new(24))
                    .unwrap()
                    .bytes(b"bytes")
                    .unwrap(),
                Value::Undefined => encoder.undefined().unwrap(),
            };
        }
    }

    /// Encodes a CBOR map with integer keys.
    fn encode_cbor_map(entries: &[(u32, Value)]) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut encoder = minicbor::Encoder::new(&mut bytes);
        encoder.map(entries.len() as u64).unwrap();
        for (tag, value) in entries {
            encoder.u32(*tag).unwrap();
            value.encode(&mut encoder);
        }
        bytes
    }

    /// Encodes a credential map with the persistence codec version byte.
    fn encode_map(entries: &[(u32, Value)]) -> Credential {
        let mut bytes = vec![0x01];
        bytes.extend(encode_cbor_map(entries));
        Credential::new(CredentialType::Other(SELF_GROUP_CREDENTIAL_TYPE), bytes)
    }

    /// A copy of the decoder used before signed parts existed.
    #[derive(Debug, DeserializeTaggedMap)]
    struct FrozenSelfGroupCredential {
        #[tag(1)]
        client_id: Uuid,
    }

    fn parse(credential: &Credential) -> SelfGroupCredential {
        SelfGroupCredential::from_credential(credential).unwrap()
    }

    fn signed_credential() -> (SelfGroupCredential, Vec<u8>, UserSigningKey) {
        let user_signing_key = random_user_signing_key();
        let leaf_key = leaf_key();
        let credential =
            SelfGroupCredential::signed(Uuid::new_v4(), &leaf_key, &user_signing_key).unwrap();
        (credential, leaf_key, user_signing_key)
    }

    fn verify_with(
        credential: &SelfGroupCredential,
        leaf_key: &[u8],
        user_signing_key: &UserSigningKey,
    ) -> Result<(), SelfGroupCredentialVerificationError> {
        credential.verify(
            leaf_key,
            user_signing_key.credential().user_id(),
            user_signing_key.credential().verifying_key(),
        )
    }

    #[test]
    fn credential_type_is_private_and_not_grease() {
        assert!((0xf000..=0xffff).contains(&SELF_GROUP_CREDENTIAL_TYPE));
        // GREASE values have the form 0x?A?A.
        assert_ne!(SELF_GROUP_CREDENTIAL_TYPE & 0x0f0f, 0x0a0a);
    }

    #[test]
    fn credential_round_trip() {
        let (signed, _, _) = signed_credential();
        for (credential, is_signed) in [
            (SelfGroupCredential::new(Uuid::new_v4()), false),
            (signed, true),
        ] {
            let mls_credential = credential.to_credential().unwrap();
            assert_eq!(
                mls_credential.credential_type(),
                CredentialType::Other(SELF_GROUP_CREDENTIAL_TYPE)
            );
            let parsed = parse(&mls_credential);
            assert_eq!(parsed, credential);
            assert_eq!(parsed.is_signed(), is_signed);
        }
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
        let credential = encode_map(&[]);
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

    #[test]
    fn signed_credential_verifies() {
        let (credential, leaf_key, user_signing_key) = signed_credential();
        verify_with(&credential, &leaf_key, &user_signing_key).unwrap();

        let payload = credential.unverified_payload().unwrap().unwrap();
        assert_eq!(payload.user_id(), user_signing_key.credential().user_id());
        assert_eq!(payload.signature_key(), leaf_key.as_slice());
    }

    #[test]
    fn unsigned_credential_verifies_without_payload() {
        let credential = SelfGroupCredential::new(Uuid::new_v4());
        verify_with(&credential, &leaf_key(), &random_user_signing_key()).unwrap();
        assert_eq!(credential.unverified_payload().unwrap(), None);
    }

    #[test]
    fn wrong_signature_fails() {
        let (mut credential, leaf_key, user_signing_key) = signed_credential();
        let Some(signature) = &mut credential.signature else {
            panic!("expected a signature");
        };
        signature[0] ^= 0xff;
        let error = verify_with(&credential, &leaf_key, &user_signing_key).unwrap_err();
        assert!(matches!(
            error,
            SelfGroupCredentialVerificationError::Signature(_)
        ));
    }

    #[test]
    fn another_leaf_key_fails() {
        let (credential, _, user_signing_key) = signed_credential();
        let error = verify_with(&credential, &leaf_key(), &user_signing_key).unwrap_err();
        assert!(matches!(
            error,
            SelfGroupCredentialVerificationError::SignatureKeyMismatch
        ));
    }

    #[test]
    fn another_users_key_fails() {
        let (credential, leaf_key, user_signing_key) = signed_credential();
        let other = other_key_of(&user_signing_key);
        let error = verify_with(&credential, &leaf_key, &other).unwrap_err();
        assert!(matches!(
            error,
            SelfGroupCredentialVerificationError::Signature(_)
        ));
    }

    #[test]
    fn wrong_user_id_fails() {
        let (credential, leaf_key, user_signing_key) = signed_credential();
        let error = credential
            .verify(
                &leaf_key,
                &UserId::random("example.com".parse().unwrap()),
                user_signing_key.credential().verifying_key(),
            )
            .unwrap_err();
        assert!(matches!(
            error,
            SelfGroupCredentialVerificationError::UserIdMismatch
        ));
    }

    #[test]
    fn payload_with_another_client_id_fails() {
        let (mut credential, leaf_key, user_signing_key) = signed_credential();
        credential.client_id = Uuid::new_v4();
        let error = verify_with(&credential, &leaf_key, &user_signing_key).unwrap_err();
        assert!(matches!(
            error,
            SelfGroupCredentialVerificationError::ClientIdMismatch
        ));
    }

    #[test]
    fn payload_with_a_missing_field_fails() {
        let user_signing_key = random_user_signing_key();
        let leaf_key = leaf_key();
        let client_id = Uuid::new_v4();
        let without_user_id = minicbor_serde::to_vec(&SelfGroupCredentialPayload {
            client_id,
            user_id: Vec::new(),
            signature_key: leaf_key.clone(),
        })
        .unwrap();
        let credentials = [
            SelfGroupCredential::signed(Uuid::nil(), &leaf_key, &user_signing_key),
            SelfGroupCredential::signed(client_id, &[], &user_signing_key),
            SelfGroupCredential::sign_payload_bytes(client_id, without_user_id, &user_signing_key),
        ];
        for credential in credentials {
            let credential = credential.unwrap();
            let error = verify_with(&credential, &leaf_key, &user_signing_key).unwrap_err();
            assert!(
                matches!(
                    error,
                    SelfGroupCredentialVerificationError::InvalidPayload(_)
                ),
                "{error:?}"
            );
            assert!(credential.unverified_payload().is_err());
        }
    }

    #[test]
    fn signature_under_another_label_fails() {
        #[derive(Debug)]
        struct OtherLabel(Vec<u8>);

        impl Signable for OtherLabel {
            type SignedOutput = SignedPayload;

            fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
                Ok(self.0.clone())
            }

            fn label(&self) -> &str {
                "SignedConnectionPackage"
            }
        }

        impl SignedStruct<OtherLabel, ClientKeyType> for SignedPayload {
            fn from_payload(tbs: OtherLabel, signature: ClientSignature) -> Self {
                Self {
                    payload_bytes: tbs.0,
                    signature,
                }
            }
        }

        let (mut credential, leaf_key, user_signing_key) = signed_credential();
        let payload = credential.payload.take().unwrap();
        let signed = OtherLabel(payload).sign(&*user_signing_key).unwrap();
        credential.payload = Some(signed.payload_bytes);
        credential.signature = Some(signed.signature.into_bytes());
        let error = verify_with(&credential, &leaf_key, &user_signing_key).unwrap_err();
        assert!(matches!(
            error,
            SelfGroupCredentialVerificationError::Signature(_)
        ));
    }

    #[test]
    fn re_encoded_payload_fails() {
        let (mut credential, leaf_key, user_signing_key) = signed_credential();
        let Some(payload) = &credential.payload else {
            panic!("expected a payload");
        };
        // An indefinite-length map with the same entries decodes to the same fields.
        let SelfGroupCredentialPayload {
            client_id,
            user_id,
            signature_key,
        } = minicbor_serde::from_slice(payload).unwrap();
        let mut re_encoded = Vec::new();
        minicbor::Encoder::new(&mut re_encoded)
            .begin_map()
            .unwrap()
            .u8(1)
            .unwrap()
            .bytes(client_id.as_bytes())
            .unwrap()
            .u8(2)
            .unwrap()
            .bytes(&user_id)
            .unwrap()
            .u8(3)
            .unwrap()
            .bytes(&signature_key)
            .unwrap()
            .end()
            .unwrap();
        assert_ne!(&re_encoded, payload);
        credential.payload = Some(re_encoded);
        assert!(credential.unverified_payload().is_ok());

        let error = verify_with(&credential, &leaf_key, &user_signing_key).unwrap_err();
        assert!(matches!(
            error,
            SelfGroupCredentialVerificationError::Signature(_)
        ));
    }

    #[test]
    fn payload_with_an_unknown_tag_verifies() {
        let user_signing_key = random_user_signing_key();
        let leaf_key = leaf_key();
        let client_id = Uuid::new_v4();
        let user_id = user_signing_key
            .credential()
            .user_id()
            .tls_serialize_detached()
            .unwrap();
        let payload = encode_cbor_map(&[
            (1, Value::Bytes(client_id.as_bytes())),
            (2, Value::Bytes(&user_id)),
            (3, Value::Bytes(&leaf_key)),
            (4, Value::Text),
        ]);
        let credential =
            SelfGroupCredential::sign_payload_bytes(client_id, payload, &user_signing_key).unwrap();
        verify_with(&credential, &leaf_key, &user_signing_key).unwrap();
    }

    #[test]
    fn parsed_states_of_the_signed_part() {
        use Value::{Array, Bytes, Int, Null, Tagged, Text, Undefined};

        let client_id = Uuid::new_v4();
        let id = || Bytes(client_id.as_bytes());
        let some = || Bytes(b"some bytes");
        let empty = || Bytes(b"");

        let unsigned = [
            vec![(1, id())],
            vec![(1, id()), (2, Null)],
            vec![(1, id()), (2, Null), (3, Null)],
        ];
        for entries in unsigned {
            let credential = parse(&encode_map(&entries));
            assert_eq!(credential.client_id(), client_id);
            assert!(matches!(credential.signed_part(), SignedPart::Unsigned));
        }

        let credential = parse(&encode_map(&[(1, id()), (2, some()), (3, some())]));
        assert!(matches!(
            credential.signed_part(),
            SignedPart::Signed { .. }
        ));

        let malformed = [
            vec![(1, id()), (2, some())],
            vec![(1, id()), (3, some())],
            vec![(1, id()), (2, some()), (3, Null)],
            vec![(1, id()), (2, empty()), (3, some())],
            vec![(1, id()), (2, some()), (3, empty())],
            vec![(1, id()), (2, Text), (3, some())],
            vec![(1, id()), (2, some()), (3, Int)],
            vec![(1, id()), (2, Array), (3, some())],
            vec![(1, id()), (2, Tagged), (3, some())],
            vec![(1, id()), (2, some()), (3, Undefined)],
        ];
        for entries in malformed {
            let credential = parse(&encode_map(&entries));
            assert_eq!(credential.client_id(), client_id);
            assert!(matches!(credential.signed_part(), SignedPart::Malformed));
            assert!(!credential.is_signed());
            assert!(matches!(
                credential.unverified_payload(),
                Err(SelfGroupCredentialVerificationError::Malformed)
            ));
            assert!(matches!(
                verify_with(&credential, &leaf_key(), &random_user_signing_key()),
                Err(SelfGroupCredentialVerificationError::Malformed)
            ));
        }
    }

    #[test]
    fn the_malformed_test_credential_is_malformed() {
        let client_id = Uuid::new_v4();
        let credential = parse(&malformed_self_group_credential(client_id));
        assert_eq!(credential.client_id(), client_id);
        assert!(matches!(credential.signed_part(), SignedPart::Malformed));
    }

    #[test]
    fn frozen_decoder_reads_a_signed_credential() {
        let (credential, _, _) = signed_credential();
        let mls_credential = credential.to_credential().unwrap();
        let frozen: FrozenSelfGroupCredential =
            PersistenceCodec::from_slice(mls_credential.serialized_content()).unwrap();
        assert_eq!(frozen.client_id, credential.client_id());
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

    #[test]
    fn signed_serialization_stability() {
        let client_id = Uuid::from_u128(0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10);
        let user_id = UserId::new(
            Uuid::from_u128(0x1112_1314_1516_1718_191a_1b1c_1d1e_1f20),
            "example.com".parse().unwrap(),
        );
        let user_signing_key = create_test_credentials(user_id).1;
        let credential = SelfGroupCredential {
            signature: Some(vec![0xbb; 64]),
            ..SelfGroupCredential::signed(client_id, &[0xaa; 32], &user_signing_key).unwrap()
        };
        let bytes = PersistenceCodec::to_vec(&credential).unwrap();
        let diag = cbor_diag::parse_bytes(&bytes[1..]).unwrap().to_hex();
        insta::assert_snapshot!(diag);
    }
}
