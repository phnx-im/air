// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{fmt, ops::Deref};

use aircommon::{
    credentials::{
        AsIntermediateCredential, AsIntermediateCredentialBody, AsIntermediateCredentialCsr,
        keys::{AsIntermediateSigningKey, AsSigningKey},
    },
    crypto::hash::Hash,
    identifiers::Fqdn,
};
use mls_assist::openmls::prelude::SignatureScheme;
use serde::{Deserialize, Serialize};
use sqlx::{Connection, PgConnection};
use tracing::error;

use crate::errors::StorageError;

use super::{CredentialGenerationError, signing_key::StorableSigningKey};

#[derive(Serialize, Deserialize)]
pub(in crate::auth_service) enum IntermediateSigningKey {
    V1(AsIntermediateSigningKey),
}

impl fmt::Debug for IntermediateSigningKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IntermediateSigningKey::V1(_) => write!(f, "V1(<signing_key>)"),
        }
    }
}

impl From<IntermediateSigningKey> for AsIntermediateSigningKey {
    fn from(signing_key: IntermediateSigningKey) -> Self {
        match signing_key {
            IntermediateSigningKey::V1(signing_key) => signing_key,
        }
    }
}

impl From<AsIntermediateSigningKey> for IntermediateSigningKey {
    fn from(signing_key: AsIntermediateSigningKey) -> Self {
        IntermediateSigningKey::V1(signing_key)
    }
}

pub(in crate::auth_service) struct IntermediateCredential(AsIntermediateCredential);

impl Deref for IntermediateCredential {
    type Target = AsIntermediateCredential;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl IntermediateSigningKey {
    pub(in crate::auth_service) async fn generate_sign_and_activate(
        connection: &mut PgConnection,
        domain: Fqdn,
        signature_scheme: SignatureScheme,
    ) -> Result<Self, CredentialGenerationError> {
        // Start the transaction
        let mut transaction = connection.begin().await.map_err(StorageError::from)?;

        // Load the currently active (root) signing key
        let signing_key = StorableSigningKey::load(&mut *transaction)
            .await?
            .ok_or(CredentialGenerationError::NoActiveCredential)?;

        let intermediate_signing_key = Self::generate(domain, signature_scheme, signing_key)?;

        // Store the intermediate signing key
        intermediate_signing_key.store(&mut *transaction).await?;

        // Activate the intermediate signing key
        intermediate_signing_key.activate(&mut *transaction).await?;

        // Commit the transaction
        transaction.commit().await.map_err(StorageError::from)?;

        Ok(intermediate_signing_key)
    }

    fn generate(
        domain: Fqdn,
        signature_scheme: SignatureScheme,
        signing_key: AsSigningKey,
    ) -> Result<IntermediateSigningKey, CredentialGenerationError> {
        let (csr, prelim_signing_key) = AsIntermediateCredentialCsr::new(signature_scheme, domain)?;
        let as_intermediate_credential = csr.sign(&signing_key, None).map_err(|error| {
            error!(%error, "Failed to sign intermediate credential");
            CredentialGenerationError::SigningError
        })?;
        let as_intermediate_signing_key = AsIntermediateSigningKey::from_prelim_key(
            prelim_signing_key,
            as_intermediate_credential,
        )
        .map_err(|error| {
            error!(%error, "Failed to convert intermediate signing key");
            CredentialGenerationError::SigningError
        })?;
        Ok(IntermediateSigningKey::from(as_intermediate_signing_key))
    }

    fn fingerprint(&self) -> &Hash<AsIntermediateCredentialBody> {
        match self {
            IntermediateSigningKey::V1(signing_key) => signing_key.credential().fingerprint(),
        }
    }
}

mod persistence {
    use aircommon::{
        codec::{BlobDecoded, BlobEncoded},
        credentials::{AsIntermediateCredential, keys::AsIntermediateSigningKey},
    };
    use sqlx::{PgExecutor, query_scalar};

    use crate::{auth_service::credentials::CredentialType, errors::StorageError};

    use super::{IntermediateCredential, IntermediateSigningKey};

    impl IntermediateSigningKey {
        pub(super) async fn store(
            &self,
            connection: impl PgExecutor<'_>,
        ) -> Result<(), StorageError> {
            sqlx::query!(
                "INSERT INTO
                    as_signing_key
                    (cred_type, credential_fingerprint, signing_key, currently_active)
                VALUES
                    ($1, $2, $3, $4)",
                CredentialType::Intermediate as _,
                self.fingerprint().as_bytes(),
                BlobEncoded(&self) as _,
                false,
            )
            .execute(connection)
            .await?;
            Ok(())
        }

        pub(in crate::auth_service) async fn load(
            connection: impl PgExecutor<'_>,
        ) -> Result<Option<AsIntermediateSigningKey>, StorageError> {
            let signing_key = query_scalar!(
                r#"SELECT signing_key AS "signing_key: BlobDecoded<IntermediateSigningKey>"
                FROM as_signing_key
                WHERE currently_active = true
                    AND cred_type = 'intermediate'"#
            )
            .fetch_optional(connection)
            .await?;
            Ok(signing_key.map(|BlobDecoded(key)| key.into()))
        }

        pub(super) async fn activate(
            &self,
            connection: impl PgExecutor<'_>,
        ) -> Result<(), StorageError> {
            sqlx::query!(
                "UPDATE as_signing_key
                SET currently_active = CASE
                    WHEN credential_fingerprint = $1 THEN true
                    ELSE false
                END
                WHERE cred_type = 'intermediate'",
                self.fingerprint().as_bytes(),
            )
            .execute(connection)
            .await?;
            Ok(())
        }
    }

    impl IntermediateCredential {
        pub(in crate::auth_service) async fn load_all(
            connection: impl PgExecutor<'_>,
        ) -> Result<Vec<AsIntermediateCredential>, StorageError> {
            let records = query_scalar!(
                r#"SELECT signing_key AS "signing_key: BlobDecoded<IntermediateSigningKey>"
                FROM as_signing_key
                WHERE cred_type = $1"#,
                CredentialType::Intermediate as _,
            )
            .fetch_all(connection)
            .await?;
            Ok(records
                .into_iter()
                .map(|BlobDecoded(signing_key)| {
                    let as_signing_key = AsIntermediateSigningKey::from(signing_key);
                    as_signing_key.into_credential()
                })
                .collect())
        }
    }

    #[cfg(test)]
    mod tests {
        use std::collections::HashSet;

        use aircommon::{
            credentials::AsCredential,
            time::{Duration, ExpirationData},
        };
        use mls_assist::openmls::prelude::SignatureScheme;
        use serde::Serialize;
        use sqlx::PgPool;

        use super::*;

        async fn store_random_signing_key(pool: &PgPool) -> anyhow::Result<IntermediateSigningKey> {
            let (_, key) = AsCredential::new(
                SignatureScheme::ED25519,
                "example.com".parse()?,
                Some(ExpirationData::new(Duration::days(42))),
            )?;
            let storable = IntermediateSigningKey::generate(
                "example.com".parse()?,
                SignatureScheme::ED25519,
                key,
            )?;
            storable.store(pool).await?;
            Ok(storable)
        }

        fn comparable<T: Serialize>(value: &T) -> String {
            serde_json::to_string(value).unwrap()
        }

        #[sqlx::test]
        async fn load(pool: PgPool) -> anyhow::Result<()> {
            let key = store_random_signing_key(&pool).await?;

            let loaded = IntermediateSigningKey::load(&pool).await?;
            assert!(loaded.is_none()); // not active

            key.activate(&pool).await?;
            let loaded = IntermediateSigningKey::load(&pool)
                .await?
                .expect("missing signing key");
            assert_eq!(
                comparable::<IntermediateSigningKey>(&loaded.into()),
                comparable(&key)
            );

            Ok(())
        }

        #[sqlx::test]
        async fn activate(pool: PgPool) -> anyhow::Result<()> {
            let keys = [
                store_random_signing_key(&pool).await?,
                store_random_signing_key(&pool).await?,
                store_random_signing_key(&pool).await?,
            ];

            let loaded = IntermediateSigningKey::load(&pool).await?;
            assert!(loaded.is_none()); // not active

            keys[0].activate(&pool).await?;
            let loaded = IntermediateSigningKey::load(&pool)
                .await?
                .expect("missing signing key");
            assert_eq!(
                comparable::<IntermediateSigningKey>(&loaded.into()),
                comparable(&keys[0])
            );

            keys[1].activate(&pool).await?;
            let loaded = IntermediateSigningKey::load(&pool)
                .await?
                .expect("missing signing key");
            assert_eq!(
                comparable::<IntermediateSigningKey>(&loaded.into()),
                comparable(&keys[1])
            );

            keys[2].activate(&pool).await?;
            let loaded = IntermediateSigningKey::load(&pool)
                .await?
                .expect("missing signing key");
            assert_eq!(
                comparable::<IntermediateSigningKey>(&loaded.into()),
                comparable(&keys[2])
            );

            Ok(())
        }

        #[sqlx::test]
        async fn load_all(pool: PgPool) -> anyhow::Result<()> {
            let keys = [
                store_random_signing_key(&pool).await?,
                store_random_signing_key(&pool).await?,
                store_random_signing_key(&pool).await?,
            ];

            let loaded = IntermediateCredential::load_all(&pool).await?;
            assert_eq!(loaded.len(), 3);

            let loaded_comparable: HashSet<_> = loaded.iter().map(comparable).collect();
            let expected_comparable: HashSet<_> = keys
                .into_iter()
                .map(AsIntermediateSigningKey::from)
                .map(|value| comparable(value.credential()))
                .collect();
            assert_eq!(loaded_comparable, expected_comparable);

            Ok(())
        }
    }
}
