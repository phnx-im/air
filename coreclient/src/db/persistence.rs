// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{borrow::Cow, collections::BTreeMap};

use aircommon::{
    codec::PersistenceCodec,
    identifiers::{AttachmentId, UserId},
};
use enumset::EnumSet;
use serde::{Deserialize, Serialize};
use sqlx::{Decode, Encode, Sqlite, Type, encode::IsNull, error::BoxDynError, query, query_as};
use tokio_stream::StreamExt;
use tracing::error;
use uuid::Uuid;

use crate::{ChatId, MessageId, db::access::WriteConnection};

use super::notification::{DbEntityId, DbEntityKind, DbNotification, DbOperation};

#[derive(Serialize, Deserialize)]
struct StoredUserId<'a>(Cow<'a, UserId>);

impl Type<Sqlite> for DbEntityId {
    fn type_info() -> <Sqlite as sqlx::Database>::TypeInfo {
        <Vec<u8> as Type<Sqlite>>::type_info()
    }
}

impl<'q> Encode<'q, Sqlite> for DbEntityId {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as sqlx::Database>::ArgumentBuffer,
    ) -> Result<IsNull, BoxDynError> {
        match self {
            DbEntityId::User(user_id) => {
                let bytes = PersistenceCodec::to_vec(&StoredUserId(Cow::Borrowed(user_id)))?;
                Encode::<Sqlite>::encode(bytes, buf)
            }
            DbEntityId::Chat(chat_id) => Encode::<Sqlite>::encode_by_ref(&chat_id.uuid, buf),
            DbEntityId::Message(message_id) => {
                Encode::<Sqlite>::encode_by_ref(&message_id.uuid, buf)
            }
            DbEntityId::Attachment(attachment_id) => {
                Encode::<Sqlite>::encode_by_ref(&attachment_id.uuid, buf)
            }
        }
    }
}

impl Type<Sqlite> for DbEntityKind {
    fn type_info() -> <Sqlite as sqlx::Database>::TypeInfo {
        <i64 as Type<Sqlite>>::type_info()
    }
}

impl<'q> Encode<'q, Sqlite> for DbEntityKind {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as sqlx::Database>::ArgumentBuffer,
    ) -> Result<IsNull, BoxDynError> {
        Encode::<Sqlite>::encode(*self as i64, buf)
    }
}

impl<'r> Decode<'r, Sqlite> for DbEntityKind {
    fn decode(value: <Sqlite as sqlx::Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let value: i64 = Decode::<Sqlite>::decode(value)?;
        Ok(value.try_into()?)
    }
}

struct SqlDbNotification {
    entity_id: Vec<u8>,
    kind: DbEntityKind,
    added: bool,
    updated: bool,
    removed: bool,
}

impl SqlDbNotification {
    fn into_entity_id_and_op(self) -> anyhow::Result<(DbEntityId, EnumSet<DbOperation>)> {
        let Self {
            entity_id,
            kind,
            added,
            updated,
            removed,
        } = self;
        let entity_id = match kind {
            DbEntityKind::User => {
                let StoredUserId(user_id) = PersistenceCodec::from_slice(&entity_id)?;
                DbEntityId::User(user_id.into_owned())
            }
            DbEntityKind::Chat => DbEntityId::Chat(ChatId::new(Uuid::from_slice(&entity_id)?)),
            DbEntityKind::Message => {
                DbEntityId::Message(MessageId::new(Uuid::from_slice(&entity_id)?))
            }
            DbEntityKind::Attachment => {
                DbEntityId::Attachment(AttachmentId::new(Uuid::from_slice(&entity_id)?))
            }
        };
        let mut op: EnumSet<DbOperation> = Default::default();
        if added {
            op.insert(DbOperation::Add);
        }
        if updated {
            op.insert(DbOperation::Update);
        }
        if removed {
            op.insert(DbOperation::Remove);
        }
        Ok((entity_id, op))
    }
}

impl DbNotification {
    pub(crate) async fn enqueue(&self, mut connection: impl WriteConnection) -> sqlx::Result<()> {
        let mut transaction = connection.begin().await?;
        for (entity_id, operation) in &self.ops {
            let kind = entity_id.kind();
            let added = operation.contains(DbOperation::Add);
            let updated = operation.contains(DbOperation::Update);
            let removed = operation.contains(DbOperation::Remove);
            query!(
                "INSERT INTO store_notification (entity_id, kind, added, updated, removed)
                VALUES (?1, ?2, ?3, ?4, ?5)
                ON CONFLICT DO UPDATE SET
                    added = MAX(?3, added),
                    updated = MAX(?4, updated),
                    removed = MAX(?5, removed)",
                entity_id,
                kind,
                added,
                updated,
                removed,
            )
            .execute(transaction.as_mut())
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    pub(crate) async fn dequeue(
        mut connection: impl WriteConnection,
    ) -> sqlx::Result<DbNotification> {
        let mut records = query_as!(
            SqlDbNotification,
            r#"DELETE FROM store_notification RETURNING
                entity_id,
                kind AS "kind: _",
                added,
                updated,
                removed
            "#
        )
        .fetch(connection.as_mut());

        let mut ops = BTreeMap::new();
        while let Some(record) = records.next().await {
            let record = record?;
            match record.into_entity_id_and_op() {
                Ok((entity_id, op)) => {
                    let entry = ops.entry(entity_id).or_default();
                    *entry |= op;
                }
                Err(error) => {
                    error!(%error, "Error parsing DB notification; skipping");
                }
            }
        }
        Ok(DbNotification { ops })
    }
}

#[cfg(test)]
mod tests {
    use sqlx::SqlitePool;
    use uuid::Uuid;

    use crate::{ChatId, MessageId, db::access::DbAccess};

    use super::*;

    #[sqlx::test]
    async fn queue_dequeue_notification(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let mut notification = DbNotification::default();
        notification.ops.insert(
            DbEntityId::User(UserId::random("localhost".parse()?)),
            DbOperation::Add.into(),
        );
        notification.ops.insert(
            DbEntityId::Chat(ChatId {
                uuid: Uuid::new_v4(),
            }),
            DbOperation::Update.into(),
        );
        notification.ops.insert(
            DbEntityId::Message(MessageId {
                uuid: uuid::Uuid::new_v4(),
            }),
            DbOperation::Remove | DbOperation::Update,
        );

        notification.enqueue(pool.write().await?).await?;

        let dequeued_notification = DbNotification::dequeue(pool.write().await?).await?;
        assert_eq!(notification, dequeued_notification);

        let dequeued_notification = DbNotification::dequeue(pool.write().await?).await?;
        assert!(dequeued_notification.is_empty());

        Ok(())
    }

    #[sqlx::test]
    async fn queue_notification_with_conflict(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let chat_id = ChatId::new(Uuid::new_v4());

        let mut notification = DbNotification::default();
        notification
            .ops
            .insert(DbEntityId::Chat(chat_id), DbOperation::Add.into());
        notification.enqueue(pool.write().await?).await?;

        let mut notification = DbNotification::default();
        notification
            .ops
            .insert(DbEntityId::Chat(chat_id), DbOperation::Update.into());
        notification.enqueue(pool.write().await?).await?;

        let mut notification = DbNotification::default();
        notification
            .ops
            .insert(DbEntityId::Chat(chat_id), DbOperation::Remove.into());
        notification.enqueue(pool.write().await?).await?;

        let dequeued_notification = DbNotification::dequeue(pool.write().await?).await?;
        let expected = DbNotification {
            ops: [(
                DbEntityId::Chat(chat_id),
                DbOperation::Add | DbOperation::Update | DbOperation::Remove,
            )]
            .into(),
        };
        assert_eq!(dequeued_notification, expected);

        Ok(())
    }
}
