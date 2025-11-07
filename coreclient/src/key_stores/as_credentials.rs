// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;

use airapiclient::as_api::AsRequestError;
use aircommon::{
    credentials::{
        AsCredential, AsCredentialBody, AsIntermediateCredential, AsIntermediateCredentialBody,
        VerifiableClientCredential,
    },
    crypto::{
        hash::Hash,
        signatures::{private_keys::SignatureVerificationError, signable::Verifiable},
    },
    identifiers::Fqdn,
};
use sqlx::{
    Database, Encode, Sqlite, SqliteConnection, SqliteExecutor, Type, encode::IsNull,
    error::BoxDynError, query, query_scalar,
};
use thiserror::Error;
use tracing::info;

use crate::clients::api_clients::ApiClientsError;

use super::*;

pub(crate) enum AsCredentials {
    // TODO: Why is this unused
    #[expect(dead_code)]
    AsCredential(AsCredential),
    AsIntermediateCredential(AsIntermediateCredential),
}

enum AsCredentialsBodyRef<'a> {
    AsCredential(&'a AsCredentialBody),
    AsIntermediateCredential(&'a AsIntermediateCredentialBody),
}

impl Type<Sqlite> for AsCredentialsBodyRef<'_> {
    fn type_info() -> <Sqlite as Database>::TypeInfo {
        <Vec<u8> as Type<Sqlite>>::type_info()
    }
}

impl Encode<'_, Sqlite> for AsCredentialsBodyRef<'_> {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as Database>::ArgumentBuffer<'_>,
    ) -> Result<IsNull, BoxDynError> {
        match self {
            Self::AsCredential(body) => Encode::<Sqlite>::encode_by_ref(body, buf),
            Self::AsIntermediateCredential(body) => Encode::<Sqlite>::encode_by_ref(body, buf),
        }
    }
}

impl AsCredentials {
    fn domain(&self) -> &Fqdn {
        match self {
            AsCredentials::AsCredential(credential) => credential.domain(),
            AsCredentials::AsIntermediateCredential(credential) => credential.domain(),
        }
    }

    fn fingerprint(&self) -> &[u8] {
        match self {
            AsCredentials::AsCredential(credential) => credential.fingerprint().as_bytes(),
            AsCredentials::AsIntermediateCredential(credential) => {
                credential.fingerprint().as_bytes()
            }
        }
    }

    fn credential_type(&self) -> &str {
        match self {
            AsCredentials::AsCredential(_) => "as_credential",
            AsCredentials::AsIntermediateCredential(_) => "as_intermediate_credential",
        }
    }

    fn body(&self) -> AsCredentialsBodyRef<'_> {
        match self {
            AsCredentials::AsCredential(credential) => {
                AsCredentialsBodyRef::AsCredential(credential.body())
            }
            AsCredentials::AsIntermediateCredential(credential) => {
                AsCredentialsBodyRef::AsIntermediateCredential(credential.body())
            }
        }
    }

    async fn store(&self, executor: impl SqliteExecutor<'_>) -> sqlx::Result<()> {
        let fingerpint = self.fingerprint();
        let domain = self.domain();
        let credential_type = self.credential_type();
        let body = self.body();
        query!(
            "INSERT OR REPLACE INTO as_credential
                (fingerprint, user_domain, credential_type, credential) VALUES (?, ?, ?, ?)",
            fingerpint,
            domain,
            credential_type,
            body,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    async fn load_intermediate(
        executor: impl SqliteExecutor<'_>,
        fingerprint_option: Option<&Hash<AsIntermediateCredentialBody>>,
        domain: &Fqdn,
    ) -> sqlx::Result<Option<AsIntermediateCredential>> {
        let body: Option<AsIntermediateCredentialBody> =
            if let Some(fingerprint) = fingerprint_option {
                query_scalar!(
                    r#"SELECT
                    credential AS "credential: _"
                FROM as_credential
                WHERE user_domain = ?
                    AND credential_type = 'as_intermediate_credential'
                    AND fingerprint = ?"#,
                    domain,
                    fingerprint,
                )
                .fetch_optional(executor)
                .await?
            } else {
                query_scalar!(
                    r#"SELECT
                    credential AS "credential: _"
                FROM as_credential
                WHERE user_domain = ?
                    AND credential_type = 'as_intermediate_credential'"#,
                    domain
                )
                .fetch_optional(executor)
                .await?
            };
        Ok(body.map(AsIntermediateCredential::from))
    }

    async fn fetch_credentials(
        domain: &Fqdn,
        api_clients: &ApiClients,
    ) -> Result<Vec<AsIntermediateCredential>, AsCredentialStoreError> {
        let as_credentials_response = api_clients.get(domain)?.as_as_credentials().await?;
        let as_credentials: HashMap<Hash<AsCredentialBody>, AsCredential> = as_credentials_response
            .as_credentials
            .into_iter()
            .map(|credential| (*credential.fingerprint(), credential))
            .collect::<HashMap<_, _>>();
        let mut as_inter_creds = vec![];
        for as_inter_cred in as_credentials_response.as_intermediate_credentials {
            let as_credential = as_credentials
                .get(as_inter_cred.signer_fingerprint())
                .ok_or(AsCredentialStoreError::AsCredentialNotFound)?;
            let verified_credential = as_inter_cred.verify(as_credential.verifying_key())?;
            as_inter_creds.push(verified_credential);
        }
        Ok(as_inter_creds)
    }

    pub(crate) async fn fetch_for_verification(
        connection: &mut SqliteConnection,
        api_clients: &ApiClients,
        verifiable_credentials: impl Iterator<Item = &VerifiableClientCredential>,
    ) -> Result<HashMap<Hash<AsIntermediateCredentialBody>, AsIntermediateCredential>> {
        let mut as_credentials = HashMap::new();
        for verifiable_credential in verifiable_credentials {
            if as_credentials.contains_key(verifiable_credential.signer_fingerprint()) {
                continue;
            }

            let as_credential = AsCredentials::get(
                connection,
                api_clients,
                verifiable_credential.domain(),
                verifiable_credential.signer_fingerprint(),
            )
            .await?;
            as_credentials.insert(*verifiable_credential.signer_fingerprint(), as_credential);
        }
        Ok(as_credentials)
    }

    /// Fetches the credentials of the AS with the given `domain` if they are
    /// not already present in the store.
    pub(crate) async fn get(
        connection: &mut SqliteConnection,
        api_clients: &ApiClients,
        domain: &Fqdn,
        fingerprint: &Hash<AsIntermediateCredentialBody>,
    ) -> Result<AsIntermediateCredential, AsCredentialStoreError> {
        info!("Loading AS credential from db");
        // Phase 1: Check if there is a credential in the database.
        let credential_option =
            AsCredentials::load_intermediate(&mut *connection, Some(fingerprint), domain).await?;

        // Phase 2: If there is no credential in the database, fetch it from the AS.
        let credential = if let Some(credential) = credential_option {
            credential
        } else {
            // Phase 2a: Fetch the credential.
            let credential = Self::fetch_credentials(domain, api_clients)
                .await?
                .into_iter()
                .find(|credential| credential.fingerprint() == fingerprint)
                .ok_or(AsCredentialStoreError::AsIntermediateCredentialNotFound)?;

            // Phase 2b: Store it in the database.
            let credential_type = AsCredentials::AsIntermediateCredential(credential);
            credential_type.store(&mut *connection).await?;
            let AsCredentials::AsIntermediateCredential(credential) = credential_type else {
                unreachable!()
            };
            credential
        };
        if credential.domain() != domain {
            return Err(AsCredentialStoreError::AsIntermediateCredentialNotFound);
        }
        Ok(credential)
    }

    pub(crate) async fn get_intermediate_credential(
        executor: impl SqliteExecutor<'_>,
        api_clients: &ApiClients,
        domain: &Fqdn,
    ) -> Result<AsIntermediateCredential, AsCredentialStoreError> {
        let credential_option = AsCredentials::load_intermediate(executor, None, domain).await?;
        match credential_option {
            Some(credential) => Ok(credential),
            None => {
                let mut credentials = Self::fetch_credentials(domain, api_clients).await?;
                let credential = credentials
                    .pop()
                    .ok_or(AsCredentialStoreError::AsIntermediateCredentialNotFound)?;
                Ok(credential)
            }
        }
    }
}

#[derive(Debug, Error)]
pub(crate) enum AsCredentialStoreError {
    #[error("Can't find AS credential for the given fingerprint")]
    AsCredentialNotFound,
    #[error("Can't find AS intermediate credential for the given fingerprint")]
    AsIntermediateCredentialNotFound,
    #[error(transparent)]
    VerificationError(#[from] SignatureVerificationError),
    #[error(transparent)]
    PersistenceError(#[from] sqlx::Error),
    #[error(transparent)]
    ApiClientsError(#[from] ApiClientsError),
    #[error(transparent)]
    AsRequestError(#[from] AsRequestError),
}
