// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use chrono::Duration;
use mls_assist::openmls::prelude::{
    BasicCredential, BasicCredentialError, Credential, SignatureScheme,
};

use serde::{Deserialize, Serialize};
use sqlx::{Database, Decode, Encode, Sqlite, Type, encode::IsNull, error::BoxDynError};
use tls_codec::{
    DeserializeBytes, Serialize as TlsSerialize, TlsDeserializeBytes, TlsSerialize, TlsSize,
};

use keys::{
    AsIntermediateKeyType, AsIntermediateSignature, AsIntermediateVerifyingKey, AsKeyType,
    AsSignature, AsSigningKey, AsVerifyingKey, PreliminaryAsIntermediateSigningKey,
    PreliminaryClientSigningKey,
};

use crate::{
    LibraryError,
    codec::PersistenceCodec,
    crypto::{
        Labeled,
        ear::Ciphertext,
        errors::KeyGenerationError,
        hash::{Hash, Hashable},
        signatures::{
            private_keys::SigningKey,
            signable::{Signable, Signature, SignedStruct, Verifiable, VerifiedStruct},
        },
    },
    identifiers::{Fqdn, UserId},
    messages::AirProtocolVersion,
    time::ExpirationData,
};

mod private_mod {
    #[derive(Default)]
    pub struct Seal;
}

pub mod keys;

use self::keys::ClientVerifyingKey;

const DEFAULT_AS_CREDENTIAL_LIFETIME: Duration = Duration::days(5 * 365);
const AS_CREDENTIAL_LABEL: &str = "Air AS Credential";

#[derive(Debug, TlsDeserializeBytes, TlsSerialize, TlsSize, Clone, Serialize, Deserialize)]
pub struct AsCredential {
    body: AsCredentialBody,
    fingerprint: Hash<AsCredentialBody>,
}

impl From<AsCredentialBody> for AsCredential {
    fn from(body: AsCredentialBody) -> Self {
        let fingerprint = body.hash();
        Self { body, fingerprint }
    }
}

impl Labeled for AsCredentialBody {
    const LABEL: &'static str = AS_CREDENTIAL_LABEL;
}

impl Hashable for AsCredentialBody {}

#[derive(Debug, TlsDeserializeBytes, TlsSerialize, TlsSize, Clone, Serialize, Deserialize)]
pub struct AsCredentialBody {
    pub version: AirProtocolVersion,
    pub user_domain: Fqdn,
    pub expiration_data: ExpirationData,
    pub signature_scheme: SignatureScheme,
    pub verifying_key: AsVerifyingKey,
}

impl Type<Sqlite> for AsCredentialBody {
    fn type_info() -> <Sqlite as Database>::TypeInfo {
        <Vec<u8> as Type<Sqlite>>::type_info()
    }
}

impl Encode<'_, Sqlite> for AsCredentialBody {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as Database>::ArgumentBuffer<'_>,
    ) -> Result<IsNull, BoxDynError> {
        let bytes = PersistenceCodec::to_vec(self)?;
        Encode::<Sqlite>::encode(bytes, buf)
    }
}

impl AsCredential {
    /// Generate a new [`AsCredential`] with the given data and a freshly
    /// generated signature keypair.
    ///
    /// The default [`ExpirationData`] for an [`AsCredential`] is five years.
    pub fn new(
        signature_scheme: SignatureScheme,
        user_domain: Fqdn,
        expiration_data_option: Option<ExpirationData>,
    ) -> Result<(Self, AsSigningKey), KeyGenerationError> {
        let version = AirProtocolVersion::default();
        // Create lifetime valid until 5 years in the future.
        let expiration_data = expiration_data_option
            .unwrap_or_else(|| ExpirationData::new(DEFAULT_AS_CREDENTIAL_LIFETIME));
        let signing_key = SigningKey::generate()?;
        let verifying_key = signing_key.verifying_key().clone();
        let body = AsCredentialBody {
            version,
            user_domain,
            expiration_data,
            signature_scheme,
            verifying_key,
        };
        let fingerprint = body.hash();
        let credential = Self { body, fingerprint };
        let signing_key =
            AsSigningKey::from_private_key_and_credential(signing_key, credential.clone());
        Ok((credential, signing_key))
    }

    pub fn from_parts(body: AsCredentialBody, fingerprint: Hash<AsCredentialBody>) -> Self {
        Self { body, fingerprint }
    }

    pub fn into_parts(self) -> (AsCredentialBody, Hash<AsCredentialBody>) {
        (self.body, self.fingerprint)
    }

    pub fn fingerprint(&self) -> &Hash<AsCredentialBody> {
        &self.fingerprint
    }

    pub fn verifying_key(&self) -> &AsVerifyingKey {
        &self.body.verifying_key
    }

    pub fn domain(&self) -> &Fqdn {
        &self.body.user_domain
    }

    pub fn body(&self) -> &AsCredentialBody {
        &self.body
    }
}

const DEFAULT_AS_INTERMEDIATE_CREDENTIAL_LIFETIME: Duration = Duration::days(365);

#[derive(Debug, Clone, TlsDeserializeBytes, TlsSerialize, TlsSize, Serialize, Deserialize)]
pub struct AsIntermediateCredentialCsr {
    pub version: AirProtocolVersion,
    pub user_domain: Fqdn,
    pub signature_scheme: SignatureScheme,
    pub verifying_key: AsIntermediateVerifyingKey, // PK used to sign client credentials
}

impl AsIntermediateCredentialCsr {
    /// Generate a new [`AsIntermediateCredentialCsr`] with the given data and a freshly
    /// generated signature keypair.
    ///
    /// Returns the CSR and a preliminary signing key. The preliminary signing
    /// key can be turned into a [`keys::AsIntermediateSigningKey`] once the CSR is
    /// signed.
    pub fn new(
        signature_scheme: SignatureScheme,
        user_domain: Fqdn,
    ) -> Result<(Self, PreliminaryAsIntermediateSigningKey), KeyGenerationError> {
        let version = AirProtocolVersion::default();
        let prelim_signing_key = PreliminaryAsIntermediateSigningKey::generate()?;
        let credential = Self {
            version,
            signature_scheme,
            verifying_key: prelim_signing_key.verifying_key().clone().convert(),
            user_domain,
        };
        Ok((credential, prelim_signing_key))
    }

    /// Sign the CSR with the given signing key to obtain an
    /// [`AsIntermediateCredential`] with the given expiration data.
    ///
    /// If no expiration data is given, the default [`ExpirationData`] of one
    /// year is set.
    pub fn sign(
        self,
        as_signing_key: &AsSigningKey,
        expiration_data_option: Option<ExpirationData>,
    ) -> Result<AsIntermediateCredential, LibraryError> {
        // Create lifetime valid until 5 years in the future.
        let expiration_data = expiration_data_option.unwrap_or(ExpirationData::new(
            DEFAULT_AS_INTERMEDIATE_CREDENTIAL_LIFETIME,
        ));
        let signer_fingerprint = *as_signing_key.credential().fingerprint();
        let credential = AsIntermediateCredentialPayload {
            csr: self,
            expiration_data,
            signer_fingerprint,
        };
        credential.sign(as_signing_key)
    }
}

#[derive(Debug, Clone, TlsDeserializeBytes, TlsSerialize, TlsSize, Serialize, Deserialize)]
pub struct AsIntermediateCredentialPayload {
    pub csr: AsIntermediateCredentialCsr,
    pub expiration_data: ExpirationData,
    pub signer_fingerprint: Hash<AsCredentialBody>, // fingerprint of the signing AsCredential
}

const AS_INTERMEDIATE_CREDENTIAL_LABEL: &str = "Air AS Intermediate Credential";

impl Labeled for AsIntermediateCredentialBody {
    const LABEL: &'static str = AS_INTERMEDIATE_CREDENTIAL_LABEL;
}

impl Hashable for AsIntermediateCredentialBody {}

impl Signable for AsIntermediateCredentialPayload {
    type SignedOutput = AsIntermediateCredential;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        self.tls_serialize_detached()
    }

    fn label(&self) -> &str {
        AS_INTERMEDIATE_CREDENTIAL_LABEL
    }
}

#[derive(Debug, Clone, TlsSerialize, TlsSize, Serialize, Deserialize)]
pub struct AsIntermediateCredentialBody {
    credential: AsIntermediateCredentialPayload,
    signature: Signature<AsKeyType>,
}

impl Type<Sqlite> for AsIntermediateCredentialBody {
    fn type_info() -> <Sqlite as Database>::TypeInfo {
        <Vec<u8> as Type<Sqlite>>::type_info()
    }
}

impl Encode<'_, Sqlite> for AsIntermediateCredentialBody {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as Database>::ArgumentBuffer<'_>,
    ) -> Result<IsNull, BoxDynError> {
        let bytes = PersistenceCodec::to_vec(self)?;
        Encode::<Sqlite>::encode(bytes, buf)
    }
}

impl Decode<'_, Sqlite> for AsIntermediateCredentialBody {
    fn decode(value: <Sqlite as Database>::ValueRef<'_>) -> Result<Self, BoxDynError> {
        let bytes: &[u8] = Decode::<Sqlite>::decode(value)?;
        Ok(PersistenceCodec::from_slice(bytes)?)
    }
}

impl AsIntermediateCredentialBody {
    pub fn into_parts(self) -> (AsIntermediateCredentialPayload, Signature<AsKeyType>) {
        (self.credential, self.signature)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsIntermediateCredential {
    body: AsIntermediateCredentialBody,
    fingerprint: Hash<AsIntermediateCredentialBody>,
}

impl From<AsIntermediateCredentialBody> for AsIntermediateCredential {
    fn from(body: AsIntermediateCredentialBody) -> Self {
        let fingerprint = body.hash();
        Self { body, fingerprint }
    }
}

impl tls_codec::Serialize for AsIntermediateCredential {
    fn tls_serialize<W: std::io::Write>(&self, writer: &mut W) -> Result<usize, tls_codec::Error> {
        self.body.tls_serialize(writer)
    }
}

impl tls_codec::Size for AsIntermediateCredential {
    fn tls_serialized_len(&self) -> usize {
        self.body.tls_serialized_len()
    }
}

impl AsIntermediateCredential {
    pub fn into_parts(
        self,
    ) -> (
        AsIntermediateCredentialBody,
        Hash<AsIntermediateCredentialBody>,
    ) {
        (self.body, self.fingerprint)
    }

    pub fn verifying_key(&self) -> &AsIntermediateVerifyingKey {
        &self.body.credential.csr.verifying_key
    }

    pub fn fingerprint(&self) -> &Hash<AsIntermediateCredentialBody> {
        &self.fingerprint
    }

    pub fn domain(&self) -> &Fqdn {
        &self.body.credential.csr.user_domain
    }

    pub fn body(&self) -> &AsIntermediateCredentialBody {
        &self.body
    }
}

impl SignedStruct<AsIntermediateCredentialPayload, AsKeyType> for AsIntermediateCredential {
    fn from_payload(payload: AsIntermediateCredentialPayload, signature: AsSignature) -> Self {
        let body = AsIntermediateCredentialBody {
            credential: payload,
            signature,
        };
        let fingerprint = body.hash();
        Self { body, fingerprint }
    }
}

#[derive(Debug, TlsDeserializeBytes, TlsSerialize, TlsSize)]
pub struct VerifiableAsIntermediateCredential {
    credential: AsIntermediateCredentialPayload,
    signature: AsSignature,
}

impl VerifiableAsIntermediateCredential {
    pub fn from_parts(credential: AsIntermediateCredentialPayload, signature: AsSignature) -> Self {
        Self {
            credential,
            signature,
        }
    }

    pub fn signer_fingerprint(&self) -> &Hash<AsCredentialBody> {
        &self.credential.signer_fingerprint
    }
}

impl Verifiable for VerifiableAsIntermediateCredential {
    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        self.credential.tls_serialize_detached()
    }

    fn signature(&self) -> impl AsRef<[u8]> {
        &self.signature
    }

    fn label(&self) -> &str {
        AS_INTERMEDIATE_CREDENTIAL_LABEL
    }
}

impl VerifiedStruct<VerifiableAsIntermediateCredential> for AsIntermediateCredential {
    type SealingType = private_mod::Seal;

    fn from_verifiable(
        verifiable: VerifiableAsIntermediateCredential,
        _seal: Self::SealingType,
    ) -> Self {
        Self::from_payload(verifiable.credential, verifiable.signature)
    }
}

const CLIENT_CREDENTIAL_LABEL: &str = "Client Credential";
const DEFAULT_CLIENT_CREDENTIAL_LIFETIME: Duration = Duration::days(90);

// WARNING: If this type is changed, a new variant of the
// VersionedClientCredential(Ref) must be created and the `FromSql` and `ToSql`
// implementations of `ClientCredential` must be updated accordingly.
#[derive(
    Debug, Clone, PartialEq, Eq, TlsDeserializeBytes, TlsSerialize, TlsSize, Serialize, Deserialize,
)]
pub struct ClientCredentialCsr {
    pub version: AirProtocolVersion,
    pub user_id: UserId,
    pub signature_scheme: SignatureScheme,
    pub verifying_key: ClientVerifyingKey,
}

impl ClientCredentialCsr {
    /// Generate a new [`ClientCredentialCsr`] with the given data and a freshly
    /// generated signature keypair.
    ///
    /// Returns the CSR and a preliminary signing key. The preliminary signing
    /// key can be turned into a [`keys::AsIntermediateSigningKey`] once the CSR is
    /// signed.
    pub fn new(
        user_id: UserId,
        signature_scheme: SignatureScheme,
    ) -> Result<(Self, PreliminaryClientSigningKey), KeyGenerationError> {
        let version = AirProtocolVersion::default();
        let prelim_signing_key = PreliminaryClientSigningKey::generate()?;
        let credential = Self {
            version,
            signature_scheme,
            verifying_key: prelim_signing_key.verifying_key().clone().convert(),
            user_id,
        };
        Ok((credential, prelim_signing_key))
    }
}

// WARNING: If this type is changed, a new variant of the
// VersionedClientCredential(Ref) must be created and the `FromSql` and `ToSql`
// implementations of `ClientCredential` must be updated accordingly.
#[derive(
    Debug, Clone, PartialEq, Eq, TlsDeserializeBytes, TlsSerialize, TlsSize, Serialize, Deserialize,
)]
pub struct ClientCredentialPayload {
    pub csr: ClientCredentialCsr,
    pub expiration_data: ExpirationData,
    pub signer_fingerprint: Hash<AsIntermediateCredentialBody>,
}

impl ClientCredentialPayload {
    pub fn new(
        csr: ClientCredentialCsr,
        expiration_data_option: Option<ExpirationData>,
        signer_fingerprint: Hash<AsIntermediateCredentialBody>,
    ) -> Self {
        let expiration_data = expiration_data_option
            .unwrap_or(ExpirationData::new(DEFAULT_CLIENT_CREDENTIAL_LIFETIME));
        Self {
            csr,
            expiration_data,
            signer_fingerprint,
        }
    }

    pub fn expiration_data(&self) -> &ExpirationData {
        &self.expiration_data
    }

    pub fn validate(&self) -> bool {
        // TODO: Check uniqueness of client id
        self.expiration_data.validate()
    }
}

impl Signable for ClientCredentialPayload {
    type SignedOutput = ClientCredential;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        self.tls_serialize_detached()
    }

    fn label(&self) -> &str {
        CLIENT_CREDENTIAL_LABEL
    }
}

impl ClientCredentialPayload {
    pub fn identity(&self) -> &UserId {
        &self.csr.user_id
    }
}

impl Labeled for ClientCredential {
    const LABEL: &'static str = CLIENT_CREDENTIAL_LABEL;
}

impl Hashable for ClientCredential {}

// WARNING: If this type is changed, a new variant of the
// VersionedClientCredential(Ref) must be created and the `FromSql` and `ToSql`
// implementations of `ClientCredential` must be updated accordingly.
#[derive(
    Debug, Clone, PartialEq, Eq, TlsSerialize, TlsDeserializeBytes, TlsSize, Serialize, Deserialize,
)]
pub struct ClientCredential {
    payload: ClientCredentialPayload,
    signature: AsIntermediateSignature,
}

impl ClientCredential {
    pub fn new(payload: ClientCredentialPayload, signature: AsIntermediateSignature) -> Self {
        Self { payload, signature }
    }

    pub fn signature_scheme(&self) -> SignatureScheme {
        self.payload.csr.signature_scheme
    }

    pub fn into_parts(self) -> (ClientCredentialPayload, AsIntermediateSignature) {
        (self.payload, self.signature)
    }

    pub fn identity(&self) -> &UserId {
        self.payload.identity()
    }

    pub fn verifying_key(&self) -> &ClientVerifyingKey {
        &self.payload.csr.verifying_key
    }

    pub fn fingerprint(&self) -> Hash<Self> {
        self.hash()
    }

    pub fn signer_fingerprint(&self) -> &Hash<AsIntermediateCredentialBody> {
        &self.payload.signer_fingerprint
    }
}

// When adding a variant to this enum, the new variant must be called
// `CurrentVersion` and the current version must be renamed to `VX`, where `X`
// is the next version number. The content type of the old `CurrentVersion` must
// be renamed and otherwise preserved to ensure backwards compatibility.
#[derive(Serialize, Deserialize)]
enum VersionedClientCredential {
    CurrentVersion(ClientCredential),
}

// Only change this enum in tandem with its non-Ref variant.
#[derive(Serialize)]
enum VersionedClientCredentialRef<'a> {
    CurrentVersion(&'a ClientCredential),
}

impl Type<Sqlite> for ClientCredential {
    fn type_info() -> <Sqlite as Database>::TypeInfo {
        <Vec<u8> as Type<Sqlite>>::type_info()
    }
}

impl<'q> Encode<'q, Sqlite> for ClientCredential {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        let versioned = VersionedClientCredentialRef::CurrentVersion(self);
        let bytes = PersistenceCodec::to_vec(&versioned)?;
        Encode::<Sqlite>::encode(bytes, buf)
    }
}

impl<'r> Decode<'r, Sqlite> for ClientCredential {
    fn decode(value: <Sqlite as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let bytes: &[u8] = Decode::<Sqlite>::decode(value)?;
        match PersistenceCodec::from_slice(bytes)? {
            VersionedClientCredential::CurrentVersion(credential) => Ok(credential),
        }
    }
}

impl VerifiedStruct<VerifiableClientCredential> for ClientCredential {
    type SealingType = private_mod::Seal;

    fn from_verifiable(verifiable: VerifiableClientCredential, _seal: Self::SealingType) -> Self {
        Self {
            payload: verifiable.payload,
            signature: verifiable.signature,
        }
    }
}

impl SignedStruct<ClientCredentialPayload, AsIntermediateKeyType> for ClientCredential {
    fn from_payload(payload: ClientCredentialPayload, signature: AsIntermediateSignature) -> Self {
        Self { payload, signature }
    }
}

#[derive(
    Debug, TlsDeserializeBytes, TlsSerialize, TlsSize, Clone, Serialize, Deserialize, PartialEq,
)]
pub struct VerifiableClientCredential {
    payload: ClientCredentialPayload,
    signature: AsIntermediateSignature,
}

impl VerifiableClientCredential {
    pub fn new(payload: ClientCredentialPayload, signature: AsIntermediateSignature) -> Self {
        Self { payload, signature }
    }

    pub fn domain(&self) -> &Fqdn {
        self.payload.csr.user_id.domain()
    }

    pub fn signer_fingerprint(&self) -> &Hash<AsIntermediateCredentialBody> {
        &self.payload.signer_fingerprint
    }

    pub fn user_id(&self) -> &UserId {
        &self.payload.csr.user_id
    }
}

impl Verifiable for VerifiableClientCredential {
    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        self.payload.tls_serialize_detached()
    }

    fn signature(&self) -> impl AsRef<[u8]> {
        &self.signature
    }

    fn label(&self) -> &str {
        CLIENT_CREDENTIAL_LABEL
    }
}

impl TryFrom<Credential> for VerifiableClientCredential {
    type Error = BasicCredentialError;

    fn try_from(value: Credential) -> Result<Self, Self::Error> {
        let basic_credential = BasicCredential::try_from(value)?;
        let credential =
            VerifiableClientCredential::tls_deserialize_exact_bytes(basic_credential.identity())?;
        Ok(credential)
    }
}

#[derive(Debug)]
pub struct EncryptedClientCredentialCtype;
pub type EncryptedClientCredential = Ciphertext<EncryptedClientCredentialCtype>;

pub mod persistence {
    use crate::{codec::PersistenceCodec, identifiers::UserId, time::ExpirationData};

    use super::{
        AsIntermediateCredentialBody, ClientCredential, ClientCredentialCsr,
        ClientCredentialPayload, Hash,
        keys::{AsIntermediateSignature, ClientVerifyingKey},
    };

    #[derive(Debug, sqlx::Type)]
    #[sqlx(type_name = "client_credential")]
    pub struct FlatClientCredential {
        version: Vec<u8>,
        signature_scheme: Vec<u8>,
        verifying_key: ClientVerifyingKey,
        expiration_data: ExpirationData,
        signer_fingerprint: Hash<AsIntermediateCredentialBody>,
        signature: AsIntermediateSignature,
    }

    impl FlatClientCredential {
        pub fn new(credential: &ClientCredential) -> Self {
            Self {
                version: PersistenceCodec::to_vec(&credential.payload.csr.version).unwrap(),
                signature_scheme: PersistenceCodec::to_vec(
                    &credential.payload.csr.signature_scheme,
                )
                .unwrap(),
                verifying_key: credential.payload.csr.verifying_key.clone(),
                expiration_data: credential.payload.expiration_data.clone(),
                signer_fingerprint: credential.payload.signer_fingerprint,
                signature: credential.signature.clone(),
            }
        }

        pub fn into_client_credential(self, user_id: UserId) -> ClientCredential {
            let payload = ClientCredentialPayload {
                csr: ClientCredentialCsr {
                    version: PersistenceCodec::from_slice(&self.version).unwrap(),
                    user_id,
                    signature_scheme: PersistenceCodec::from_slice(&self.signature_scheme).unwrap(),
                    verifying_key: self.verifying_key,
                },
                expiration_data: self.expiration_data,
                signer_fingerprint: self.signer_fingerprint,
            };
            let signature = self.signature;
            ClientCredential { payload, signature }
        }
    }
}

#[cfg(feature = "test_utils")]
pub mod test_utils {
    use super::{
        keys::{AsIntermediateSigningKey, ClientSigningKey},
        *,
    };

    pub fn create_test_credentials(
        user_id: UserId,
    ) -> (AsIntermediateSigningKey, ClientSigningKey) {
        let (credential_csr, signing_key) =
            ClientCredentialCsr::new(user_id.clone(), SignatureScheme::ED25519).unwrap();
        let domain = user_id.domain().clone();
        let (_as_credential, ac_sk) =
            AsCredential::new(SignatureScheme::ED25519, domain.clone(), None).unwrap();
        let (as_intermediate_credential_csr, aic_sk) =
            AsIntermediateCredentialCsr::new(SignatureScheme::ED25519, domain.clone()).unwrap();
        let as_intermediate_credential = as_intermediate_credential_csr.sign(&ac_sk, None).unwrap();
        let aic_sk =
            AsIntermediateSigningKey::from_prelim_key(aic_sk, as_intermediate_credential).unwrap();
        let client_credential =
            ClientCredentialPayload::new(credential_csr, None, *aic_sk.credential().fingerprint())
                .sign(&aic_sk)
                .unwrap();
        let client_sk = ClientSigningKey::from_prelim_key(signing_key, client_credential).unwrap();
        (aic_sk, client_sk)
    }
}
