// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Parsed MLS leaf credentials.

use mls_assist::openmls::prelude::{BasicCredentialError, Credential, CredentialType};
use tls_codec::Serialize as _;

use crate::identifiers::UserId;

use super::{
    SELF_GROUP_CREDENTIAL_TYPE, SelfGroupCredential, SelfGroupCredentialError,
    VerifiableUserCredential,
};

/// A parsed MLS leaf credential.
///
/// Regular group leaves carry an AS-issued user credential. Self-group leaves carry a
/// [`SelfGroupCredential`].
#[derive(Debug, Clone)]
pub enum LeafCredential {
    User(VerifiableUserCredential),
    SelfGroup(SelfGroupCredential),
}

#[derive(Debug, thiserror::Error)]
pub enum LeafCredentialError {
    #[error("unsupported credential type")]
    UnsupportedCredentialType,
    #[error(transparent)]
    User(#[from] BasicCredentialError),
    #[error(transparent)]
    SelfGroup(#[from] SelfGroupCredentialError),
}

impl LeafCredential {
    pub fn from_credential(credential: &Credential) -> Result<Self, LeafCredentialError> {
        match credential.credential_type() {
            CredentialType::Basic => Ok(Self::User(
                VerifiableUserCredential::from_basic_credential(credential)?,
            )),
            CredentialType::Other(SELF_GROUP_CREDENTIAL_TYPE) => Ok(Self::SelfGroup(
                SelfGroupCredential::from_credential(credential)?,
            )),
            CredentialType::X509 | CredentialType::Grease(_) | CredentialType::Other(_) => {
                Err(LeafCredentialError::UnsupportedCredentialType)
            }
        }
    }

    /// The user id of the leaf's owner.
    ///
    /// Self-group leaves carry no user identity: all members of a self-group are clients of the
    /// group's single user, so they resolve to `own_user_id`.
    pub fn user_id<'a>(&'a self, own_user_id: &'a UserId) -> &'a UserId {
        match self {
            LeafCredential::User(credential) => credential.user_id(),
            LeafCredential::SelfGroup(_) => own_user_id,
        }
    }

    /// Room-policy identity of the leaf's owner.
    ///
    /// User credentials resolve to the TLS-serialized user id. Self-group credentials carry no
    /// user identity and resolve to the raw client id bytes, which are unique per client. The two
    /// forms cannot collide: a serialized user id is always longer than the 16 client id bytes.
    pub fn room_policy_identity(&self) -> Result<Vec<u8>, tls_codec::Error> {
        match self {
            LeafCredential::User(credential) => credential.user_id().tls_serialize_detached(),
            LeafCredential::SelfGroup(credential) => {
                Ok(credential.client_id().into_bytes().to_vec())
            }
        }
    }

    /// Returns the contained user credential, or `None` for a self-group leaf.
    pub fn into_user(self) -> Option<VerifiableUserCredential> {
        match self {
            LeafCredential::User(credential) => Some(credential),
            LeafCredential::SelfGroup(_) => None,
        }
    }
}

#[cfg(test)]
mod test {
    use mls_assist::openmls::prelude::{BasicCredential, SignatureScheme};
    use tls_codec::Serialize as _;
    use uuid::Uuid;

    use crate::{
        credentials::{UserCredentialCsr, UserCredentialPayload, keys::AsIntermediateSignature},
        crypto::hash::Hash,
    };

    use super::*;

    /// Build an unverified [`VerifiableUserCredential`] for the given user. The signature is empty,
    /// which is fine here since parsing does not verify.
    fn user_credential(user_id: UserId) -> VerifiableUserCredential {
        let (csr, _prelim_key) = UserCredentialCsr::new(user_id, SignatureScheme::ED25519).unwrap();
        let payload = UserCredentialPayload::new(csr, None, Hash::from_bytes([0u8; 32]));
        VerifiableUserCredential::new(payload, AsIntermediateSignature::empty())
    }

    #[test]
    fn basic_credential_parses_to_user() {
        let user_id = UserId::new(Uuid::new_v4(), "example.com".parse().unwrap());
        let credential = user_credential(user_id.clone());
        let mls_credential: Credential =
            BasicCredential::new(credential.tls_serialize_detached().unwrap()).into();

        let LeafCredential::User(parsed) =
            LeafCredential::from_credential(&mls_credential).unwrap()
        else {
            panic!("expected a user credential");
        };
        assert_eq!(parsed.user_id(), &user_id);
    }

    #[test]
    fn self_group_credential_parses_to_self_group() {
        let credential = SelfGroupCredential::new(Uuid::new_v4());
        let mls_credential = credential.to_credential().unwrap();

        let LeafCredential::SelfGroup(parsed) =
            LeafCredential::from_credential(&mls_credential).unwrap()
        else {
            panic!("expected a self-group credential");
        };
        assert_eq!(parsed, credential);
    }

    #[test]
    fn unsupported_credential_type_is_rejected() {
        let mls_credential = Credential::new(CredentialType::Other(0xff03), Vec::new());
        let error = LeafCredential::from_credential(&mls_credential).unwrap_err();
        assert!(matches!(
            error,
            LeafCredentialError::UnsupportedCredentialType
        ));
    }

    #[test]
    fn room_policy_identity_resolution() {
        let user_id = UserId::new(Uuid::new_v4(), "example.com".parse().unwrap());
        let user_leaf = LeafCredential::User(user_credential(user_id.clone()));
        assert_eq!(
            user_leaf.room_policy_identity().unwrap(),
            user_id.tls_serialize_detached().unwrap()
        );

        let client_id = Uuid::new_v4();
        let self_group_leaf = LeafCredential::SelfGroup(SelfGroupCredential::new(client_id));
        assert_eq!(
            self_group_leaf.room_policy_identity().unwrap(),
            client_id.into_bytes().to_vec()
        );
    }

    #[test]
    fn user_id_resolution() {
        let user_id = UserId::new(Uuid::new_v4(), "example.com".parse().unwrap());
        let own_user_id = UserId::new(Uuid::new_v4(), "example.com".parse().unwrap());

        let user_leaf = LeafCredential::User(user_credential(user_id.clone()));
        assert_eq!(user_leaf.user_id(&own_user_id), &user_id);

        let self_group_leaf = LeafCredential::SelfGroup(SelfGroupCredential::new(Uuid::new_v4()));
        assert_eq!(self_group_leaf.user_id(&own_user_id), &own_user_id);
    }
}
