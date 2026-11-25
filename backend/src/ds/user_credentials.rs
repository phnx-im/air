// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Authentication of the leaf credentials that enter a group: user credentials and signed
//! self-group credentials, both against the key the AS stores for their user.

use std::collections::{HashMap, hash_map::Entry};

use aircommon::{
    credentials::{
        LeafCredential, SelfGroupCredential, SelfGroupCredentialVerificationError,
        VerifiableUserCredential, keys::ClientVerifyingKey,
    },
    identifiers::UserId,
};
use tonic::Status;
use tracing::{error, warn};

use crate::auth_service::AsConnector;

/// A signed self-group credential with the signature key of its leaf and the user ID in its
/// payload.
struct SignedSelfGroupLeaf {
    credential: SelfGroupCredential,
    signature_key: Vec<u8>,
    /// Read without checking the signature.
    unverified_user_id: UserId,
}

/// Leaf credentials that enter a group and need to be authenticated with the AS.
///
/// Unsigned self-group credentials carry nothing to authenticate and are not kept.
#[derive(Default)]
pub(super) struct EnteringCredentials {
    user: Vec<VerifiableUserCredential>,
    self_group: Vec<SignedSelfGroupLeaf>,
}

impl EnteringCredentials {
    /// Adds the credential of a leaf whose MLS signature key is `signature_key`.
    ///
    /// Fails for a self-group credential whose signed part is malformed or whose payload doesn't
    /// decode.
    pub(super) fn push(
        &mut self,
        credential: LeafCredential,
        signature_key: Vec<u8>,
    ) -> Result<(), SelfGroupCredentialVerificationError> {
        match credential {
            LeafCredential::User(credential) => self.user.push(credential),
            LeafCredential::SelfGroup(credential) => {
                if let Some(payload) = credential.unverified_payload()? {
                    self.self_group.push(SignedSelfGroupLeaf {
                        unverified_user_id: payload.user_id().clone(),
                        credential,
                        signature_key,
                    });
                }
            }
        }
        Ok(())
    }

    /// The user IDs in the payloads of the signed self-group credentials, read without checking
    /// signatures.
    pub(super) fn unverified_self_group_user_ids(&self) -> impl Iterator<Item = &UserId> {
        self.self_group.iter().map(|leaf| &leaf.unverified_user_id)
    }

    #[cfg(test)]
    pub(super) fn users(&self) -> &[VerifiableUserCredential] {
        &self.user
    }

    #[cfg(test)]
    pub(super) fn self_group_client_ids(&self) -> Vec<uuid::Uuid> {
        self.self_group
            .iter()
            .map(|leaf| leaf.credential.client_id())
            .collect()
    }
}

/// The outcome of a group-state step along with the user and self-group credentials that need to
/// be verified with the AS.
#[must_use = "the outcome is only available once its credentials are authenticated"]
pub(super) struct Unauthenticated<T> {
    outcome: T,
    credentials: EnteringCredentials,
}

impl<T> Unauthenticated<T> {
    /// Holds back `outcome` until each of `credentials` is authenticated.
    pub(super) fn new(outcome: T, credentials: EnteringCredentials) -> Self {
        Self {
            outcome,
            credentials,
        }
    }

    /// Transforms the held-back outcome with `f`, keeping the credentials.
    pub(super) fn try_map<U, E>(
        self,
        f: impl FnOnce(T) -> Result<U, E>,
    ) -> Result<Unauthenticated<U>, E> {
        Ok(Unauthenticated {
            outcome: f(self.outcome)?,
            credentials: self.credentials,
        })
    }

    /// Returns the outcome once every credential is authenticated, or the error of the first
    /// credential that fails.
    ///
    /// A user unknown to the AS or a failed check is [`Status::permission_denied`], a failed lookup
    /// [`Status::internal`].
    pub(super) async fn authenticate(self, as_connector: &impl AsConnector) -> Result<T, Status> {
        for credential in &self.credentials.user {
            as_connector
                .authenticate_user_credential(credential)
                .await?;
        }
        // All signed self-group credentials in a group contain the same user ID, so this is usually
        // a single lookup.
        let mut stored_keys: HashMap<&UserId, ClientVerifyingKey> = HashMap::new();
        for leaf in &self.credentials.self_group {
            let user_id = &leaf.unverified_user_id;
            let stored_key = match stored_keys.entry(user_id) {
                Entry::Occupied(entry) => entry.into_mut(),
                Entry::Vacant(entry) => entry.insert(stored_user_key(as_connector, user_id).await?),
            };
            leaf.credential
                .verify(&leaf.signature_key, user_id, stored_key)
                .map_err(|error| {
                    warn!(%error, "Self-group credential does not verify");
                    Status::permission_denied("self-group credential does not verify")
                })?;
        }
        Ok(self.outcome)
    }

    #[cfg(test)]
    pub(super) fn into_parts(self) -> (T, EnteringCredentials) {
        (self.outcome, self.credentials)
    }
}

/// The client verifying key the AS stores for `user_id`.
async fn stored_user_key(
    as_connector: &impl AsConnector,
    user_id: &UserId,
) -> Result<ClientVerifyingKey, Status> {
    as_connector
        .client_verifying_key(user_id)
        .await
        .map_err(|error| {
            error!(%error, "Failed to load client verifying key from AS");
            Status::internal("failed to load client verifying key")
        })?
        .ok_or_else(|| {
            warn!("Self-group credential of a user unknown to the AS");
            Status::permission_denied("unknown user")
        })
}

#[cfg(test)]
mod tests {
    use aircommon::{
        credentials::{
            UserCredentialCsr, UserCredentialPayload,
            keys::{AsIntermediateSignature, UserSigningKey},
            test_utils::random_user_signing_key,
        },
        crypto::hash::Hash,
    };
    use mls_assist::openmls::prelude::SignatureScheme;
    use tonic::Code;
    use uuid::Uuid;

    use super::*;
    use crate::ds::test_utils::{
        FailingAs, StoredKeys, assert_authenticated, authentication_cases,
    };

    fn credential(user_id: UserId) -> VerifiableUserCredential {
        let (csr, _prelim_key) = UserCredentialCsr::new(user_id, SignatureScheme::ED25519).unwrap();
        let payload = UserCredentialPayload::new(csr, None, Hash::from_bytes([0; 32]));
        VerifiableUserCredential::new(payload, AsIntermediateSignature::empty())
    }

    fn random_user() -> UserId {
        UserId::random("example.com".parse().unwrap())
    }

    #[tokio::test]
    async fn a_credential_with_the_stored_key_is_authenticated() {
        let credential = credential(random_user());
        let mut stored = StoredKeys::default();
        stored.register(&credential);

        stored
            .authenticate_user_credential(&credential)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn a_credential_with_another_key_is_rejected() {
        let user_id = random_user();
        let mut stored = StoredKeys::default();
        stored.register(&credential(user_id.clone()));

        let status = stored
            .authenticate_user_credential(&credential(user_id))
            .await
            .unwrap_err();
        assert_eq!(status.code(), Code::PermissionDenied);
    }

    #[tokio::test]
    async fn a_credential_of_an_unknown_user_is_rejected() {
        let status = StoredKeys::default()
            .authenticate_user_credential(&credential(random_user()))
            .await
            .unwrap_err();
        assert_eq!(status.code(), Code::PermissionDenied);
    }

    #[tokio::test]
    async fn a_credential_of_another_domain_is_rejected() {
        let registered = credential(random_user());
        let mut stored = StoredKeys::default();
        stored.register(&registered);

        let (mut csr, _prelim_key) = UserCredentialCsr::new(
            UserId::new(
                registered.user_id().uuid(),
                "other.example".parse().unwrap(),
            ),
            SignatureScheme::ED25519,
        )
        .unwrap();
        csr.verifying_key = registered.verifying_key().clone();
        let payload = UserCredentialPayload::new(csr, None, Hash::from_bytes([0; 32]));
        let foreign = VerifiableUserCredential::new(payload, AsIntermediateSignature::empty());

        let status = stored
            .authenticate_user_credential(&foreign)
            .await
            .unwrap_err();
        assert_eq!(status.code(), Code::PermissionDenied);
    }

    #[tokio::test]
    async fn a_failing_lookup_is_an_internal_error() {
        let status = FailingAs
            .authenticate_user_credential(&credential(random_user()))
            .await
            .unwrap_err();
        assert_eq!(status.code(), Code::Internal);
    }

    #[tokio::test]
    async fn the_outcome_is_withheld_until_every_credential_is_authenticated() {
        let user = credential(random_user());
        let signing_key = random_user_signing_key();
        let mut stored = StoredKeys::of(&[&signing_key]);
        stored.register(&user);
        let cases = [
            (
                LeafCredential::User(user),
                LeafCredential::User(credential(random_user())),
            ),
            (
                LeafCredential::SelfGroup(signed_self_group_credential(&signing_key)),
                LeafCredential::SelfGroup(signed_self_group_credential(&random_user_signing_key())),
            ),
        ];

        for (registered, unknown) in cases {
            let accepted = Unauthenticated::new("outcome", entering([registered.clone()]));
            assert_eq!(accepted.authenticate(&stored).await.unwrap(), "outcome");

            let rejected = Unauthenticated::new("outcome", entering([registered, unknown]));
            let status = rejected.authenticate(&stored).await.unwrap_err();
            assert_eq!(status.code(), Code::PermissionDenied);
        }
    }

    const LEAF_KEY: [u8; 32] = [1; 32];

    /// The entering `credentials`, each of a leaf with the signature key [`LEAF_KEY`].
    fn entering(credentials: impl IntoIterator<Item = LeafCredential>) -> EnteringCredentials {
        let mut entering = EnteringCredentials::default();
        for credential in credentials {
            entering.push(credential, LEAF_KEY.to_vec()).unwrap();
        }
        entering
    }

    /// Authenticates `credential` of a leaf with the signature key [`LEAF_KEY`].
    async fn authenticate_self_group(
        as_connector: &impl AsConnector,
        credential: SelfGroupCredential,
    ) -> Result<(), Status> {
        Unauthenticated::new((), entering([LeafCredential::SelfGroup(credential)]))
            .authenticate(as_connector)
            .await
    }

    fn signed_self_group_credential(signing_key: &UserSigningKey) -> SelfGroupCredential {
        SelfGroupCredential::signed(Uuid::new_v4(), &LEAF_KEY, signing_key).unwrap()
    }

    #[tokio::test]
    async fn a_self_group_credential_is_authenticated_against_the_key_the_as_stores() {
        let alice = random_user_signing_key();
        for (stored, signer, accepted) in authentication_cases(&alice) {
            let credential = signed_self_group_credential(&signer);
            assert_authenticated(authenticate_self_group(&stored, credential).await, accepted);
        }
    }

    #[tokio::test]
    async fn a_self_group_credential_for_another_leaf_key_is_rejected() {
        let signing_key = random_user_signing_key();
        let stored = StoredKeys::of(&[&signing_key]);

        let credential =
            SelfGroupCredential::signed(Uuid::new_v4(), &[2; 32], &signing_key).unwrap();
        assert_authenticated(authenticate_self_group(&stored, credential).await, false);
    }

    #[tokio::test]
    async fn a_self_group_credential_of_an_unknown_user_is_rejected() {
        let credential = signed_self_group_credential(&random_user_signing_key());
        let status = authenticate_self_group(&StoredKeys::default(), credential)
            .await
            .unwrap_err();
        assert_eq!(status.code(), Code::PermissionDenied);
        assert_eq!(status.message(), "unknown user");
    }

    #[tokio::test]
    async fn an_unsigned_self_group_credential_needs_no_lookup() {
        let credential = SelfGroupCredential::new(Uuid::new_v4());
        authenticate_self_group(&FailingAs, credential)
            .await
            .unwrap();
    }
}
