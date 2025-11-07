// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use chrono::Duration;
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
        signatures::signable::Signable,
    },
    identifiers::UserHandleHash,
    messages::{AirProtocolVersion, connection_package::ConnectionPackage},
    time::{ExpirationData, TimeStamp},
};

pub use payload::{ConnectionPackageV1In, ConnectionPackageV1Payload};

pub(crate) const CONNECTION_PACKAGE_EXPIRATION: Duration = Duration::days(30);

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

    #[derive(Debug, Clone, PartialEq, Eq, TlsSerialize, TlsSize, Serialize, Deserialize)]
    pub struct ConnectionPackageV1Payload {
        pub protocol_version: AirProtocolVersion,
        pub user_handle_hash: UserHandleHash,
        pub encryption_key: ConnectionEncryptionKey,
        pub lifetime: ExpirationData,
        pub verifying_key: HandleVerifyingKey,
    }

    mod private_mod {
        #[derive(Default)]
        pub struct Seal;
    }

    impl Signable for ConnectionPackageV1Payload {
        type SignedOutput = ConnectionPackageV1;

        fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
            self.tls_serialize_detached()
        }

        fn label(&self) -> &str {
            "ConnectionPackage"
        }
    }

    impl SignedStruct<ConnectionPackageV1Payload, HandleKeyType> for ConnectionPackageV1 {
        fn from_payload(payload: ConnectionPackageV1Payload, signature: HandleSignature) -> Self {
            Self { payload, signature }
        }
    }

    #[derive(Debug)]
    pub struct ConnectionPackageV1In {
        payload: ConnectionPackageV1Payload,
        signature: HandleSignature,
    }

    impl ConnectionPackageV1In {
        pub fn new(payload: ConnectionPackageV1Payload, signature: HandleSignature) -> Self {
            Self { payload, signature }
        }

        pub fn verify(self) -> Result<ConnectionPackageV1, SignatureVerificationError> {
            let verifying_key = self.payload.verifying_key.clone();
            <Self as Verifiable>::verify(self, &verifying_key)
        }
    }

    impl Verifiable for ConnectionPackageV1In {
        fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
            self.payload.tls_serialize_detached()
        }

        fn signature(&self) -> impl AsRef<[u8]> {
            &self.signature
        }

        fn label(&self) -> &str {
            "ConnectionPackage"
        }
    }

    impl VerifiedStruct<ConnectionPackageV1In> for ConnectionPackageV1 {
        type SealingType = private_mod::Seal;

        fn from_verifiable(verifiable: ConnectionPackageV1In, _seal: Self::SealingType) -> Self {
            ConnectionPackageV1 {
                payload: verifiable.payload,
                signature: verifiable.signature,
            }
        }
    }
}

impl Labeled for ConnectionPackageV1 {
    const LABEL: &'static str = "ConnectionPackage";
}

pub type ConnectionPackageV1Hash = Hash<ConnectionPackageV1>;

impl Hashable for ConnectionPackageV1 {}

#[derive(Debug, Clone, PartialEq, Eq, TlsSerialize, TlsSize, Serialize, Deserialize)]
pub struct ConnectionPackageV1 {
    payload: ConnectionPackageV1Payload,
    signature: HandleSignature,
}

#[derive(Debug, Error)]
pub enum ConnectionPackageError {
    #[error(transparent)]
    LibraryError(#[from] LibraryError),
    #[error("Error generating decryption key: {0}")]
    DecryptionKeyError(#[from] RandomnessError),
}

impl ConnectionPackageV1 {
    pub fn new(
        user_handle_hash: UserHandleHash,
        signing_key: &HandleSigningKey,
    ) -> Result<(ConnectionDecryptionKey, Self), ConnectionPackageError> {
        let decryption_key = ConnectionDecryptionKey::generate()?;
        let payload = ConnectionPackageV1Payload {
            protocol_version: AirProtocolVersion::default(),
            user_handle_hash,
            encryption_key: decryption_key.encryption_key().clone(),
            lifetime: ExpirationData::new(CONNECTION_PACKAGE_EXPIRATION),
            verifying_key: signing_key.verifying_key().clone(),
        };
        let connection_package = payload.sign(signing_key)?;
        Ok((decryption_key, connection_package))
    }

    pub fn from_parts(payload: ConnectionPackageV1Payload, signature: HandleSignature) -> Self {
        Self { payload, signature }
    }

    pub fn into_parts(self) -> (ConnectionPackageV1Payload, HandleSignature) {
        (self.payload, self.signature)
    }

    pub fn encryption_key(&self) -> &ConnectionEncryptionKey {
        &self.payload.encryption_key
    }

    pub fn expires_at(&self) -> TimeStamp {
        self.payload.lifetime.not_after()
    }

    #[cfg(feature = "test_utils")]
    pub fn new_for_test(payload: ConnectionPackageV1Payload, signature: HandleSignature) -> Self {
        Self { payload, signature }
    }
}

impl From<ConnectionPackage> for ConnectionPackageV1 {
    fn from(v2: ConnectionPackage) -> Self {
        let (payload, signature) = v2.into_parts();
        let payload = ConnectionPackageV1Payload {
            protocol_version: payload.protocol_version,
            user_handle_hash: payload.user_handle_hash,
            encryption_key: payload.encryption_key,
            lifetime: payload.lifetime,
            verifying_key: payload.verifying_key,
        };
        Self { payload, signature }
    }
}
