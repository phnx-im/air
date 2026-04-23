// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::ops::Deref;

use aircommon::{
    codec::{BlobDecoded, BlobEncoded, PersistenceCodec},
    credentials::{ClientCredential, GroupStorageWitness, VerifiableClientCredential},
    crypto::aead::keys::{GroupStateEarKey, IdentityLinkWrapperKey},
    time::TimeStamp,
};
use anyhow::{Result, ensure};
use mimi_room_policy::{RoomState, VerifiedRoomState};
use openmls::group::{GroupId, MlsGroup};
use openmls::prelude::{LeafNodeIndex, StagedCommit};
use openmls_traits::OpenMlsProvider;
use sqlx::{SqliteTransaction, query, query_as};
use tls_codec::Serialize as _;
use tracing::error;

use crate::{
    ChatId,
    chats::messages::TimestampedMessage,
    db_access::{ReadConnection, WriteConnection, WriteDbTransaction},
    utils::persistence::{GroupIdRefWrapper, GroupIdWrapper},
};

use super::{Group, GroupDataBytes, diff::StagedGroupDiff, openmls_provider::AirOpenMlsProvider};

struct SqlGroup {
    group_id: GroupIdWrapper,
    identity_link_wrapper_key: IdentityLinkWrapperKey,
    group_state_ear_key: GroupStateEarKey,
    pending_diff: Option<BlobDecoded<StagedGroupDiff>>,
    room_state: Vec<u8>,
    self_updated_at: Option<TimeStamp>,
}

impl SqlGroup {
    fn into_group(self, mls_group: MlsGroup) -> Group {
        let Self {
            group_id: GroupIdWrapper(group_id),
            identity_link_wrapper_key,
            group_state_ear_key,
            pending_diff,
            room_state,
            self_updated_at,
        } = self;

        let room_state = if let Some(state) = PersistenceCodec::from_slice::<RoomState>(&room_state)
            .ok()
            .and_then(|state| VerifiedRoomState::verify(state).ok())
        {
            state
        } else {
            error!("Failed to load room state. Falling back to default room state.");
            let members: Vec<_> = mls_group
                .members()
                .map(|m| -> anyhow::Result<_> {
                    let credential =
                        VerifiableClientCredential::from_basic_credential(&m.credential)?;
                    Ok(credential.user_id().tls_serialize_detached()?)
                })
                .filter_map(|res| {
                    res.inspect_err(|error| {
                        error!(%error, "Failed to serialize user id for fallback room");
                    })
                    .ok()
                })
                .collect();

            VerifiedRoomState::fallback_room(members)
        };

        Group {
            group_id,
            identity_link_wrapper_key,
            group_state_ear_key,
            mls_group,
            pending_diff: pending_diff.map(|BlobDecoded(diff)| diff),
            room_state,
            self_updated_at,
        }
    }
}

/// Verification that a group was loaded from the local storage.
struct LocalGroupStorage(GroupId);

// MLS groups are only written to local storage after all leaf credentials have been verified
// against an AS intermediate credential.
impl GroupStorageWitness for LocalGroupStorage {
    fn group_id(&self) -> &GroupId {
        &self.0
    }
}

/// A [`Group`] loaded from local storage, with the guarantee that all
/// leaf credentials have been previously verified against AS credentials.
pub(crate) struct VerifiedGroup(Group);

impl VerifiedGroup {
    pub(crate) fn group_mut(&mut self) -> &mut Group {
        &mut self.0
    }

    /// Like [`Group::credential_at`] but without requiring an explicit witness argument.
    pub(crate) fn credential_at(
        &self,
        index: LeafNodeIndex,
    ) -> anyhow::Result<Option<ClientCredential>> {
        self.0.credential_at(index, self)
    }

    /// Delegates to [`Group::merge_pending_commit`] using a temporary witness.
    ///
    /// A temporary `LocalGroupStorage` is created to avoid a simultaneous
    /// `&mut self` (for the group) and `&self` (for the witness) borrow conflict.
    pub(crate) async fn merge_pending_commit(
        &mut self,
        txn: &mut SqliteTransaction<'_>,
        staged_commit_option: impl Into<Option<StagedCommit>>,
        ds_timestamp: TimeStamp,
    ) -> Result<(Vec<TimestampedMessage>, Option<GroupDataBytes>)> {
        let witness = LocalGroupStorage(self.0.group_id().clone());
        self.0
            .merge_pending_commit(txn, &witness, staged_commit_option, ds_timestamp)
            .await
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(group: Group) -> Self {
        Self(group)
    }
}

impl Deref for VerifiedGroup {
    type Target = Group;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// VerifiedGroup can only be constructed via `load_verified` / `load_clean_verified`, which load
// groups from local storage after all leaf credentials have been verified against AS intermediate
// credentials.
impl GroupStorageWitness for VerifiedGroup {
    fn group_id(&self) -> &GroupId {
        self.0.group_id()
    }
}

impl Group {
    pub(crate) async fn store(&self, mut connection: impl WriteConnection) -> sqlx::Result<()> {
        let group_id = GroupIdRefWrapper::from(&self.group_id);
        let room_state = BlobEncoded(&self.room_state);
        let pending_diff = self.pending_diff.as_ref().map(BlobEncoded);

        query!(
            r#"INSERT INTO "group" (
                group_id,
                identity_link_wrapper_key,
                group_state_ear_key,
                pending_diff,
                room_state,
                self_updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?)"#,
            group_id,
            self.identity_link_wrapper_key,
            self.group_state_ear_key,
            pending_diff,
            room_state,
            self.self_updated_at,
        )
        .execute(connection.as_mut())
        .await?;
        Ok(())
    }

    pub(crate) async fn load_clean(
        mut connection: impl ReadConnection,
        group_id: &GroupId,
    ) -> anyhow::Result<Option<Self>> {
        let Some(group) = Group::load(connection, group_id).await? else {
            return Ok(None);
        };

        ensure!(
            group.mls_group.pending_commit().is_none(),
            "Room already had a pending commit"
        );

        Ok(Some(group))
    }

    pub(crate) async fn load_clean_verified(
        connection: impl ReadConnection,
        group_id: &GroupId,
    ) -> anyhow::Result<Option<VerifiedGroup>> {
        Ok(Self::load_clean(connection, group_id)
            .await?
            .map(VerifiedGroup))
    }

    pub(crate) async fn load_with_chat_id_clean(
        connection: impl ReadConnection,
        chat_id: ChatId,
    ) -> anyhow::Result<Option<Self>> {
        let Some(group) = Group::load_with_chat_id(connection, chat_id).await? else {
            return Ok(None);
        };

        ensure!(
            group.mls_group.pending_commit().is_none(),
            "Room already had a pending commit"
        );

        Ok(Some(group))
    }

    pub(crate) async fn load_verified(
        connection: impl ReadConnection,
        group_id: &GroupId,
    ) -> sqlx::Result<Option<VerifiedGroup>> {
        Ok(Self::load(connection, group_id).await?.map(VerifiedGroup))
    }

    pub(crate) async fn load(
        mut connection: impl ReadConnection,
        group_id: &GroupId,
    ) -> sqlx::Result<Option<Self>> {
        let Some(mls_group) = MlsGroup::load(
            AirOpenMlsProvider::new(connection.as_mut()).storage(),
            group_id,
        )?
        else {
            return Ok(None);
        };
        let group_id = GroupIdRefWrapper::from(group_id);
        query_as!(
            SqlGroup,
            r#"SELECT
                group_id AS "group_id: _",
                identity_link_wrapper_key AS "identity_link_wrapper_key: _",
                group_state_ear_key AS "group_state_ear_key: _",
                pending_diff AS "pending_diff: _",
                room_state AS "room_state: _",
                self_updated_at AS "self_updated_at: _"
            FROM "group" WHERE group_id = ?"#,
            group_id
        )
        .fetch_optional(connection.as_mut())
        .await
        .map(|res| res.map(|group| SqlGroup::into_group(group, mls_group)))
    }

    /// Same as [`Self::load()`], but load the group via the corresponding chat.
    pub(crate) async fn load_with_chat_id(
        mut connection: impl ReadConnection,
        chat_id: ChatId,
    ) -> sqlx::Result<Option<Self>> {
        let Some(sql_group) = query_as!(
            SqlGroup,
            r#"SELECT
                g.group_id AS "group_id: _",
                g.identity_link_wrapper_key AS "identity_link_wrapper_key: _",
                g.group_state_ear_key AS "group_state_ear_key: _",
                g.pending_diff AS "pending_diff: _",
                g.room_state AS "room_state: _",
                g.self_updated_at AS "self_updated_at: _"
            FROM "group" g
            INNER JOIN chat c ON c.group_id = g.group_id
            WHERE c.chat_id = ?
            "#,
            chat_id
        )
        .fetch_optional(connection.as_mut())
        .await?
        else {
            return Ok(None);
        };
        let Some(mls_group) = MlsGroup::load(
            AirOpenMlsProvider::new(connection.as_mut()).storage(),
            &sql_group.group_id.0,
        )?
        else {
            return Ok(None);
        };
        Ok(Some(SqlGroup::into_group(sql_group, mls_group)))
    }

    /// Stores a group update.
    ///
    /// The parameter `self_updated_at` specifies whether the key material of the current user was
    /// updated in the group and if so, at what time.
    pub(crate) async fn store_update(
        &mut self,
        mut connection: impl WriteConnection,
        self_updated_at: Option<TimeStamp>,
    ) -> sqlx::Result<()> {
        let group_id = GroupIdRefWrapper::from(&self.group_id);
        let pending_diff = self.pending_diff.as_ref().map(BlobEncoded);
        let room_state = BlobEncoded(&self.room_state);
        query!(
            r#"UPDATE "group" SET
                identity_link_wrapper_key = ?,
                group_state_ear_key = ?,
                pending_diff = ?,
                room_state = ?,
                self_updated_at = COALESCE(?, self_updated_at)
            WHERE group_id = ?"#,
            self.identity_link_wrapper_key,
            self.group_state_ear_key,
            pending_diff,
            room_state,
            self_updated_at,
            group_id,
        )
        .execute(connection.as_mut())
        .await?;
        if let Some(self_updated_at) = self_updated_at {
            self.self_updated_at = Some(self_updated_at);
        }
        Ok(())
    }

    pub(crate) async fn delete_from_db(
        txn: &mut WriteDbTransaction<'_>,
        group_id: &GroupId,
    ) -> sqlx::Result<()> {
        if let Some(mut group) = Group::load(&mut *txn, group_id).await? {
            let provider = AirOpenMlsProvider::new(txn.as_mut());
            group.mls_group.delete(provider.storage())?;
        };
        let group_id = GroupIdRefWrapper::from(group_id);
        query!(r#"DELETE FROM "group" WHERE group_id = ?"#, group_id)
            .execute(txn.as_mut())
            .await?;
        Ok(())
    }

    pub(crate) async fn load_all_group_ids(
        connection: &mut sqlx::SqliteConnection,
    ) -> sqlx::Result<Vec<GroupId>> {
        struct SqlGroupId {
            group_id: GroupIdWrapper,
        }
        let group_ids = query_as!(
            SqlGroupId,
            r#"SELECT group_id AS "group_id: _" FROM "group""#,
        )
        .fetch_all(connection)
        .await?;

        Ok(group_ids
            .into_iter()
            .map(
                |SqlGroupId {
                     group_id: GroupIdWrapper(group_id),
                 }| group_id,
            )
            .collect())
    }
}
