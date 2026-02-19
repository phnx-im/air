// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::UserId;
use sqlx::{SqliteExecutor, query, query_scalar};

use super::StorableClientCredential;

impl StorableClientCredential {
    pub(crate) async fn load_by_user_id(
        executor: impl SqliteExecutor<'_>,
        user_id: &UserId,
    ) -> sqlx::Result<Option<Self>> {
        let uuid = user_id.uuid();
        let domain = user_id.domain();
        query_scalar!(
            r#"SELECT
                client_credential AS "client_credential: _"
            FROM client_credential
            WHERE user_uuid = ? AND user_domain = ?"#,
            uuid,
            domain,
        )
        .fetch_optional(executor)
        .await
        .map(|res| res.map(StorableClientCredential::new))
    }

    /// Stores the client credential in the database if it does not already exist.
    pub(crate) async fn store(&self, executor: impl SqliteExecutor<'_>) -> sqlx::Result<()> {
        let fingerprint = self.fingerprint();
        let user_id = self.client_credential.identity();
        let uuid = user_id.uuid();
        let domain = user_id.domain();
        query!(
            "INSERT OR IGNORE INTO client_credential
                (fingerprint, user_uuid, user_domain, client_credential) VALUES (?, ?, ?, ?)",
            fingerprint,
            uuid,
            domain,
            self.client_credential,
        )
        .execute(executor)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use aircommon::{
        credentials::{
            AsIntermediateCredentialBody, ClientCredential, ClientCredentialCsr,
            ClientCredentialPayload,
        },
        crypto::{
            hash::Hash,
            signatures::signable::{Signature, SignedStruct},
        },
    };
    use openmls::prelude::SignatureScheme;
    use sqlx::SqlitePool;
    use tls_codec::Serialize;
    use uuid::Uuid;

    use super::*;

    /// Returns test credential with a fixed identity but random payload.
    fn test_client_credential(user_uuid: Uuid) -> StorableClientCredential {
        let user_id = UserId::new(user_uuid, "localhost".parse().unwrap());
        let (client_credential_csr, _) =
            ClientCredentialCsr::new(user_id, SignatureScheme::ED25519).unwrap();
        let fingerprint =
            Hash::<AsIntermediateCredentialBody>::new_for_test(b"fingerprint".to_vec());
        let client_credential = ClientCredential::from_payload(
            ClientCredentialPayload::new(client_credential_csr, None, fingerprint),
            Signature::new_for_test(b"signature".to_vec()),
        );
        StorableClientCredential { client_credential }
    }

    #[sqlx::test]
    async fn client_credential_store_load(pool: SqlitePool) -> anyhow::Result<()> {
        let credential = test_client_credential(Uuid::new_v4());

        credential.store(&pool).await?;
        let loaded = StorableClientCredential::load_by_user_id(&pool, credential.identity())
            .await?
            .expect("missing credential");
        assert_eq!(
            loaded.client_credential.tls_serialize_detached(),
            credential.client_credential.tls_serialize_detached()
        );

        Ok(())
    }

    #[sqlx::test]
    async fn client_credential_store_load_by_id(pool: SqlitePool) -> anyhow::Result<()> {
        let credential = test_client_credential(Uuid::new_v4());

        credential.store(&pool).await?;
        let loaded = StorableClientCredential::load_by_user_id(&pool, credential.identity())
            .await?
            .expect("missing credential");
        assert_eq!(
            loaded.client_credential.tls_serialize_detached(),
            credential.client_credential.tls_serialize_detached()
        );

        Ok(())
    }

    #[sqlx::test]
    async fn store_idempotent(pool: SqlitePool) -> anyhow::Result<()> {
        let id = Uuid::new_v4();
        let credential_1 = test_client_credential(id);
        let credential_2 = test_client_credential(id);

        // precondition
        assert_eq!(credential_1.identity(), credential_2.identity());
        assert_ne!(
            credential_1.tls_serialize_detached(),
            credential_2.tls_serialize_detached()
        );

        credential_1.store(&pool).await?;
        credential_2.store(&pool).await?;

        let loaded = StorableClientCredential::load_by_user_id(&pool, credential_1.identity())
            .await?
            .expect("missing credential");
        assert_eq!(
            loaded.client_credential.tls_serialize_detached(),
            credential_1.client_credential.tls_serialize_detached()
        );

        Ok(())
    }
}
