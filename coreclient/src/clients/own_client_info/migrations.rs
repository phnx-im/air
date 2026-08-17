// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use sqlx::query;

use crate::db::access::{ReadConnection, WriteConnection};

/// A one-time client DB migration that must not rerun on every open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::VariantArray)]
#[repr(i64)]
pub(crate) enum OwnClientMigration {
    BackfillClientId = 0,
    PurgedStaleDeletedMessages = 1,
}

pub(crate) struct OwnClientMigrations;

impl OwnClientMigrations {
    pub(crate) async fn is_done(
        mut read: impl ReadConnection,
        migration: OwnClientMigration,
    ) -> sqlx::Result<bool> {
        let migration = migration as i64;
        let found = query!(
            "SELECT 1 AS found FROM own_client_migration WHERE migration = ?",
            migration,
        )
        .fetch_optional(read.as_mut())
        .await?
        .is_some();
        Ok(found)
    }

    pub(crate) async fn mark_done(
        mut write: impl WriteConnection,
        migration: OwnClientMigration,
    ) -> sqlx::Result<()> {
        let migration = migration as i64;
        query!(
            "INSERT INTO own_client_migration (user_uuid, user_domain, migration)
            SELECT user_uuid, user_domain, ? FROM own_client_info",
            migration,
        )
        .execute(write.as_mut())
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use aircommon::identifiers::{QsClientId, QsUserId, UserId};
    use sqlx::SqlitePool;
    use uuid::Uuid;

    use super::*;
    use crate::{clients::own_client_info::OwnClientInfo, db::access::DbAccess};

    #[sqlx::test]
    async fn is_done_and_mark_done(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let mut rng = rand::rng();
        OwnClientInfo {
            qs_user_id: QsUserId::random(),
            qs_client_id: QsClientId::random(&mut rng),
            user_id: UserId::new(Uuid::new_v4(), "localhost".parse().unwrap()),
            client_id: Uuid::new_v4(),
            self_group_id: None,
            self_group_signing_key: None,
        }
        .store(pool.write().await?)
        .await?;

        assert!(
            !OwnClientMigrations::is_done(pool.read().await?, OwnClientMigration::BackfillClientId)
                .await?
        );

        OwnClientMigrations::mark_done(pool.write().await?, OwnClientMigration::BackfillClientId)
            .await?;
        assert!(
            OwnClientMigrations::is_done(pool.read().await?, OwnClientMigration::BackfillClientId)
                .await?
        );

        Ok(())
    }
}
