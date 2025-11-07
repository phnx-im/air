// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use sqlx::query;

use super::OwnClientInfo;

impl OwnClientInfo {
    pub(crate) async fn store(&self, executor: impl sqlx::SqliteExecutor<'_>) -> sqlx::Result<()> {
        let uuid = self.user_id.uuid();
        let domain = self.user_id.domain();
        query!(
            "INSERT INTO own_client_info (
                server_url,
                qs_user_id,
                qs_client_id,
                user_uuid,
                user_domain
            ) VALUES (?, ?, ?, ?, ?)",
            self.server_url,
            self.qs_user_id,
            self.qs_client_id,
            uuid,
            domain,
        )
        .execute(executor)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use aircommon::identifiers::{QsClientId, QsUserId, UserId};
    use sqlx::{Row, SqlitePool};
    use uuid::Uuid;

    use super::*;

    #[sqlx::test]
    async fn store(pool: SqlitePool) -> anyhow::Result<()> {
        let own_client_info = OwnClientInfo {
            server_url: "https://localhost".to_string(),
            qs_user_id: QsUserId::random(),
            qs_client_id: QsClientId::random(&mut rand::thread_rng()),
            user_id: UserId::new(Uuid::new_v4(), "localhost".parse().unwrap()),
        };

        own_client_info.store(&pool).await?;

        let row = sqlx::query(
            "SELECT server_url, qs_user_id, qs_client_id, user_uuid, user_domain
            FROM own_client_info",
        )
        .fetch_one(&pool)
        .await?;
        let server_url = row.try_get(0)?;
        let qs_user_id = row.try_get(1)?;
        let qs_client_id = row.try_get(2)?;
        let user_uuid = row.try_get(3)?;
        let user_domain = row.try_get(4)?;
        let loaded = OwnClientInfo {
            server_url,
            qs_user_id,
            qs_client_id,
            user_id: UserId::new(user_uuid, user_domain),
        };

        assert_eq!(loaded, own_client_info);

        Ok(())
    }
}
