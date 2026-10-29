// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The QS client references in the leaves of a self group and the signature
//! over them.
//!
//! A device signs the references with the QS user key. It seals the signature
//! to the QS. The DS reads the references from the leaves and forwards them
//! together with the signature. The QS then knows which client records of the
//! user belong to a device in the self group.
//!
//! The signature also covers the [`ConfirmedTranscriptHash`] of the commit. A
//! signature then cannot be replayed with another commit.
//!
//! The client and the DS share the rule that decides when a commit needs a
//! signature. See [`ClientReferencesChange`].

use std::collections::HashSet;

use mls_assist::{
    openmls::prelude::{GroupContext, LeafNode},
    openmls_traits::types::HpkeCiphertext,
};
use thiserror::Error;
use tls_codec::{Serialize as _, TlsDeserializeBytes, TlsSerialize, TlsSize, VLBytes};

use crate::{
    LibraryError,
    crypto::{
        errors::DecryptionError,
        hpke::{
            ClientIdDecryptionKey, ClientIdEncryptionKey, ClientIdKeyType, HpkeDecryptable,
            HpkeEncryptable,
        },
        signatures::{
            keys::{QsUserSignature, QsUserSigningKey, QsUserVerifyingKey, QsUserVerifyingKeyType},
            private_keys::SignatureVerificationError,
            signable::{Signable, SignedStruct, Verifiable, VerifiedStruct},
        },
    },
    identifiers::QsReference,
    mls_group_config::{EncodedClientReference, leaf_client_reference},
};

/// Sealed client references use the same QS key. A distinct HPKE info keeps
/// one from being opened as the other.
const SEALED_SIGNATURE_INFO: &[u8] = b"QS client references signature";

const LABEL: &str = "ClientReferences";

/// The client references in the leaves of a self group.
#[derive(Debug, PartialEq, Eq)]
pub enum SelfGroupReferences {
    /// A leaf has no client reference, or one that does not decode.
    Incomplete,
    Complete(HashSet<QsReference>),
}

impl SelfGroupReferences {
    pub fn from_leaves<'a>(leaves: impl IntoIterator<Item = &'a LeafNode>) -> Self {
        Self::from_encoded(leaves.into_iter().map(leaf_client_reference))
    }

    fn from_encoded(
        encoded_references: impl IntoIterator<Item = Option<EncodedClientReference>>,
    ) -> Self {
        let mut client_references = HashSet::new();
        for encoded in encoded_references {
            let Some(Ok(client_reference)) = encoded.as_ref().map(EncodedClientReference::decode)
            else {
                return Self::Incomplete;
            };
            client_references.insert(client_reference);
        }
        Self::Complete(client_references)
    }
}

/// What a commit does to the client references of a self group.
#[derive(Debug, PartialEq, Eq)]
pub enum ClientReferencesChange {
    Unchanged(SelfGroupReferences),
    Changed {
        before: SelfGroupReferences,
        after: SelfGroupReferences,
    },
}

impl ClientReferencesChange {
    pub fn between(before: SelfGroupReferences, after: SelfGroupReferences) -> Self {
        if before == after {
            Self::Unchanged(after)
        } else {
            Self::Changed { before, after }
        }
    }

    pub fn requires_signature(&self) -> bool {
        match self {
            Self::Changed {
                before: SelfGroupReferences::Complete(_),
                after: _,
            } => true,
            Self::Unchanged(_)
            | Self::Changed {
                before: SelfGroupReferences::Incomplete,
                after: _,
            } => false,
        }
    }

    /// The references after the commit that a signature covers. Returns `None`
    /// if a leaf has no reference after the commit.
    pub fn into_signed_references(self) -> Option<HashSet<QsReference>> {
        let (Self::Unchanged(after) | Self::Changed { before: _, after }) = self;
        match after {
            SelfGroupReferences::Complete(client_references) => Some(client_references),
            SelfGroupReferences::Incomplete => None,
        }
    }
}

/// The confirmed transcript hash of the epoch that a commit creates.
#[derive(Debug, Clone, PartialEq, Eq, TlsSerialize, TlsDeserializeBytes, TlsSize)]
pub struct ConfirmedTranscriptHash {
    hash: VLBytes,
}

impl ConfirmedTranscriptHash {
    pub fn of(group_context: &GroupContext) -> Self {
        Self {
            hash: group_context.confirmed_transcript_hash().into(),
        }
    }

    #[cfg(any(test, feature = "test_utils"))]
    pub fn new_for_test(hash: Vec<u8>) -> Self {
        Self { hash: hash.into() }
    }
}

/// [`ClientReferencesSignature`] sealed to the QS. Opaque to the DS.
#[derive(Debug, Clone, PartialEq, Eq, TlsSerialize, TlsDeserializeBytes, TlsSize)]
pub struct SealedClientReferencesSignature {
    ciphertext: HpkeCiphertext,
}

impl SealedClientReferencesSignature {
    /// Signs `confirmed_transcript_hash` and `client_references` with the QS
    /// user key. Seals the signature to the QS.
    pub fn seal(
        confirmed_transcript_hash: ConfirmedTranscriptHash,
        client_references: impl IntoIterator<Item = QsReference>,
        signing_key: &QsUserSigningKey,
        encryption_key: &ClientIdEncryptionKey,
    ) -> Result<Self, LibraryError> {
        let tbs = ClientReferencesTbs::new(confirmed_transcript_hash, client_references)
            .map_err(LibraryError::missing_bound_check)?;
        Ok(tbs
            .sign(signing_key)?
            .encrypt(encryption_key, SEALED_SIGNATURE_INFO, &[]))
    }

    /// Opens the signature and verifies it over `confirmed_transcript_hash` and
    /// `client_references`.
    pub fn verify(
        self,
        confirmed_transcript_hash: ConfirmedTranscriptHash,
        client_references: impl IntoIterator<Item = QsReference>,
        decryption_key: &ClientIdDecryptionKey,
        verifying_key: &QsUserVerifyingKey,
    ) -> Result<(), ClientReferencesSignatureError> {
        let ClientReferencesSignature { signature } =
            ClientReferencesSignature::decrypt(self, decryption_key, SEALED_SIGNATURE_INFO, &[])?;
        let tbs = ClientReferencesTbs::new(confirmed_transcript_hash, client_references)?;
        let VerifiedClientReferences =
            VerifiableClientReferences { tbs, signature }.verify(verifying_key)?;
        Ok(())
    }

    pub fn into_ciphertext(self) -> HpkeCiphertext {
        self.ciphertext
    }
}

impl From<HpkeCiphertext> for SealedClientReferencesSignature {
    fn from(ciphertext: HpkeCiphertext) -> Self {
        Self { ciphertext }
    }
}

impl AsRef<HpkeCiphertext> for SealedClientReferencesSignature {
    fn as_ref(&self) -> &HpkeCiphertext {
        &self.ciphertext
    }
}

#[derive(Debug, Error)]
pub enum ClientReferencesSignatureError {
    #[error("Failed to open the sealed signature: {0}")]
    Decryption(#[from] DecryptionError),
    #[error(transparent)]
    Verification(#[from] SignatureVerificationError),
    #[error("Failed to serialize the client references: {0}")]
    Serialization(#[from] tls_codec::Error),
}

#[derive(TlsSerialize, TlsDeserializeBytes, TlsSize)]
struct ClientReferencesSignature {
    signature: QsUserSignature,
}

impl SignedStruct<ClientReferencesTbs, QsUserVerifyingKeyType> for ClientReferencesSignature {
    fn from_payload(_payload: ClientReferencesTbs, signature: QsUserSignature) -> Self {
        Self { signature }
    }
}

impl HpkeEncryptable<ClientIdKeyType, SealedClientReferencesSignature>
    for ClientReferencesSignature
{
}
impl HpkeDecryptable<ClientIdKeyType, SealedClientReferencesSignature>
    for ClientReferencesSignature
{
}

/// The signed content. We sort the references by their encoding. The signature
/// then does not depend on the order of the leaves.
#[derive(Debug, TlsSerialize, TlsSize)]
struct ClientReferencesTbs {
    confirmed_transcript_hash: ConfirmedTranscriptHash,
    client_references: Vec<QsReference>,
}

impl ClientReferencesTbs {
    fn new(
        confirmed_transcript_hash: ConfirmedTranscriptHash,
        client_references: impl IntoIterator<Item = QsReference>,
    ) -> Result<Self, tls_codec::Error> {
        let mut encoded = Vec::new();
        for client_reference in client_references {
            encoded.push((client_reference.tls_serialize_detached()?, client_reference));
        }
        encoded.sort_by(|(a, _), (b, _)| a.cmp(b));
        let client_references = encoded
            .into_iter()
            .map(|(_, client_reference)| client_reference)
            .collect();
        Ok(Self {
            confirmed_transcript_hash,
            client_references,
        })
    }
}

impl Signable for ClientReferencesTbs {
    type SignedOutput = ClientReferencesSignature;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        self.tls_serialize_detached()
    }

    fn label(&self) -> &str {
        LABEL
    }
}

#[derive(Debug)]
struct VerifiableClientReferences {
    tbs: ClientReferencesTbs,
    signature: QsUserSignature,
}

impl Verifiable for VerifiableClientReferences {
    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        self.tbs.tls_serialize_detached()
    }

    fn signature(&self) -> impl AsRef<[u8]> {
        &self.signature
    }

    fn label(&self) -> &str {
        LABEL
    }
}

struct VerifiedClientReferences;

mod private_mod {
    #[derive(Default)]
    pub struct Seal;
}

impl VerifiedStruct<VerifiableClientReferences> for VerifiedClientReferences {
    type SealingType = private_mod::Seal;

    fn from_verifiable(_verifiable: VerifiableClientReferences, _seal: Self::SealingType) -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        crypto::{hpke::HpkeEncryptable, signatures::signable::SignContent},
        identifiers::{ClientConfig, QsClientId, SealedClientReference},
    };

    use super::*;

    fn client_reference(encryption_key: &ClientIdEncryptionKey) -> QsReference {
        let sealed_reference = ClientConfig {
            client_id: QsClientId::random(&mut rand::rng()),
            push_token_ear_key: None,
        }
        .encrypt(encryption_key, &[], &[]);
        QsReference {
            client_homeserver_domain: "example.com".parse().unwrap(),
            sealed_reference,
        }
    }

    struct Setup {
        decryption_key: ClientIdDecryptionKey,
        signing_key: QsUserSigningKey,
        client_references: Vec<QsReference>,
    }

    fn setup() -> Setup {
        let decryption_key = ClientIdDecryptionKey::generate().unwrap();
        let encryption_key = decryption_key.encryption_key();
        let client_references = vec![
            client_reference(encryption_key),
            client_reference(encryption_key),
            client_reference(encryption_key),
        ];
        Setup {
            decryption_key,
            signing_key: QsUserSigningKey::generate().unwrap(),
            client_references,
        }
    }

    fn confirmed_transcript_hash() -> ConfirmedTranscriptHash {
        ConfirmedTranscriptHash::new_for_test(vec![0x0c; 32])
    }

    fn seal(setup: &Setup) -> SealedClientReferencesSignature {
        SealedClientReferencesSignature::seal(
            confirmed_transcript_hash(),
            setup.client_references.clone(),
            &setup.signing_key,
            setup.decryption_key.encryption_key(),
        )
        .unwrap()
    }

    fn client_references<const N: usize>() -> [QsReference; N] {
        let decryption_key = ClientIdDecryptionKey::generate().unwrap();
        std::array::from_fn(|_| client_reference(decryption_key.encryption_key()))
    }

    fn set(client_references: &[&QsReference]) -> HashSet<QsReference> {
        let mut set = HashSet::new();
        for client_reference in client_references {
            set.insert((*client_reference).clone());
        }
        set
    }

    fn encoded(client_reference: &QsReference) -> Option<EncodedClientReference> {
        Some(client_reference.tls_serialize_detached().unwrap().into())
    }

    fn complete(client_references: &[&QsReference]) -> SelfGroupReferences {
        SelfGroupReferences::Complete(set(client_references))
    }

    #[test]
    fn every_leaf_needs_a_valid_reference() {
        let [a, b] = &client_references();
        let from_encoded = SelfGroupReferences::from_encoded;

        assert_eq!(from_encoded([encoded(a), encoded(b)]), complete(&[a, b]));
        assert_eq!(
            from_encoded([encoded(a), None]),
            SelfGroupReferences::Incomplete
        );
        let invalid = Some(EncodedClientReference::from(vec![0xff]));
        assert_eq!(
            from_encoded([encoded(a), invalid]),
            SelfGroupReferences::Incomplete
        );
        assert_eq!(from_encoded([encoded(a), encoded(a)]), complete(&[a]));
    }

    #[test]
    fn order_of_leaves_does_not_change_references() {
        let [a, b] = &client_references();
        let before = SelfGroupReferences::from_encoded([encoded(a), encoded(b)]);
        let after = SelfGroupReferences::from_encoded([encoded(b), encoded(a)]);
        assert_eq!(
            ClientReferencesChange::between(before, after),
            ClientReferencesChange::Unchanged(complete(&[a, b]))
        );
    }

    #[test]
    fn changed_complete_references_require_a_signature() {
        let [a, b] = &client_references();

        let added = ClientReferencesChange::between(complete(&[a]), complete(&[a, b]));
        assert!(added.requires_signature());
        assert_eq!(added.into_signed_references(), Some(set(&[a, b])));

        let removed = ClientReferencesChange::between(complete(&[a, b]), complete(&[b]));
        assert!(removed.requires_signature());
        assert_eq!(removed.into_signed_references(), Some(set(&[b])));

        let dropped =
            ClientReferencesChange::between(complete(&[a, b]), SelfGroupReferences::Incomplete);
        assert!(dropped.requires_signature());
        assert_eq!(dropped.into_signed_references(), None);
    }

    #[test]
    fn unchanged_references_take_an_optional_signature() {
        let [a, b] = &client_references();

        let unchanged = ClientReferencesChange::between(complete(&[a, b]), complete(&[a, b]));
        assert!(!unchanged.requires_signature());
        assert_eq!(unchanged.into_signed_references(), Some(set(&[a, b])));
    }

    #[test]
    fn incomplete_references_are_not_checked() {
        use SelfGroupReferences::Incomplete;
        let [a, b] = &client_references();

        let completed = ClientReferencesChange::between(Incomplete, complete(&[a, b]));
        assert!(!completed.requires_signature());
        assert_eq!(completed.into_signed_references(), Some(set(&[a, b])));

        let incomplete = ClientReferencesChange::between(Incomplete, Incomplete);
        assert!(!incomplete.requires_signature());
        assert_eq!(incomplete.into_signed_references(), None);
    }

    fn fixed_client_reference(byte: u8) -> QsReference {
        QsReference {
            client_homeserver_domain: "example.com".parse().unwrap(),
            sealed_reference: SealedClientReference::from(HpkeCiphertext {
                kem_output: vec![byte; 4].into(),
                ciphertext: vec![byte; 8].into(),
            }),
        }
    }

    fn signed_content(client_references: Vec<QsReference>) -> Vec<u8> {
        let tbs = ClientReferencesTbs::new(confirmed_transcript_hash(), client_references).unwrap();
        let payload = tbs.unsigned_payload().unwrap();
        SignContent::from((tbs.label(), payload.as_slice()))
            .tls_serialize_detached()
            .unwrap()
    }

    #[test]
    fn signed_content_stability() {
        let a = fixed_client_reference(0x01);
        let b = fixed_client_reference(0x02);
        let bytes = signed_content(vec![b.clone(), a.clone()]);
        assert_eq!(bytes, signed_content(vec![a, b]));
        insta::assert_binary_snapshot!("client_references_signed_content.tls", bytes);
    }

    #[test]
    fn verifies_in_any_order() {
        let setup = setup();
        let sealed = seal(&setup);
        let mut client_references = setup.client_references.clone();
        client_references.reverse();
        sealed
            .verify(
                confirmed_transcript_hash(),
                client_references,
                &setup.decryption_key,
                setup.signing_key.verifying_key(),
            )
            .unwrap();
    }

    #[test]
    fn rejects_other_references() {
        let setup = setup();
        let sealed = seal(&setup);
        let mut client_references = setup.client_references.clone();
        client_references.pop();
        let error = sealed
            .verify(
                confirmed_transcript_hash(),
                client_references,
                &setup.decryption_key,
                setup.signing_key.verifying_key(),
            )
            .unwrap_err();
        assert!(matches!(
            error,
            ClientReferencesSignatureError::Verification(_)
        ));
    }

    #[test]
    fn rejects_other_confirmed_transcript_hash() {
        let setup = setup();
        let sealed = seal(&setup);
        let error = sealed
            .verify(
                ConfirmedTranscriptHash::new_for_test(vec![0x0d; 32]),
                setup.client_references.clone(),
                &setup.decryption_key,
                setup.signing_key.verifying_key(),
            )
            .unwrap_err();
        assert!(matches!(
            error,
            ClientReferencesSignatureError::Verification(_)
        ));
    }

    #[test]
    fn rejects_other_user_key() {
        let setup = setup();
        let sealed = seal(&setup);
        let other_signing_key = QsUserSigningKey::generate().unwrap();
        let error = sealed
            .verify(
                confirmed_transcript_hash(),
                setup.client_references.clone(),
                &setup.decryption_key,
                other_signing_key.verifying_key(),
            )
            .unwrap_err();
        assert!(matches!(
            error,
            ClientReferencesSignatureError::Verification(_)
        ));
    }

    #[test]
    fn sealed_client_reference_is_not_a_signature() {
        let setup = setup();
        let sealed_reference = setup.client_references[0].sealed_reference.clone();
        let sealed = SealedClientReferencesSignature::from(sealed_reference.into_ciphertext());
        let error = sealed
            .verify(
                confirmed_transcript_hash(),
                setup.client_references.clone(),
                &setup.decryption_key,
                setup.signing_key.verifying_key(),
            )
            .unwrap_err();
        assert!(matches!(
            error,
            ClientReferencesSignatureError::Decryption(_)
        ));
    }
}
