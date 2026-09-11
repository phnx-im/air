// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::UsernameHash;
use airprotos::{
    auth_service::v1::SignedConnectionPackage,
    client::signed_connection_package::VerifiedSignedConnectionPackage, common::v1::Signature,
};
use chrono::{DateTime, Utc};
use sqlx::PgExecutor;

pub(crate) struct StorableSignedConnectionPackage;

impl StorableSignedConnectionPackage {
    pub(crate) async fn store_multiple_for_username(
        connection: impl PgExecutor<'_>,
        hash: &UsernameHash,
        packages: Vec<VerifiedSignedConnectionPackage>,
    ) -> sqlx::Result<()> {
        if packages.is_empty() {
            return Ok(());
        }

        let mut payloads = Vec::with_capacity(packages.len());
        let mut signatures = Vec::with_capacity(packages.len());
        let mut is_last_resorts = Vec::with_capacity(packages.len());
        let mut expires_ats = Vec::with_capacity(packages.len());
        for package in packages {
            // Stored but not read yet. Packages are only removed on fetch or together with the
            // username.
            expires_ats.push(DateTime::<Utc>::from(package.expires_at()));
            let (payload, signature, is_last_resort) = package.into_parts();
            payloads.push(payload);
            signatures.push(signature.into_bytes());
            is_last_resorts.push(is_last_resort);
        }
        sqlx::query!(
            r#"INSERT INTO username_signed_connection_package
                (hash, payload, signature, is_last_resort, expires_at)
            SELECT $1::bytea, payload, signature, is_last_resort, expires_at
            FROM UNNEST($2::bytea[], $3::bytea[], $4::boolean[], $5::timestamptz[])
            AS t(payload, signature, is_last_resort, expires_at)
            ON CONFLICT (hash, payload) DO NOTHING"#,
            hash.as_bytes(),
            &payloads,
            &signatures,
            &is_last_resorts,
            &expires_ats,
        )
        .execute(connection)
        .await?;

        Ok(())
    }

    pub(crate) async fn load_for_username(
        connection: impl PgExecutor<'_>,
        hash: &UsernameHash,
    ) -> sqlx::Result<Option<SignedConnectionPackage>> {
        let record = sqlx::query!(
            r#"WITH next_connection_package AS (
                SELECT id, payload, signature
                FROM username_signed_connection_package
                WHERE hash = $1
                ORDER BY is_last_resort ASC
                LIMIT 1
                FOR UPDATE -- make sure two concurrent queries don't return the same package
                SKIP LOCKED -- skip rows that are already locked by other processes
            ),
            deleted_package AS (
                DELETE FROM username_signed_connection_package
                WHERE id = (SELECT id FROM next_connection_package)
                AND NOT is_last_resort
                AND (SELECT COUNT(*) FROM username_signed_connection_package WHERE hash = $1) > 1
            )
            SELECT payload, signature FROM next_connection_package"#,
            hash.as_bytes(),
        )
        .fetch_optional(connection)
        .await?;

        Ok(record.map(|record| SignedConnectionPackage {
            payload: record.payload,
            signature: Some(Signature {
                value: record.signature,
            }),
        }))
    }
}
