// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::{
        AsIntermediateCredentialBody, ClientCredential, VerifiableClientCredential,
        keys::{AsIntermediateVerifyingKey, ClientSignature},
    },
    crypto::{
        ear::{
            EarDecryptable, EarEncryptable,
            keys::{
                FriendshipPackageEarKey, GroupStateEarKey, IdentityLinkWrapperKey,
                WelcomeAttributionInfoEarKey,
            },
        },
        hash::Hash,
        hpke::{HpkeDecryptable, HpkeEncryptable},
        indexed_aead::keys::UserProfileBaseSecret,
        kdf::keys::ConnectionKeyType,
        signatures::{
            private_keys::SignatureVerificationError,
            signable::{Signable, SignedStruct, Verifiable, VerifiedStruct},
        },
    },
    identifiers::{Fqdn, UserHandle},
    messages::{
        FriendshipToken,
        client_as::{EncryptedConnectionOffer, EncryptedFriendshipPackageCtype},
        connection_package::ConnectionPackageHash,
    },
};
use openmls::group::GroupId;
use payload::{ConnectionOfferPayload, ConnectionOfferPayloadIn};
use tbs::{ConnectionOfferTbs, VerifiableConnectionOffer};
use tls_codec::{Serialize as TlsSerializeTrait, TlsDeserializeBytes, TlsSerialize, TlsSize};

pub(crate) mod payload {
    use aircommon::{LibraryError, credentials::keys::ClientSigningKey, identifiers::UserHandle};

    use super::*;

    #[derive(Debug, TlsDeserializeBytes, TlsSize, Clone)]
    pub(super) struct ConnectionOfferPayloadIn {
        pub(super) sender_client_credential: VerifiableClientCredential,
        connection_group_id: GroupId,
        connection_group_ear_key: GroupStateEarKey,
        connection_group_identity_link_wrapper_key: IdentityLinkWrapperKey,
        friendship_package_ear_key: FriendshipPackageEarKey,
        friendship_package: FriendshipPackage,
        connection_package_hash: ConnectionPackageHash,
    }

    impl ConnectionOfferPayloadIn {
        pub(super) fn verify(
            self,
            verifying_key: &AsIntermediateVerifyingKey,
        ) -> Result<ConnectionOfferPayload, SignatureVerificationError> {
            let client_credential = self.sender_client_credential.verify(verifying_key)?;
            let verified_payload = ConnectionOfferPayload {
                sender_client_credential: client_credential,
                connection_info: ConnectionInfo {
                    connection_group_id: self.connection_group_id,
                    connection_group_ear_key: self.connection_group_ear_key,
                    connection_group_identity_link_wrapper_key: self
                        .connection_group_identity_link_wrapper_key,
                    friendship_package_ear_key: self.friendship_package_ear_key,
                    friendship_package: self.friendship_package,
                },
                connection_package_hash: self.connection_package_hash,
            };
            Ok(verified_payload)
        }
    }

    #[derive(Debug, TlsSerialize, TlsDeserializeBytes, TlsSize, Clone)]
    #[cfg_attr(test, derive(PartialEq))]
    pub(crate) struct ConnectionInfo {
        pub(crate) connection_group_id: GroupId,
        pub(crate) connection_group_ear_key: GroupStateEarKey,
        pub(crate) connection_group_identity_link_wrapper_key: IdentityLinkWrapperKey,
        pub(crate) friendship_package_ear_key: FriendshipPackageEarKey,
        pub(crate) friendship_package: FriendshipPackage,
    }

    #[derive(Debug, TlsSerialize, TlsSize, Clone)]
    #[cfg_attr(test, derive(PartialEq))]
    pub(crate) struct ConnectionOfferPayload {
        pub(crate) sender_client_credential: ClientCredential,
        pub(crate) connection_info: ConnectionInfo,
        pub(crate) connection_package_hash: ConnectionPackageHash,
    }

    impl ConnectionOfferPayload {
        pub(crate) fn sign(
            self,
            signing_key: &ClientSigningKey,
            recipient_user_handle: UserHandle,
            connection_package_hash: ConnectionPackageHash,
        ) -> Result<ConnectionOffer, LibraryError> {
            let tbs = ConnectionOfferTbs::from_payload(
                self.clone(),
                recipient_user_handle,
                connection_package_hash,
            );
            tbs.sign(signing_key)
        }

        #[cfg(test)]
        pub(super) fn dummy(client_credential: ClientCredential) -> Self {
            Self {
                sender_client_credential: client_credential,
                connection_info: ConnectionInfo {
                    connection_group_id: GroupId::from_slice(b"dummy_group_id"),
                    connection_group_ear_key: GroupStateEarKey::random().unwrap(),
                    connection_group_identity_link_wrapper_key: IdentityLinkWrapperKey::random()
                        .unwrap(),
                    friendship_package_ear_key: FriendshipPackageEarKey::random().unwrap(),
                    friendship_package: FriendshipPackage {
                        friendship_token: FriendshipToken::random().unwrap(),
                        wai_ear_key: WelcomeAttributionInfoEarKey::random().unwrap(),
                        user_profile_base_secret: UserProfileBaseSecret::random().unwrap(),
                    },
                },
                connection_package_hash: ConnectionPackageHash::new_for_test(vec![0; 32]),
            }
        }
    }
}

mod tbs {
    use super::*;
    use aircommon::{
        credentials::keys::{ClientKeyType, ClientSignature},
        identifiers::UserHandle,
    };

    use super::payload::ConnectionOfferPayload;

    #[derive(Debug, TlsSerialize, TlsSize, Clone)]
    pub(super) struct ConnectionOfferTbs {
        payload: ConnectionOfferPayload,
        recipient_user_handle: UserHandle,
        connection_package_hash: ConnectionPackageHash,
    }

    impl ConnectionOfferTbs {
        pub(super) fn from_payload(
            payload: ConnectionOfferPayload,
            recipient_user_handle: UserHandle,
            connection_package_hash: ConnectionPackageHash,
        ) -> Self {
            Self {
                payload,
                recipient_user_handle,
                connection_package_hash,
            }
        }
    }

    impl Signable for ConnectionOfferTbs {
        type SignedOutput = ConnectionOffer;

        fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
            self.tls_serialize_detached()
        }

        fn label(&self) -> &str {
            "ConnectionOfferTBS"
        }
    }

    impl SignedStruct<ConnectionOfferTbs, ClientKeyType> for ConnectionOffer {
        fn from_payload(tbs: ConnectionOfferTbs, signature: ClientSignature) -> Self {
            Self {
                payload: tbs.payload,
                signature,
            }
        }
    }

    mod private_mod {
        #[derive(Default)]
        pub struct Seal;
    }

    #[derive(Debug)]
    pub(super) struct VerifiableConnectionOffer {
        tbs: ConnectionOfferTbs,
        signature: ClientSignature,
    }

    impl VerifiableConnectionOffer {
        pub(super) fn from_verified_payload(
            verified_payload: ConnectionOfferPayload,
            recipient_user_handle: UserHandle,
            connection_package_hash: ConnectionPackageHash,
            signature: ClientSignature,
        ) -> Self {
            let tbs = ConnectionOfferTbs::from_payload(
                verified_payload,
                recipient_user_handle,
                connection_package_hash,
            );
            Self { tbs, signature }
        }

        pub(super) fn verify(self) -> Result<ConnectionOfferPayload, SignatureVerificationError> {
            let verifying_key = self
                .tbs
                .payload
                .sender_client_credential
                .verifying_key()
                .clone();
            <Self as Verifiable>::verify(self, &verifying_key)
        }
    }

    impl Verifiable for VerifiableConnectionOffer {
        fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
            self.tbs.tls_serialize_detached()
        }

        fn signature(&self) -> impl AsRef<[u8]> {
            &self.signature
        }

        fn label(&self) -> &str {
            "ConnectionOfferTBS"
        }
    }

    impl VerifiedStruct<VerifiableConnectionOffer> for ConnectionOfferPayload {
        type SealingType = private_mod::Seal;

        fn from_verifiable(
            verifiable: VerifiableConnectionOffer,
            _seal: Self::SealingType,
        ) -> Self {
            verifiable.tbs.payload
        }
    }
}

#[derive(Debug, TlsSerialize, TlsSize, Clone)]
pub(crate) struct ConnectionOffer {
    payload: ConnectionOfferPayload,
    signature: ClientSignature,
}

impl HpkeEncryptable<ConnectionKeyType, EncryptedConnectionOffer> for ConnectionOffer {}

#[derive(Debug, TlsDeserializeBytes, TlsSize, Clone)]
pub(super) struct ConnectionOfferIn {
    payload: ConnectionOfferPayloadIn,
    signature: ClientSignature,
}

impl ConnectionOfferIn {
    pub(super) fn sender_domain(&self) -> &Fqdn {
        self.payload.sender_client_credential.domain()
    }

    pub(super) fn signer_fingerprint(&self) -> &Hash<AsIntermediateCredentialBody> {
        self.payload.sender_client_credential.signer_fingerprint()
    }

    pub(super) fn verify(
        self,
        verifying_key: &AsIntermediateVerifyingKey,
        recipient_user_handle: UserHandle,
        connection_package_hash: ConnectionPackageHash,
    ) -> Result<ConnectionOfferPayload, SignatureVerificationError> {
        let verified_payload = self.payload.verify(verifying_key)?;
        VerifiableConnectionOffer::from_verified_payload(
            verified_payload,
            recipient_user_handle,
            connection_package_hash,
            self.signature,
        )
        .verify()
    }
}

impl HpkeDecryptable<ConnectionKeyType, EncryptedConnectionOffer> for ConnectionOfferIn {}

#[derive(Debug, Clone, TlsDeserializeBytes, TlsSerialize, TlsSize)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct FriendshipPackage {
    pub(crate) friendship_token: FriendshipToken,
    pub(crate) wai_ear_key: WelcomeAttributionInfoEarKey,
    pub(crate) user_profile_base_secret: UserProfileBaseSecret,
}

impl EarEncryptable<FriendshipPackageEarKey, EncryptedFriendshipPackageCtype>
    for FriendshipPackage
{
}
impl EarDecryptable<FriendshipPackageEarKey, EncryptedFriendshipPackageCtype>
    for FriendshipPackage
{
}

#[cfg(test)]
mod tests {
    use aircommon::{
        credentials::test_utils::create_test_credentials,
        crypto::signatures::private_keys::SignatureVerificationError,
        identifiers::{UserHandle, UserId},
        messages::connection_package::ConnectionPackageHash,
    };
    use tls_codec::{DeserializeBytes as _, Serialize};

    use super::{ConnectionOfferIn, payload::ConnectionOfferPayload};

    #[test]
    fn signing_and_verifying() {
        let sender_user_id = UserId::random("localhost".parse().unwrap());
        let (as_sk, client_sk) = create_test_credentials(sender_user_id);
        let cep_payload = ConnectionOfferPayload::dummy(client_sk.credential().clone());
        let user_handle = UserHandle::new("ellie_01".to_owned()).unwrap();
        let hash = ConnectionPackageHash::new_for_test(vec![0; 32]);
        let cep = cep_payload
            .clone()
            .sign(&client_sk, user_handle.clone(), hash)
            .unwrap();
        let cep_in =
            ConnectionOfferIn::tls_deserialize_exact_bytes(&cep.tls_serialize_detached().unwrap())
                .unwrap();
        let cep_verified = cep_in
            .clone()
            .verify(as_sk.verifying_key(), user_handle.clone(), hash)
            .unwrap();
        assert_eq!(cep_verified, cep_payload);

        // Try with a different recipient
        let user_handle_2 = UserHandle::new("ellie_02".to_owned()).unwrap();
        let err = cep_in
            .verify(as_sk.verifying_key(), user_handle_2, hash)
            .unwrap_err();
        assert!(matches!(
            err,
            SignatureVerificationError::VerificationFailure
        ));
    }
}
