// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tls_codec::{Serialize as _, TlsSerialize, TlsSize};

use crate::{
    LibraryError,
    credentials::keys::{HandleSignature, HandleSigningKey},
    crypto::{
        ConnectionDecryptionKey, ConnectionEncryptionKey, Labeled,
        errors::RandomnessError,
        hash::{Hash, Hashable},
        signatures::{private_keys::SignatureVerificationError, signable::Signable},
    },
    identifiers::UserHandleHash,
    messages::{
        AirProtocolVersion,
        connection_package::payload::TlsBool,
        connection_package_v1::{
            CONNECTION_PACKAGE_EXPIRATION, ConnectionPackageV1, ConnectionPackageV1In,
        },
    },
    time::{ExpirationData, TimeStamp},
};

pub use payload::{ConnectionPackageIn, ConnectionPackagePayload};

const LABEL: &str = "ConnectionPackage";

/// This enum holds different versions of `ConnectionPackage`. It exists to
/// allow separate processing (especially signature verification) of the
/// ProtoBuf ConnectionPackage. For example, the ProtoBuf version gained an
/// extra field that indicates whether the connection is a last resort. This
/// field is not present in the original version of ConnectionPackage and the
/// presence of the signature means we cannot just change the struct without
/// breaking backwards compatibility.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionedConnectionPackage {
    V1(ConnectionPackageV1),
    V2(ConnectionPackage),
}

impl VersionedConnectionPackage {
    pub fn is_last_resort(&self) -> Option<bool> {
        match self {
            VersionedConnectionPackage::V1(_) => None,
            VersionedConnectionPackage::V2(cp_v2) => Some(cp_v2.is_last_resort()),
        }
    }

    pub fn into_current(self) -> ConnectionPackage {
        match self {
            VersionedConnectionPackage::V1(cp) => cp.into(),
            VersionedConnectionPackage::V2(cp_v2) => cp_v2,
        }
    }
}

/// See [`VersionedConnectionPackage`].
pub enum VersionedConnectionPackageIn {
    V1(ConnectionPackageV1In),
    V2(ConnectionPackageIn),
}

impl VersionedConnectionPackageIn {
    pub fn verify(self) -> Result<VersionedConnectionPackage, SignatureVerificationError> {
        match self {
            VersionedConnectionPackageIn::V1(cp) => {
                let verified = cp.verify()?;
                Ok(VersionedConnectionPackage::V1(verified))
            }
            VersionedConnectionPackageIn::V2(cp) => {
                let verified = cp.verify()?;
                Ok(VersionedConnectionPackage::V2(verified))
            }
        }
    }
}

mod payload {
    use super::*;
    use crate::{
        credentials::keys::{HandleKeyType, HandleVerifyingKey},
        crypto::{
            ConnectionEncryptionKey,
            signatures::{
                private_keys::SignatureVerificationError,
                signable::{Signable, SignedStruct, Verifiable, VerifiedStruct},
            },
        },
        identifiers::UserHandleHash,
        messages::AirProtocolVersion,
        time::ExpirationData,
    };

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(transparent)]
    pub struct TlsBool(pub bool);

    impl From<bool> for TlsBool {
        fn from(value: bool) -> Self {
            TlsBool(value)
        }
    }

    impl tls_codec::Size for TlsBool {
        fn tls_serialized_len(&self) -> usize {
            1
        }
    }

    impl tls_codec::Serialize for TlsBool {
        fn tls_serialize<W: std::io::Write>(
            &self,
            writer: &mut W,
        ) -> Result<usize, tls_codec::Error> {
            let byte = if self.0 { 1u8 } else { 0u8 };
            writer.write_all(&[byte])?;
            Ok(1)
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq, TlsSerialize, TlsSize, Serialize, Deserialize)]
    pub struct ConnectionPackagePayload {
        pub protocol_version: AirProtocolVersion,
        pub user_handle_hash: UserHandleHash,
        pub encryption_key: ConnectionEncryptionKey,
        pub lifetime: ExpirationData,
        pub verifying_key: HandleVerifyingKey,
        pub is_last_resort: TlsBool,
    }

    mod private_mod {
        #[derive(Default)]
        pub struct Seal;
    }

    impl Signable for ConnectionPackagePayload {
        type SignedOutput = ConnectionPackage;

        fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
            self.tls_serialize_detached()
        }

        fn label(&self) -> &str {
            LABEL
        }
    }

    impl SignedStruct<ConnectionPackagePayload, HandleKeyType> for ConnectionPackage {
        fn from_payload(payload: ConnectionPackagePayload, signature: HandleSignature) -> Self {
            Self { payload, signature }
        }
    }

    #[derive(Debug)]
    pub struct ConnectionPackageIn {
        payload: ConnectionPackagePayload,
        signature: HandleSignature,
    }

    impl ConnectionPackageIn {
        pub fn new(payload: ConnectionPackagePayload, signature: HandleSignature) -> Self {
            Self { payload, signature }
        }

        pub fn verify(self) -> Result<ConnectionPackage, SignatureVerificationError> {
            let verifying_key = self.payload.verifying_key.clone();
            <Self as Verifiable>::verify(self, &verifying_key)
        }
    }

    impl Verifiable for ConnectionPackageIn {
        fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
            self.payload.tls_serialize_detached()
        }

        fn signature(&self) -> impl AsRef<[u8]> {
            &self.signature
        }

        fn label(&self) -> &str {
            LABEL
        }
    }

    impl VerifiedStruct<ConnectionPackageIn> for ConnectionPackage {
        type SealingType = private_mod::Seal;

        fn from_verifiable(verifiable: ConnectionPackageIn, _seal: Self::SealingType) -> Self {
            ConnectionPackage {
                payload: verifiable.payload,
                signature: verifiable.signature,
            }
        }
    }
}

impl Labeled for ConnectionPackage {
    const LABEL: &'static str = LABEL;
}

pub type ConnectionPackageHash = Hash<ConnectionPackage>;

impl Hashable for ConnectionPackage {}

#[derive(Debug, Clone, PartialEq, Eq, TlsSerialize, TlsSize, Serialize, Deserialize)]
pub struct ConnectionPackage {
    payload: ConnectionPackagePayload,
    signature: HandleSignature,
}

#[derive(Debug, Error)]
pub enum ConnectionPackageError {
    #[error(transparent)]
    LibraryError(#[from] LibraryError),
    #[error("Error generating decryption key: {0}")]
    DecryptionKeyError(#[from] RandomnessError),
}

impl ConnectionPackage {
    pub fn new(
        user_handle_hash: UserHandleHash,
        signing_key: &HandleSigningKey,
        is_last_resort: bool,
    ) -> Result<(ConnectionDecryptionKey, Self), ConnectionPackageError> {
        let decryption_key = ConnectionDecryptionKey::generate()?;
        let payload = ConnectionPackagePayload {
            protocol_version: AirProtocolVersion::default(),
            user_handle_hash,
            encryption_key: decryption_key.encryption_key().clone(),
            lifetime: ExpirationData::new(CONNECTION_PACKAGE_EXPIRATION),
            verifying_key: signing_key.verifying_key().clone(),
            is_last_resort: TlsBool(is_last_resort),
        };
        let connection_package = payload.sign(signing_key)?;
        Ok((decryption_key, connection_package))
    }

    pub fn from_parts(payload: ConnectionPackagePayload, signature: HandleSignature) -> Self {
        Self { payload, signature }
    }

    pub fn into_parts(self) -> (ConnectionPackagePayload, HandleSignature) {
        (self.payload, self.signature)
    }

    pub fn encryption_key(&self) -> &ConnectionEncryptionKey {
        &self.payload.encryption_key
    }

    pub fn expires_at(&self) -> TimeStamp {
        self.payload.lifetime.not_after()
    }

    pub fn is_last_resort(&self) -> bool {
        self.payload.is_last_resort.0
    }

    #[cfg(feature = "test_utils")]
    pub fn new_for_test(payload: ConnectionPackagePayload, signature: HandleSignature) -> Self {
        Self { payload, signature }
    }
}

impl From<ConnectionPackageV1> for ConnectionPackage {
    fn from(v1: ConnectionPackageV1) -> Self {
        let (payload, signature) = v1.into_parts();
        let payload = ConnectionPackagePayload {
            protocol_version: payload.protocol_version,
            user_handle_hash: payload.user_handle_hash,
            encryption_key: payload.encryption_key,
            lifetime: payload.lifetime,
            verifying_key: payload.verifying_key,
            is_last_resort: false.into(),
        };
        Self { payload, signature }
    }
}
