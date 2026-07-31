// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::keys::ClientSigningKey,
    identifiers::{Fqdn, QsClientId, QsUserId, UserId},
};
use openmls::group::GroupId;
use sqlx::{query, query_scalar};
use uuid::Uuid;

use crate::{
    db::access::{ReadConnection, WriteConnection},
    utils::persistence::{GroupIdRefWrapper, GroupIdWrapper},
};

use super::OwnClientInfo;

impl OwnClientInfo {
    pub(crate) async fn store(&self, mut connection: impl WriteConnection) -> sqlx::Result<()> {
        let uuid = self.user_id.uuid();
        let domain = self.user_id.domain();
        let self_group_id = self.self_group_id.as_ref().map(GroupIdRefWrapper::from);
        let self_group_signing_key = self.self_group_signing_key.as_ref();
        query!(
            "INSERT INTO own_client_info (
                qs_user_id,
                qs_client_id,
                user_uuid,
                user_domain,
                client_id,
                self_group_id,
                self_group_signing_key
            ) VALUES (?, ?, ?, ?, ?, ?, ?)",
            self.qs_user_id,
            self.qs_client_id,
            uuid,
            domain,
            self.client_id,
            self_group_id,
            self_group_signing_key
        )
        .execute(connection.as_mut())
        .await?;
        Ok(())
    }

    pub(crate) async fn load(mut connection: impl ReadConnection) -> Result<Self, sqlx::Error> {
        struct SqlOwnClientInfo {
            qs_user_id: QsUserId,
            qs_client_id: QsClientId,
            user_uuid: Uuid,
            user_domain: Fqdn,
            client_id: Uuid,
            self_group_id: Option<GroupIdWrapper>,
            self_group_signing_key: Option<ClientSigningKey>,
        }
        let sql = sqlx::query_as!(
            SqlOwnClientInfo,
            r#"SELECT
                qs_user_id AS "qs_user_id: _",
                qs_client_id AS "qs_client_id: _",
                user_uuid AS "user_uuid: _",
                user_domain AS "user_domain: _",
                client_id AS "client_id: _",
                self_group_id AS "self_group_id: _",
                self_group_signing_key AS "self_group_signing_key: _"
            FROM own_client_info"#,
        )
        .fetch_one(connection.as_mut())
        .await?;
        Ok(Self {
            qs_user_id: sql.qs_user_id,
            qs_client_id: sql.qs_client_id,
            user_id: UserId::new(sql.user_uuid, sql.user_domain),
            client_id: sql.client_id,
            self_group_id: sql.self_group_id.map(From::from),
            self_group_signing_key: sql.self_group_signing_key,
        })
    }

    /// Returns the user id of this client.
    pub(crate) async fn load_user_id(mut connection: impl ReadConnection) -> sqlx::Result<UserId> {
        struct SqlUserId {
            user_uuid: Uuid,
            user_domain: Fqdn,
        }
        let sql = sqlx::query_as!(
            SqlUserId,
            r#"SELECT
                user_uuid AS "user_uuid: _",
                user_domain AS "user_domain: _"
            FROM own_client_info"#,
        )
        .fetch_one(connection.as_mut())
        .await?;
        Ok(UserId::new(sql.user_uuid, sql.user_domain))
    }

    /// Returns the `self_group_id`.
    pub(crate) async fn load_self_group_id(
        mut connection: impl ReadConnection,
    ) -> sqlx::Result<Option<GroupId>> {
        let self_group_id: Option<GroupIdWrapper> =
            query_scalar!(r#"SELECT self_group_id AS "self_group_id: _" FROM own_client_info"#)
                .fetch_one(connection.as_mut())
                .await?;
        Ok(self_group_id.map(From::from))
    }

    /// Returns `true` if `group_id` is the user's own self group.
    pub(crate) async fn is_own_self_group(
        mut connection: impl ReadConnection,
        group_id: &GroupId,
    ) -> sqlx::Result<bool> {
        let group_id = GroupIdRefWrapper::from(group_id);
        let found = query!(
            "SELECT 1 AS found FROM own_client_info WHERE self_group_id = ?",
            group_id,
        )
        .fetch_optional(connection.as_mut())
        .await?
        .is_some();
        Ok(found)
    }

    /// Backfill a missing client id with a freshly generated one.
    ///
    /// The migration adding the column defaults it to the nil UUID for clients that existed
    /// before, since sqlite cannot generate valid UUIDs. Runs on every client DB open and is a
    /// no-op once the id is set.
    pub(crate) async fn backfill_client_id(mut write: impl WriteConnection) -> sqlx::Result<()> {
        let client_id = Uuid::new_v4();
        let nil_client_id = Uuid::nil();
        query!(
            "UPDATE own_client_info SET client_id = ? WHERE client_id = ?",
            client_id,
            nil_client_id,
        )
        .execute(write.as_mut())
        .await?;
        Ok(())
    }

    pub(crate) async fn set_self_group(
        mut write: impl WriteConnection,
        self_group_id: &GroupId,
        self_group_signing_key: &ClientSigningKey,
    ) -> sqlx::Result<()> {
        let self_group_id = GroupIdRefWrapper::from(self_group_id);
        query!(
            "UPDATE own_client_info SET self_group_id = ?, self_group_signing_key = ?",
            self_group_id,
            self_group_signing_key,
        )
        .execute(write.as_mut())
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use aircommon::{
        RustCrypto,
        identifiers::{QsClientId, QsUserId, UserId},
    };
    use openmls::group::GroupId;
    use sqlx::SqlitePool;
    use uuid::Uuid;

    use crate::db::access::DbAccess;

    use super::*;

    #[sqlx::test]
    async fn store(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let mut rng = rand::rng();
        let own_client_info = OwnClientInfo {
            qs_user_id: QsUserId::random(),
            qs_client_id: QsClientId::random(&mut rng),
            user_id: UserId::new(Uuid::new_v4(), "localhost".parse().unwrap()),
            client_id: Uuid::new_v4(),
            self_group_id: Some(GroupId::random(&RustCrypto::default())),
            self_group_signing_key: None,
        };

        own_client_info.store(pool.write().await?).await?;

        let loaded = OwnClientInfo::load(pool.read().await?).await?;
        assert_eq!(loaded.qs_user_id, own_client_info.qs_user_id);
        assert_eq!(loaded.qs_client_id, own_client_info.qs_client_id);
        assert_eq!(loaded.user_id, own_client_info.user_id);
        assert_eq!(loaded.client_id, own_client_info.client_id);
        assert_eq!(loaded.self_group_id, own_client_info.self_group_id);
        assert!(loaded.self_group_signing_key.is_none());

        Ok(())
    }

    #[sqlx::test]
    async fn backfill_client_id(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let mut rng = rand::rng();

        // A client that existed before the client-id migration: its client_id defaults to the
        // nil UUID.
        sqlx::query(
            "INSERT INTO own_client_info (qs_user_id, qs_client_id, user_uuid, user_domain)
            VALUES (?, ?, ?, ?)",
        )
        .bind(QsUserId::random())
        .bind(QsClientId::random(&mut rng))
        .bind(Uuid::new_v4())
        .bind("localhost")
        .execute(pool.write().await?.as_mut())
        .await?;

        let loaded = OwnClientInfo::load(pool.read().await?).await?;
        assert!(loaded.client_id.is_nil());

        OwnClientInfo::backfill_client_id(pool.write().await?).await?;

        let loaded = OwnClientInfo::load(pool.read().await?).await?;
        assert!(!loaded.client_id.is_nil());

        // A second run keeps the id.
        let client_id = loaded.client_id;
        OwnClientInfo::backfill_client_id(pool.write().await?).await?;
        let loaded = OwnClientInfo::load(pool.read().await?).await?;
        assert_eq!(loaded.client_id, client_id);

        Ok(())
    }
}
