// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Signature over the QS client references in the leaves of a self group.
//!
//! A device signs the references with the QS user key. It seals the signature
//! to the QS. The DS reads the references from the leaves and forwards them
//! together with the signature. The QS then knows which client records of the
//! user belong to a device in the self group.

use mls_assist::openmls_traits::types::HpkeCiphertext;
use thiserror::Error;
use tls_codec::{Serialize as _, TlsDeserializeBytes, TlsSerialize, TlsSize};

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
};

/// Sealed client references use the same QS key. A distinct HPKE info keeps
/// one from being opened as the other.
const SEALED_SIGNATURE_INFO: &[u8] = b"QS client references signature";

const LABEL: &str = "ClientReferences";

/// [`ClientReferencesSignature`] sealed to the QS. Opaque to the DS.
#[derive(Debug, Clone, PartialEq, Eq, TlsSerialize, TlsDeserializeBytes, TlsSize)]
pub struct SealedClientReferencesSignature {
    ciphertext: HpkeCiphertext,
}

impl SealedClientReferencesSignature {
    /// Signs `client_references` with the QS user key and seals the signature
    /// to the QS.
    pub fn seal(
        client_references: Vec<QsReference>,
        signing_key: &QsUserSigningKey,
        encryption_key: &ClientIdEncryptionKey,
    ) -> Result<Self, LibraryError> {
        let tbs = ClientReferencesTbs::new(client_references)
            .map_err(LibraryError::missing_bound_check)?;
        Ok(tbs
            .sign(signing_key)?
            .encrypt(encryption_key, SEALED_SIGNATURE_INFO, &[]))
    }

    /// Opens the signature and verifies it over `client_references`.
    pub fn verify(
        self,
        client_references: Vec<QsReference>,
        decryption_key: &ClientIdDecryptionKey,
        verifying_key: &QsUserVerifyingKey,
    ) -> Result<(), ClientReferencesSignatureError> {
        let ClientReferencesSignature { signature } =
            ClientReferencesSignature::decrypt(self, decryption_key, SEALED_SIGNATURE_INFO, &[])?;
        let tbs = ClientReferencesTbs::new(client_references)?;
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
    client_references: Vec<QsReference>,
}

impl ClientReferencesTbs {
    fn new(client_references: Vec<QsReference>) -> Result<Self, tls_codec::Error> {
        let mut encoded = Vec::with_capacity(client_references.len());
        for client_reference in client_references {
            encoded.push((client_reference.tls_serialize_detached()?, client_reference));
        }
        encoded.sort_by(|(a, _), (b, _)| a.cmp(b));
        let client_references = encoded
            .into_iter()
            .map(|(_, client_reference)| client_reference)
            .collect();
        Ok(Self { client_references })
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
        crypto::hpke::HpkeEncryptable,
        identifiers::{ClientConfig, QsClientId},
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

    fn seal(setup: &Setup) -> SealedClientReferencesSignature {
        SealedClientReferencesSignature::seal(
            setup.client_references.clone(),
            &setup.signing_key,
            setup.decryption_key.encryption_key(),
        )
        .unwrap()
    }

    #[test]
    fn verifies_in_any_order() {
        let setup = setup();
        let sealed = seal(&setup);
        let mut client_references = setup.client_references.clone();
        client_references.reverse();
        sealed
            .verify(
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
    fn rejects_other_user_key() {
        let setup = setup();
        let sealed = seal(&setup);
        let other_signing_key = QsUserSigningKey::generate().unwrap();
        let error = sealed
            .verify(
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
