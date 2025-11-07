// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{credentials::ClientCredential, identifiers::UserId, time::TimeStamp};
use sqlx::PgExecutor;

use crate::errors::StorageError;

#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub(super) struct ClientRecord {
    pub(super) activity_time: TimeStamp,
    pub(super) credential: ClientCredential,
    pub(super) token_allowance: i32,
}

const DEFAULT_TOKEN_ALLOWANCE: i32 = 1000;

impl ClientRecord {
    pub(super) async fn new_and_store(
        connection: impl PgExecutor<'_>,
        credential: ClientCredential,
    ) -> Result<Self, StorageError> {
        let record = Self {
            activity_time: TimeStamp::now(),
            credential,
            token_allowance: DEFAULT_TOKEN_ALLOWANCE,
        };
        record.store(connection).await?;
        Ok(record)
    }

    #[cfg(test)]
    fn user_id(&self) -> &UserId {
        self.credential.identity()
    }
}

pub(crate) mod persistence {
    use aircommon::credentials::persistence::FlatClientCredential;
    use sqlx::{
        PgExecutor, query,
        types::chrono::{DateTime, Utc},
    };

    use super::*;

    impl ClientRecord {
        pub(super) async fn store(
            &self,
            connection: impl PgExecutor<'_>,
        ) -> Result<(), StorageError> {
            let activity_time = DateTime::<Utc>::from(self.activity_time);
            let client_credential = FlatClientCredential::new(&self.credential);
            let user_id = self.credential.identity();
            query!(
                "INSERT INTO as_client_record (
                    user_uuid,
                    user_domain,
                    activity_time,
                    credential,
                    remaining_tokens
                ) VALUES ($1, $2, $3, $4, $5)",
                user_id.uuid(),
                user_id.domain() as _,
                activity_time,
                client_credential as FlatClientCredential,
                self.token_allowance,
            )
            .execute(connection)
            .await?;
            Ok(())
        }

        pub(in crate::auth_service) async fn update(
            &self,
            connection: impl PgExecutor<'_>,
        ) -> Result<(), StorageError> {
            let activity_time = DateTime::<Utc>::from(self.activity_time);
            let client_credential = FlatClientCredential::new(&self.credential);
            let user_id = self.credential.identity();
            query!(
                "UPDATE as_client_record SET
                    activity_time = $1,
                    credential = $2,
                    remaining_tokens = $3
                WHERE user_uuid = $4 AND user_domain = $5",
                activity_time,
                client_credential as FlatClientCredential,
                self.token_allowance,
                user_id.uuid(),
                user_id.domain() as _,
            )
            .execute(connection)
            .await?;
            Ok(())
        }

        pub(in crate::auth_service) async fn load(
            connection: impl PgExecutor<'_>,
            user_id: &UserId,
        ) -> Result<Option<ClientRecord>, StorageError> {
            query!(
                r#"SELECT
                    activity_time,
                    credential AS "credential: FlatClientCredential",
                    remaining_tokens
                FROM as_client_record
                WHERE user_uuid = $1 AND user_domain = $2"#,
                user_id.uuid(),
                user_id.domain() as _,
            )
            .fetch_optional(connection)
            .await?
            .map(|record| {
                Ok(ClientRecord {
                    activity_time: record.activity_time.into(),
                    credential: record.credential.into_client_credential(user_id.clone()),
                    token_allowance: record.remaining_tokens,
                })
            })
            .transpose()
        }

        #[allow(dead_code)]
        pub(in crate::auth_service) async fn delete(
            connection: impl PgExecutor<'_>,
            user_id: &UserId,
        ) -> Result<(), StorageError> {
            query!(
                "DELETE FROM as_client_record WHERE user_uuid = $1",
                user_id.uuid(),
            )
            .execute(connection)
            .await?;
            Ok(())
        }

        /// Return the client credentials of a user for a given username.
        #[allow(dead_code)]
        pub(in crate::auth_service) async fn load_user_credentials(
            connection: impl PgExecutor<'_>,
            user_id: &UserId,
        ) -> Result<Vec<ClientCredential>, StorageError> {
            let credentials = sqlx::query_scalar!(
                r#"SELECT credential as "client_credential: FlatClientCredential"
                FROM as_client_record
                WHERE user_uuid = $1 AND user_domain = $2"#,
                user_id.uuid(),
                user_id.domain() as _,
            )
            .fetch_all(connection)
            .await?;
            let credentials = credentials
                .into_iter()
                .map(|flat_credential| flat_credential.into_client_credential(user_id.clone()))
                .collect();
            Ok(credentials)
        }
    }

    #[cfg(test)]
    pub(crate) mod tests {
        use aircommon::{
            credentials::{ClientCredentialCsr, ClientCredentialPayload},
            crypto::{hash::Hash, signatures::signable::Signature},
            time::{Duration, ExpirationData},
        };
        use mls_assist::openmls::prelude::SignatureScheme;
        use sqlx::PgPool;

        use crate::auth_service::user_record::persistence::tests::store_random_user_record;

        use super::*;

        pub(crate) async fn store_random_client_record(
            pool: &PgPool,
            user_id: UserId,
        ) -> anyhow::Result<ClientRecord> {
            let record = random_client_record(user_id)?;
            record.store(pool).await?;
            Ok(record)
        }

        pub(crate) fn random_client_record(user_id: UserId) -> Result<ClientRecord, anyhow::Error> {
            let (csr, _) = ClientCredentialCsr::new(user_id, SignatureScheme::ED25519)?;
            let expiration_data = ExpirationData::new(Duration::days(90));
            let record = ClientRecord {
                activity_time: TimeStamp::now(),
                credential: ClientCredential::new(
                    ClientCredentialPayload::new(
                        csr,
                        Some(expiration_data),
                        Hash::new_for_test(b"fingerprint".to_vec()),
                    ),
                    Signature::new_for_test(b"signature".to_vec()),
                ),
                token_allowance: 42,
            };
            Ok(record)
        }

        #[sqlx::test]
        async fn load(pool: PgPool) -> anyhow::Result<()> {
            let user_record = store_random_user_record(&pool).await?;
            let client_record =
                store_random_client_record(&pool, user_record.user_id().clone()).await?;

            let loaded = ClientRecord::load(&pool, client_record.user_id())
                .await?
                .expect("missing client record");
            assert_eq!(loaded, client_record);

            Ok(())
        }

        #[sqlx::test]
        async fn load_user_credentials(pool: PgPool) -> anyhow::Result<()> {
            let user_record = store_random_user_record(&pool).await?;
            let client_record =
                store_random_client_record(&pool, user_record.user_id().clone()).await?;
            let loaded = ClientRecord::load_user_credentials(&pool, user_record.user_id()).await?;
            assert_eq!(loaded, [client_record.credential]);
            Ok(())
        }

        #[sqlx::test]
        async fn update(pool: PgPool) -> anyhow::Result<()> {
            let user_record = store_random_user_record(&pool).await?;
            let client_record =
                store_random_client_record(&pool, user_record.user_id().clone()).await?;

            let loaded = ClientRecord::load(&pool, client_record.user_id())
                .await?
                .expect("missing client record");
            assert_eq!(loaded, client_record);

            let updated_client_record = random_client_record(client_record.user_id().clone())?;

            updated_client_record.update(&pool).await?;
            let loaded = ClientRecord::load(&pool, client_record.user_id())
                .await?
                .expect("missing client record");
            assert_eq!(loaded, updated_client_record);

            Ok(())
        }

        #[sqlx::test]
        async fn delete(pool: PgPool) -> anyhow::Result<()> {
            let user_record = store_random_user_record(&pool).await?;
            let client_record =
                store_random_client_record(&pool, user_record.user_id().clone()).await?;

            let loaded = ClientRecord::load(&pool, client_record.user_id())
                .await?
                .expect("missing client record");
            assert_eq!(loaded, client_record);

            ClientRecord::delete(&pool, client_record.user_id()).await?;

            let loaded = ClientRecord::load(&pool, client_record.user_id()).await?;
            assert!(loaded.is_none());

            Ok(())
        }
    }
}
