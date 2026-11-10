// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Authentication of the user credentials that enter a group.

use aircommon::credentials::VerifiableUserCredential;
use tonic::Status;
use tracing::{error, warn};

use crate::auth_service::AsConnector;

/// Checks that the key in `credential` is consistent with the one the AS stores for its user.
///
/// Returns [`Status::permission_denied`] if the AS has no record of the user or stores another
/// key for it, and [`Status::internal`] if the lookup fails.
pub(super) async fn authenticate_user_credential(
    as_connector: &impl AsConnector,
    credential: &VerifiableUserCredential,
) -> Result<(), Status> {
    let stored_key = as_connector
        .client_verifying_key(credential.user_id())
        .await
        .map_err(|error| {
            error!(%error, "Failed to load client verifying key from AS");
            Status::internal("failed to load client verifying key")
        })?;
    match stored_key {
        Some(stored_key) if stored_key == *credential.verifying_key() => Ok(()),
        Some(_) => {
            warn!("User credential carries another key than the AS stores");
            Err(Status::permission_denied(
                "user credential does not match the registered user",
            ))
        }
        None => {
            warn!("User credential of a user unknown to the AS");
            Err(Status::permission_denied("unknown user"))
        }
    }
}

/// The outcome of a group-state step along with credentials that need to be verified with the AS.
#[must_use = "the outcome is only available once its user credentials are authenticated"]
pub(super) struct Unauthenticated<T> {
    outcome: T,
    credentials: Vec<VerifiableUserCredential>,
}

impl<T> Unauthenticated<T> {
    /// Holds back `outcome` until each of `credentials` is authenticated.
    pub(super) fn new(outcome: T, credentials: Vec<VerifiableUserCredential>) -> Self {
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

    /// Returns the outcome once every credential passed [`authenticate_user_credential`], or the
    /// error of the first credential that fails.
    pub(super) async fn authenticate(self, as_connector: &impl AsConnector) -> Result<T, Status> {
        for credential in &self.credentials {
            authenticate_user_credential(as_connector, credential).await?;
        }
        Ok(self.outcome)
    }

    #[cfg(test)]
    pub(super) fn into_parts(self) -> (T, Vec<VerifiableUserCredential>) {
        (self.outcome, self.credentials)
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, io};

    use aircommon::{
        credentials::{
            UserCredentialCsr, UserCredentialPayload,
            keys::{AsIntermediateSignature, ClientVerifyingKey},
        },
        crypto::hash::Hash,
        identifiers::UserId,
    };
    use mls_assist::openmls::prelude::SignatureScheme;
    use tonic::Code;

    use super::*;

    #[derive(Debug, Default)]
    struct StoredKeys(HashMap<UserId, ClientVerifyingKey>);

    impl StoredKeys {
        fn register(&mut self, credential: &VerifiableUserCredential) {
            self.0.insert(
                credential.user_id().clone(),
                credential.verifying_key().clone(),
            );
        }
    }

    impl AsConnector for StoredKeys {
        type Error = io::Error;

        async fn client_verifying_key(
            &self,
            user_id: &UserId,
        ) -> Result<Option<ClientVerifyingKey>, Self::Error> {
            Ok(self.0.get(user_id).cloned())
        }
    }

    #[derive(Debug)]
    struct FailingAs;

    impl AsConnector for FailingAs {
        type Error = io::Error;

        async fn client_verifying_key(
            &self,
            _user_id: &UserId,
        ) -> Result<Option<ClientVerifyingKey>, Self::Error> {
            Err(io::Error::other("storage unavailable"))
        }
    }

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

        authenticate_user_credential(&stored, &credential)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn a_credential_with_another_key_is_rejected() {
        let user_id = random_user();
        let mut stored = StoredKeys::default();
        stored.register(&credential(user_id.clone()));

        let status = authenticate_user_credential(&stored, &credential(user_id))
            .await
            .unwrap_err();
        assert_eq!(status.code(), Code::PermissionDenied);
    }

    #[tokio::test]
    async fn a_credential_of_an_unknown_user_is_rejected() {
        let status =
            authenticate_user_credential(&StoredKeys::default(), &credential(random_user()))
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

        let status = authenticate_user_credential(&stored, &foreign)
            .await
            .unwrap_err();
        assert_eq!(status.code(), Code::PermissionDenied);
    }

    #[tokio::test]
    async fn a_failing_lookup_is_an_internal_error() {
        let status = authenticate_user_credential(&FailingAs, &credential(random_user()))
            .await
            .unwrap_err();
        assert_eq!(status.code(), Code::Internal);
    }

    #[tokio::test]
    async fn the_outcome_is_withheld_until_every_credential_is_authenticated() {
        let registered = credential(random_user());
        let mut stored = StoredKeys::default();
        stored.register(&registered);

        let accepted = Unauthenticated::new("outcome", vec![registered.clone()]);
        assert_eq!(accepted.authenticate(&stored).await.unwrap(), "outcome");

        let rejected = Unauthenticated::new("outcome", vec![registered, credential(random_user())]);
        let status = rejected.authenticate(&stored).await.unwrap_err();
        assert_eq!(status.code(), Code::PermissionDenied);
    }
}
