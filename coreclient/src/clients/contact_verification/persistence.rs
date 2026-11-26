// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::UserCredential,
    crypto::hash::Hash,
    identifiers::{Fqdn, UserId},
};
use chrono::{DateTime, Utc};
use sqlx::{query, query_as};
use uuid::Uuid;

use crate::db::access::{ReadConnection, WriteConnection};

use super::ContactVerification;

struct SqlContactVerification {
    user_uuid: Uuid,
    user_domain: Fqdn,
    credential_fingerprint: Hash<UserCredential>,
    verified_at: DateTime<Utc>,
}

impl From<SqlContactVerification> for ContactVerification {
    fn from(
        SqlContactVerification {
            user_uuid,
            user_domain,
            credential_fingerprint,
            verified_at,
        }: SqlContactVerification,
    ) -> Self {
        Self {
            user_id: UserId::new(user_uuid, user_domain),
            credential_fingerprint,
            verified_at,
        }
    }
}

impl ContactVerification {
    pub(crate) async fn load(
        mut connection: impl ReadConnection,
        user_id: &UserId,
    ) -> sqlx::Result<Option<Self>> {
        let uuid = user_id.uuid();
        let domain = user_id.domain();
        let record = query_as!(
            SqlContactVerification,
            r#"SELECT
                    user_uuid AS "user_uuid: _",
                    user_domain AS "user_domain: _",
                    credential_fingerprint AS "credential_fingerprint: _",
                    verified_at AS "verified_at: _"
                FROM contact_verification
                WHERE user_uuid = ?1 AND user_domain = ?2"#,
            uuid,
            domain,
        )
        .fetch_optional(connection.as_mut())
        .await?;
        Ok(record.map(From::from))
    }

    /// Every stored mark, sorted by user id.
    pub(crate) async fn load_all(mut connection: impl ReadConnection) -> sqlx::Result<Vec<Self>> {
        let records = query_as!(
            SqlContactVerification,
            r#"SELECT
                    user_uuid AS "user_uuid: _",
                    user_domain AS "user_domain: _",
                    credential_fingerprint AS "credential_fingerprint: _",
                    verified_at AS "verified_at: _"
                FROM contact_verification
                ORDER BY user_uuid, user_domain"#
        )
        .fetch_all(connection.as_mut())
        .await?;
        Ok(records.into_iter().map(From::from).collect())
    }

    /// Stores the mark, replacing an existing one for the same user.
    pub(crate) async fn store(&self, mut connection: impl WriteConnection) -> sqlx::Result<()> {
        let uuid = self.user_id.uuid();
        let domain = self.user_id.domain();
        query!(
            "INSERT INTO contact_verification (
                    user_uuid,
                    user_domain,
                    credential_fingerprint,
                    verified_at
                ) VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT (user_uuid, user_domain) DO UPDATE SET
                    credential_fingerprint = excluded.credential_fingerprint,
                    verified_at = excluded.verified_at",
            uuid,
            domain,
            self.credential_fingerprint,
            self.verified_at,
        )
        .execute(connection.as_mut())
        .await?;
        connection.notifier().update(self.user_id.clone());
        Ok(())
    }

    pub(crate) async fn delete(
        mut connection: impl WriteConnection,
        user_id: &UserId,
    ) -> sqlx::Result<()> {
        let uuid = user_id.uuid();
        let domain = user_id.domain();
        query!(
            "DELETE FROM contact_verification WHERE user_uuid = ?1 AND user_domain = ?2",
            uuid,
            domain,
        )
        .execute(connection.as_mut())
        .await?;
        connection.notifier().update(user_id.clone());
        Ok(())
    }
}
