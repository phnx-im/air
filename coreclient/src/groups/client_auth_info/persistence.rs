// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    identifiers::{Fqdn, UserId},
    utils::removed_clients,
};
use anyhow::anyhow;
use openmls::{group::GroupId, prelude::LeafNodeIndex};
use sqlx::{Row, SqliteConnection, SqliteExecutor, query, query_as, query_scalar};
use tokio_stream::StreamExt;
use uuid::Uuid;

use crate::{
    groups::Group,
    utils::persistence::{GroupIdRefWrapper, GroupIdWrapper},
};

use super::{GroupMembership, StorableClientCredential};

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

struct SqlGroupMembership {
    group_id: GroupIdWrapper,
    user_uuid: Uuid,
    user_domain: Fqdn,
    leaf_index: u32,
}

impl From<SqlGroupMembership> for GroupMembership {
    fn from(
        SqlGroupMembership {
            group_id: GroupIdWrapper(group_id),
            user_uuid,
            user_domain,
            leaf_index,
        }: SqlGroupMembership,
    ) -> Self {
        Self {
            user_id: UserId::new(user_uuid, user_domain),
            group_id,
            leaf_index: LeafNodeIndex::new(leaf_index),
        }
    }
}

impl GroupMembership {
    /// Merge all staged group memberships for the given group id.
    pub(in crate::groups) async fn merge_for_group(
        connection: &mut sqlx::SqliteConnection,
        group_id: &GroupId,
    ) -> sqlx::Result<()> {
        let group_id = GroupIdRefWrapper::from(group_id);

        // Delete all 'staged_removal' rows.
        query!(
            "DELETE FROM group_membership
            WHERE group_id = ? AND status = 'staged_removal'",
            group_id,
        )
        .execute(&mut *connection)
        .await?;

        // Move modified information from 'staged_update' rows to their
        // 'merged' counterparts (i.e. rows with the same group_id and
        // user_id).
        query!(
            "UPDATE group_membership AS merged
            SET leaf_index = staged.leaf_index
            FROM group_membership AS staged
            WHERE merged.group_id = staged.group_id
              AND merged.user_uuid = staged.user_uuid
              AND merged.user_domain = staged.user_domain
              AND merged.status = 'merged'
              AND staged.status = 'staged_update'"
        )
        .execute(&mut *connection)
        .await?;

        // Delete all (previously merged) 'staged_update' rows.
        query!(
            "DELETE FROM group_membership
            WHERE group_id = ? AND status = 'staged_update'",
            group_id,
        )
        .execute(&mut *connection)
        .await?;

        // Mark all 'staged_add' rows as 'merged'.
        query!(
            "UPDATE group_membership SET status = 'merged'
            WHERE group_id = ? AND status = 'staged_add'",
            group_id,
        )
        .execute(connection)
        .await?;

        Ok(())
    }

    pub(crate) async fn store(&self, executor: impl SqliteExecutor<'_>) -> sqlx::Result<()> {
        let uuid = self.user_id.uuid();
        let domain = self.user_id.domain();
        let sql_group_id = self.sql_group_id();
        let leaf_index = self.leaf_index.u32();
        query!(
            "INSERT OR IGNORE INTO group_membership (
                user_uuid,
                user_domain,
                group_id,
                leaf_index,
                status
            ) VALUES (?, ?, ?, ?, 'merged')",
            uuid,
            domain,
            sql_group_id,
            leaf_index,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub(super) async fn stage_update(&self, executor: impl SqliteExecutor<'_>) -> sqlx::Result<()> {
        let uuid = self.user_id.uuid();
        let domain = self.user_id.domain();
        let sql_group_id = self.sql_group_id();
        let leaf_index = self.leaf_index.u32();
        query!(
            "INSERT INTO group_membership (
                user_uuid,
                user_domain,
                group_id,
                leaf_index,
                status
            )
            VALUES (?, ?, ?, ?, 'staged_update')",
            uuid,
            domain,
            sql_group_id,
            leaf_index,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub(super) async fn stage_add(&self, executor: impl SqliteExecutor<'_>) -> sqlx::Result<()> {
        let uuid = self.user_id.uuid();
        let domain = self.user_id.domain();
        let sql_group_id = self.sql_group_id();
        let leaf_index = self.leaf_index.u32();
        query!(
            "INSERT INTO group_membership (
                user_uuid,
                user_domain,
                group_id,
                leaf_index,
                status
            )
            VALUES (?, ?, ?, ?, 'staged_add')",
            uuid,
            domain,
            sql_group_id,
            leaf_index,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    pub(in crate::groups) async fn stage_removals_in_pending_commit(
        connection: &mut SqliteConnection,
        group: &Group,
    ) -> anyhow::Result<()> {
        for removed_client in removed_clients(
            group
                .mls_group
                .pending_commit()
                .ok_or(anyhow!("No pending commit after commit operation"))?,
        ) {
            Self::stage_removal(&mut *connection, group.group_id(), removed_client).await?;
        }
        Ok(())
    }

    pub(in crate::groups) async fn stage_removal(
        executor: impl SqliteExecutor<'_>,
        group_id: &GroupId,
        removed_index: LeafNodeIndex,
    ) -> sqlx::Result<()> {
        let group_id = GroupIdRefWrapper::from(group_id);
        let removed_index = removed_index.u32();
        query!(
            "UPDATE group_membership SET status = 'staged_removal'
            WHERE group_id = ? AND leaf_index = ?",
            group_id,
            removed_index,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    /// Load the [`GroupMembership`] for the given group_id and leaf_index from the
    /// database. Only loads merged group memberships. Use `load_staged` to load
    /// an staged group membership.
    pub(in crate::groups) async fn load(
        executor: impl SqliteExecutor<'_>,
        group_id: &GroupId,
        leaf_index: LeafNodeIndex,
    ) -> sqlx::Result<Option<Self>> {
        Self::load_internal(executor, group_id, leaf_index, true).await
    }

    /// Load the [`GroupMembership`] for the given group_id and leaf_index from the
    /// database. Only loads staged group memberships. Use `load` to load
    /// a merged group membership.
    pub(super) async fn load_staged(
        executor: impl SqliteExecutor<'_>,
        group_id: &GroupId,
        leaf_index: LeafNodeIndex,
    ) -> sqlx::Result<Option<Self>> {
        Self::load_internal(executor, group_id, leaf_index, false).await
    }

    async fn load_internal(
        executor: impl SqliteExecutor<'_>,
        group_id: &GroupId,
        leaf_index: LeafNodeIndex,
        merged: bool,
    ) -> sqlx::Result<Option<Self>> {
        let group_id = GroupIdRefWrapper::from(group_id);
        let leaf_index = leaf_index.u32();
        let sql_group_membership = if merged {
            query_as!(
                SqlGroupMembership,
                r#"SELECT
                    group_id AS "group_id: _",
                    user_uuid AS "user_uuid: _",
                    user_domain AS "user_domain: _",
                    leaf_index AS "leaf_index: _"
                FROM group_membership
                WHERE group_id = ? AND leaf_index = ?
                AND status = 'merged'"#,
                group_id,
                leaf_index,
            )
            .fetch_optional(executor)
            .await
        } else {
            query_as!(
                SqlGroupMembership,
                r#"SELECT
                    group_id AS "group_id: _",
                    user_uuid AS "user_uuid: _",
                    user_domain AS "user_domain: _",
                    leaf_index AS "leaf_index: _"
                FROM group_membership
                WHERE group_id = ? AND leaf_index = ?
                AND status LIKE 'staged_%'"#,
                group_id,
                leaf_index,
            )
            .fetch_optional(executor)
            .await
        };

        sql_group_membership.map(|res| res.map(From::from))
    }

    /// Returns a vector of all leaf indices occupied by (merged) group members
    /// that were not staged for removal.
    pub(super) async fn member_indices(
        executor: impl SqliteExecutor<'_>,
        group_id: &GroupId,
    ) -> sqlx::Result<Vec<LeafNodeIndex>> {
        let group_id = GroupIdRefWrapper::from(group_id);
        query_scalar!(
            r#"SELECT
                leaf_index AS "leaf_index: _"
            FROM group_membership WHERE group_id = ?
            AND NOT (status = 'staged_removal'
                OR status = 'staged_add'
                OR status = 'staged_update')"#,
            group_id
        )
        .fetch(executor)
        .map(|res| res.map(LeafNodeIndex::new))
        .collect()
        .await
    }

    pub(in crate::groups) async fn client_indices(
        executor: impl SqliteExecutor<'_>,
        group_id: &GroupId,
        user_ids: &[UserId],
    ) -> sqlx::Result<Vec<LeafNodeIndex>> {
        let placeholders = user_ids
            .iter()
            .map(|_| "(?, ?)")
            .collect::<Vec<_>>()
            .join(",");
        let query_string = format!(
            "SELECT leaf_index FROM group_membership
            WHERE group_id = ? AND (user_uuid, user_domain) IN ({placeholders})"
        );

        let mut query = sqlx::query(&query_string).bind(GroupIdRefWrapper::from(group_id));
        for user_id in user_ids {
            query = query.bind(user_id.uuid()).bind(user_id.domain());
        }

        query
            .fetch(executor)
            .map(|row| {
                let leaf_index: u32 = row?.try_get(0)?;
                Ok(LeafNodeIndex::new(leaf_index))
            })
            .collect()
            .await
    }

    pub(in crate::groups) async fn group_members(
        executor: impl SqliteExecutor<'_>,
        group_id: &GroupId,
    ) -> sqlx::Result<Vec<UserId>> {
        struct SqlGroupMember {
            user_uuid: Uuid,
            user_domain: Fqdn,
        }

        let group_id = GroupIdRefWrapper::from(group_id);
        query_as!(
            SqlGroupMember,
            r#"SELECT
                user_uuid AS "user_uuid: _",
                user_domain AS "user_domain: _"
            FROM group_membership WHERE group_id = ?"#,
            group_id
        )
        .fetch(executor)
        .map(|res| {
            let SqlGroupMember {
                user_uuid,
                user_domain,
            } = res?;
            Ok(UserId::new(user_uuid, user_domain))
        })
        .collect()
        .await
    }

    fn sql_group_id(&self) -> GroupIdRefWrapper<'_> {
        GroupIdRefWrapper::from(&self.group_id)
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

    const TEST_GROUP_ID: [u8; 32] = [0u8; 32];

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

    async fn store_dummy_group(executor: impl SqliteExecutor<'_>) -> sqlx::Result<()> {
        let group_id = GroupId::from_slice(&TEST_GROUP_ID);
        let group_id_wrapper = GroupIdRefWrapper::from(&group_id);
        query(
            r#"INSERT OR IGNORE INTO "group" (
                group_id, 
                identity_link_wrapper_key, 
                group_state_ear_key, 
                pending_diff, 
                room_state
            ) VALUES (?, ?, ?, ?, ?)"#,
        )
        .bind(group_id_wrapper)
        .bind(vec![0u8; 32])
        .bind(vec![0u8; 32])
        .bind(None::<Vec<u8>>)
        .bind(vec![0u8; 32])
        .execute(executor)
        .await?;
        Ok(())
    }

    /// Returns test group membership with a given parameter and fixed group id.
    fn test_group_membership(
        credential: &ClientCredential,
        index: LeafNodeIndex,
    ) -> GroupMembership {
        let group_id = GroupId::from_slice(&TEST_GROUP_ID);

        GroupMembership::new(credential.identity().clone(), group_id, index)
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
    async fn group_membership_merge_for_group(pool: SqlitePool) -> anyhow::Result<()> {
        store_dummy_group(&pool).await?;

        let credential_add = test_client_credential(Uuid::new_v4());
        credential_add.store(&pool).await?;
        let index_add = LeafNodeIndex::new(0);
        let membership_add = test_group_membership(&credential_add, index_add);
        membership_add.stage_add(&pool).await?;

        let credential_update = test_client_credential(Uuid::new_v4());
        credential_update.store(&pool).await?;
        let index_update = LeafNodeIndex::new(1);
        let membership_update = test_group_membership(&credential_update, index_update);
        membership_update.store(&pool).await?;
        membership_update.stage_update(&pool).await?;

        let credential_remove = test_client_credential(Uuid::new_v4());
        credential_remove.store(&pool).await?;
        let index_remove = LeafNodeIndex::new(2);
        let membership_remove = test_group_membership(&credential_remove, index_remove);
        membership_remove.store(&pool).await?;
        GroupMembership::stage_removal(&pool, &membership_remove.group_id, index_remove).await?;

        let credential = test_client_credential(Uuid::new_v4());
        credential.store(&pool).await?;
        let index = LeafNodeIndex::new(3);
        let membership = test_group_membership(&credential, index);
        membership.store(&pool).await?;

        GroupMembership::merge_for_group(pool.acquire().await?.as_mut(), &membership.group_id)
            .await?;

        let group_id = &membership.group_id;

        let loaded = GroupMembership::load(&pool, group_id, index_add)
            .await?
            .expect("missing membership");
        assert_eq!(loaded, membership_add);
        let loaded = GroupMembership::load(&pool, group_id, index_update)
            .await?
            .expect("missing membership");
        assert_eq!(loaded, membership_update);
        let loaded = GroupMembership::load(&pool, group_id, index_remove).await?;
        assert_eq!(loaded, None);
        let loaded = GroupMembership::load(&pool, group_id, index)
            .await?
            .expect("missing membership");
        assert_eq!(loaded, membership);

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

    #[sqlx::test]
    async fn group_membership_store_load(pool: SqlitePool) -> anyhow::Result<()> {
        store_dummy_group(&pool).await?;

        let credential = test_client_credential(Uuid::new_v4());
        credential.store(&pool).await?;

        let index = LeafNodeIndex::new(0);
        let membership = test_group_membership(&credential, index);

        membership.store(&pool).await?;
        let loaded = GroupMembership::load(&pool, &membership.group_id, index)
            .await?
            .expect("missing membership");
        assert_eq!(loaded, membership);
        let loaded = GroupMembership::load_staged(&pool, &membership.group_id, index).await?;
        assert_eq!(loaded, None);

        Ok(())
    }

    #[sqlx::test]
    async fn group_membership_store_load_staged(pool: SqlitePool) -> anyhow::Result<()> {
        store_dummy_group(&pool).await?;
        let credential = test_client_credential(Uuid::new_v4());

        credential.store(&pool).await?;

        let index = LeafNodeIndex::new(0);
        let membership = test_group_membership(&credential, index);

        membership.stage_add(pool.acquire().await?.as_mut()).await?;
        let loaded = GroupMembership::load(&pool, &membership.group_id, index).await?;
        assert_eq!(loaded, None);
        let loaded = GroupMembership::load_staged(&pool, &membership.group_id, index)
            .await?
            .expect("missing membership");
        assert_eq!(loaded, membership);

        Ok(())
    }

    #[sqlx::test]
    async fn group_membership_member_indices(pool: SqlitePool) -> anyhow::Result<()> {
        store_dummy_group(&pool).await?;

        let credential = test_client_credential(Uuid::new_v4());
        credential.store(&pool).await?;

        let index = LeafNodeIndex::new(0);
        let membership = test_group_membership(&credential, index);

        membership.store(&pool).await?;
        let indices = GroupMembership::member_indices(&pool, &membership.group_id).await?;
        assert_eq!(indices, [index]);

        Ok(())
    }

    #[sqlx::test]
    async fn group_membership_client_indices(pool: SqlitePool) -> anyhow::Result<()> {
        store_dummy_group(&pool).await?;

        let credential_a = test_client_credential(Uuid::new_v4());
        credential_a.store(&pool).await?;

        let credential_b = test_client_credential(Uuid::new_v4());
        credential_b.store(&pool).await?;

        let index_a = LeafNodeIndex::new(0);
        let membership_a = test_group_membership(&credential_a, index_a);

        let index_b = LeafNodeIndex::new(1);
        let membership_b = test_group_membership(&credential_b, index_b);

        membership_a.store(&pool).await?;
        membership_b.store(&pool).await?;

        let indices = GroupMembership::client_indices(
            &pool,
            &membership_a.group_id,
            &[credential_a.identity().clone()],
        )
        .await?;
        assert_eq!(indices, vec![index_a]);

        let indices = GroupMembership::client_indices(
            &pool,
            &membership_b.group_id,
            &[credential_b.identity().clone()],
        )
        .await?;
        assert_eq!(indices, [index_b]);

        Ok(())
    }

    #[sqlx::test]
    async fn group_membership_group_members(pool: SqlitePool) -> anyhow::Result<()> {
        store_dummy_group(&pool).await?;

        let credential_a = test_client_credential(Uuid::new_v4());
        credential_a.store(&pool).await?;

        let credential_b = test_client_credential(Uuid::new_v4());
        credential_b.store(&pool).await?;

        let index_a = LeafNodeIndex::new(0);
        let membership_a = test_group_membership(&credential_a, index_a);

        let index_b = LeafNodeIndex::new(1);
        let membership_b = test_group_membership(&credential_b, index_b);

        membership_a.store(&pool).await?;
        membership_b.store(&pool).await?;

        let members = GroupMembership::group_members(&pool, &membership_a.group_id).await?;
        assert_eq!(
            members,
            [
                credential_a.identity().clone(),
                credential_b.identity().clone()
            ]
        );

        Ok(())
    }
}
