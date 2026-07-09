// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::{Fqdn, QsClientId, QsUserId, UserId};
use openmls::group::GroupId;
use sqlx::query;
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
        query!(
            "INSERT INTO own_client_info (
                qs_user_id,
                qs_client_id,
                user_uuid,
                user_domain,
                self_group_id
            ) VALUES (?,  ?, ?, ?, ?)",
            self.qs_user_id,
            self.qs_client_id,
            uuid,
            domain,
            self_group_id,
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
            self_group_id: Option<GroupIdWrapper>,
        }
        let sql = sqlx::query_as!(
            SqlOwnClientInfo,
            r#"SELECT
                qs_user_id AS "qs_user_id: _",
                qs_client_id AS "qs_client_id: _",
                user_uuid AS "user_uuid: _",
                user_domain AS "user_domain: _",
                self_group_id AS "self_group_id: _"
            FROM own_client_info"#,
        )
        .fetch_one(connection.as_mut())
        .await?;
        Ok(Self {
            qs_user_id: sql.qs_user_id,
            qs_client_id: sql.qs_client_id,
            user_id: UserId::new(sql.user_uuid, sql.user_domain),
            self_group_id: sql.self_group_id.map(From::from),
        })
    }

    pub(crate) async fn set_self_group_id(
        mut write: impl WriteConnection,
        group_id: &GroupId,
    ) -> sqlx::Result<()> {
        let group_id = GroupIdRefWrapper::from(group_id);
        query!("UPDATE own_client_info SET self_group_id = ?", group_id)
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
        let mut rng = rand::thread_rng();
        let own_client_info = OwnClientInfo {
            qs_user_id: QsUserId::random(),
            qs_client_id: QsClientId::random(&mut rng),
            user_id: UserId::new(Uuid::new_v4(), "localhost".parse().unwrap()),
            self_group_id: Some(GroupId::random(&RustCrypto::default())),
        };

        own_client_info.store(pool.write().await?).await?;

        let loaded = OwnClientInfo::load(pool.read().await?).await?;
        assert_eq!(loaded, own_client_info);

        Ok(())
    }
}
