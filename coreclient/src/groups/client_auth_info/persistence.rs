// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{credentials::UserCredential, identifiers::UserId};
use sqlx::{query, query_scalar};

use crate::db::access::{ReadConnection, WriteConnection};

use super::StorableUserCredential;

impl StorableUserCredential {
    pub(crate) async fn load_by_user_id(
        mut connection: impl ReadConnection,
        user_id: &UserId,
    ) -> sqlx::Result<Option<Self>> {
        let uuid = user_id.uuid();
        let domain = user_id.domain();
        query_scalar!(
            r#"SELECT
                user_credential AS "user_credential: _"
            FROM user_credential
            WHERE user_uuid = ? AND user_domain = ?"#,
            uuid,
            domain,
        )
        .fetch_optional(connection.as_mut())
        .await
        .map(|res| res.map(StorableUserCredential::new))
    }

    /// Pins `self` as the user credential of its user id.
    ///
    /// The first credential stored for a user id pins its verifying key. A later credential with
    /// the same key leaves the stored one in place. A credential with another key is rejected
    /// with [`UserCredentialKeyChanged`], since no flow changes a user's key.
    pub(crate) async fn pin(&self, mut connection: impl WriteConnection) -> anyhow::Result<()> {
        match Self::check_pin(&mut connection, self).await? {
            PinCheck::Unpinned => {}
            PinCheck::Matches => return Ok(()),
            PinCheck::Conflicts => return Err(UserCredentialKeyChanged.into()),
        }
        let fingerprint = self.fingerprint();
        let user_id = self.user_credential.user_id();
        let uuid = user_id.uuid();
        let domain = user_id.domain();
        query!(
            "INSERT INTO user_credential
                (fingerprint, user_uuid, user_domain, user_credential) VALUES (?, ?, ?, ?)",
            fingerprint,
            uuid,
            domain,
            self.user_credential,
        )
        .execute(connection.as_mut())
        .await?;
        Ok(())
    }

    /// Compares the verifying key of `credential` with the key pinned for its user id.
    pub(crate) async fn check_pin(
        connection: impl ReadConnection,
        credential: &UserCredential,
    ) -> sqlx::Result<PinCheck> {
        let pinned = Self::load_by_user_id(connection, credential.user_id()).await?;
        Ok(match pinned {
            None => PinCheck::Unpinned,
            Some(pinned) if pinned.verifying_key() == credential.verifying_key() => {
                PinCheck::Matches
            }
            Some(_) => PinCheck::Conflicts,
        })
    }
}

/// How a user credential relates to the key pinned for its user id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PinCheck {
    /// No credential is stored for the user id.
    Unpinned,
    /// The credential carries the pinned key.
    Matches,
    /// The credential carries another key than the pinned one.
    Conflicts,
}

/// A user credential carries another verifying key than the one pinned for its user id.
#[derive(Debug, thiserror::Error)]
#[error("user credential carries another key than the one pinned for its user id")]
pub(crate) struct UserCredentialKeyChanged;

#[cfg(test)]
mod tests {
    use aircommon::{
        credentials::{
            AsIntermediateCredentialBody, UserCredential, UserCredentialCsr, UserCredentialPayload,
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

    use crate::db::access::DbAccess;

    use super::*;

    fn test_csr(user_uuid: Uuid) -> UserCredentialCsr {
        let user_id = UserId::new(user_uuid, "localhost".parse().unwrap());
        let (user_credential_csr, _) =
            UserCredentialCsr::new(user_id, SignatureScheme::ED25519).unwrap();
        user_credential_csr
    }

    fn sign_for_test(csr: UserCredentialCsr, signature: &[u8]) -> StorableUserCredential {
        let fingerprint =
            Hash::<AsIntermediateCredentialBody>::new_for_test(b"fingerprint".to_vec());
        let user_credential = UserCredential::from_payload(
            UserCredentialPayload::new(csr, None, fingerprint),
            Signature::new_for_test(signature.to_vec()),
        );
        StorableUserCredential { user_credential }
    }

    /// Returns a test credential for `user_uuid` with a fresh key.
    fn test_user_credential(user_uuid: Uuid) -> StorableUserCredential {
        sign_for_test(test_csr(user_uuid), b"signature")
    }

    async fn load(pool: &DbAccess, user_id: &UserId) -> anyhow::Result<Vec<u8>> {
        let loaded = StorableUserCredential::load_by_user_id(pool.read().await?, user_id)
            .await?
            .expect("missing credential");
        Ok(loaded.user_credential.tls_serialize_detached()?)
    }

    #[sqlx::test]
    async fn pin_store_load(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let credential = test_user_credential(Uuid::new_v4());

        credential.pin(pool.write().await?).await?;

        assert_eq!(
            load(&pool, credential.user_id()).await?,
            credential.user_credential.tls_serialize_detached()?
        );
        Ok(())
    }

    #[sqlx::test]
    async fn pin_is_idempotent(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let credential = test_user_credential(Uuid::new_v4());

        credential.pin(pool.write().await?).await?;
        credential.pin(pool.write().await?).await?;

        assert_eq!(
            load(&pool, credential.user_id()).await?,
            credential.user_credential.tls_serialize_detached()?
        );
        Ok(())
    }

    #[sqlx::test]
    async fn pin_keeps_the_first_credential_for_the_same_key(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let csr = test_csr(Uuid::new_v4());
        let first = sign_for_test(csr.clone(), b"first");
        let reissued = sign_for_test(csr, b"reissued");
        assert_ne!(first.fingerprint(), reissued.fingerprint());

        first.pin(pool.write().await?).await?;
        reissued.pin(pool.write().await?).await?;

        assert_eq!(
            load(&pool, first.user_id()).await?,
            first.user_credential.tls_serialize_detached()?
        );
        Ok(())
    }

    #[sqlx::test]
    async fn pin_rejects_another_key_for_the_same_user_id(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let id = Uuid::new_v4();
        let first = test_user_credential(id);
        let second = test_user_credential(id);
        assert_ne!(first.verifying_key(), second.verifying_key());

        first.pin(pool.write().await?).await?;
        let error = second.pin(pool.write().await?).await.unwrap_err();

        assert!(error.is::<UserCredentialKeyChanged>());
        assert_eq!(
            StorableUserCredential::check_pin(pool.read().await?, &second).await?,
            PinCheck::Conflicts
        );
        assert_eq!(
            load(&pool, first.user_id()).await?,
            first.user_credential.tls_serialize_detached()?
        );
        Ok(())
    }
}
