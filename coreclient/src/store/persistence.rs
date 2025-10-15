// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{borrow::Cow, collections::BTreeMap};

use aircommon::{
    codec::PersistenceCodec,
    identifiers::{AttachmentId, UserId},
};
use anyhow::Context;
use enumset::EnumSet;
use serde::{Deserialize, Serialize};
use sqlx::{
    Acquire, Decode, Encode, Sqlite, SqliteExecutor, Type, encode::IsNull, error::BoxDynError,
    query, query_as,
};
use tokio_stream::StreamExt;
use tracing::error;
use uuid::Uuid;

use crate::{ChatId, MessageId, store::notification::StoreMessageStatus};

use super::{StoreEntityId, StoreNotification, StoreOperation, notification::StoreEntityKind};

#[derive(Serialize, Deserialize)]
struct StoredUserId<'a>(Cow<'a, UserId>);

impl Type<Sqlite> for StoreEntityId {
    fn type_info() -> <Sqlite as sqlx::Database>::TypeInfo {
        <Vec<u8> as Type<Sqlite>>::type_info()
    }
}

impl<'q> Encode<'q, Sqlite> for StoreEntityId {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as sqlx::Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        match self {
            StoreEntityId::User(user_id) => {
                let bytes = PersistenceCodec::to_vec(&StoredUserId(Cow::Borrowed(user_id)))?;
                Encode::<Sqlite>::encode(bytes, buf)
            }
            StoreEntityId::Chat(chat_id) => Encode::<Sqlite>::encode_by_ref(&chat_id.uuid, buf),
            StoreEntityId::Message(message_id) => {
                Encode::<Sqlite>::encode_by_ref(&message_id.uuid, buf)
            }
            StoreEntityId::Attachment(attachment_id) => {
                Encode::<Sqlite>::encode_by_ref(&attachment_id.uuid, buf)
            }
            StoreEntityId::Status(chat_id, status) => {
                let mut bytes: Vec<u8> = Vec::with_capacity(17);
                bytes.extend_from_slice(chat_id.uuid.as_bytes());
                bytes.push(status.repr());
                Encode::<Sqlite>::encode(bytes, buf)
            }
        }
    }
}

impl Type<Sqlite> for StoreEntityKind {
    fn type_info() -> <Sqlite as sqlx::Database>::TypeInfo {
        <i64 as Type<Sqlite>>::type_info()
    }
}

impl<'q> Encode<'q, Sqlite> for StoreEntityKind {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as sqlx::Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        Encode::<Sqlite>::encode(*self as i64, buf)
    }
}

impl<'r> Decode<'r, Sqlite> for StoreEntityKind {
    fn decode(value: <Sqlite as sqlx::Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let value: i64 = Decode::<Sqlite>::decode(value)?;
        Ok(value.try_into()?)
    }
}

struct SqlStoreNotification {
    entity_id: Vec<u8>,
    kind: StoreEntityKind,
    added: bool,
    updated: bool,
    removed: bool,
}

impl SqlStoreNotification {
    fn into_entity_id_and_op(self) -> anyhow::Result<(StoreEntityId, EnumSet<StoreOperation>)> {
        let Self {
            entity_id,
            kind,
            added,
            updated,
            removed,
        } = self;
        let entity_id = match kind {
            StoreEntityKind::User => {
                let StoredUserId(user_id) = PersistenceCodec::from_slice(&entity_id)?;
                StoreEntityId::User(user_id.into_owned())
            }
            StoreEntityKind::Chat => {
                StoreEntityId::Chat(ChatId::new(Uuid::from_slice(&entity_id)?))
            }
            StoreEntityKind::Message => {
                StoreEntityId::Message(MessageId::new(Uuid::from_slice(&entity_id)?))
            }
            StoreEntityKind::Attachment => {
                StoreEntityId::Attachment(AttachmentId::new(Uuid::from_slice(&entity_id)?))
            }
            StoreEntityKind::Status => {
                let entity_id: &[u8; 17] =
                    entity_id.as_slice().try_into().context("invalid status")?;
                let chat_id = ChatId::new(Uuid::from_slice(&entity_id[0..16])?);
                let status = StoreMessageStatus::from(entity_id[16]);
                StoreEntityId::Status(chat_id, status)
            }
        };
        let mut op: EnumSet<StoreOperation> = Default::default();
        if added {
            op.insert(StoreOperation::Add);
        }
        if updated {
            op.insert(StoreOperation::Update);
        }
        if removed {
            op.insert(StoreOperation::Remove);
        }
        Ok((entity_id, op))
    }
}

impl StoreNotification {
    pub(crate) async fn enqueue(
        &self,
        connection: &mut sqlx::SqliteConnection,
    ) -> sqlx::Result<()> {
        let mut transaction = connection.begin().await?;
        for (entity_id, operation) in &self.ops {
            let kind = entity_id.kind();
            let added = operation.contains(StoreOperation::Add);
            let updated = operation.contains(StoreOperation::Update);
            let removed = operation.contains(StoreOperation::Remove);
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
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    pub(crate) async fn dequeue(
        executor: impl SqliteExecutor<'_>,
    ) -> sqlx::Result<StoreNotification> {
        let mut records = query_as!(
            SqlStoreNotification,
            r#"DELETE FROM store_notification RETURNING
                entity_id,
                kind AS "kind: _",
                added,
                updated,
                removed
            "#
        )
        .fetch(executor);

        let mut ops = BTreeMap::new();
        while let Some(record) = records.next().await {
            let record = record?;
            match record.into_entity_id_and_op() {
                Ok((entity_id, op)) => {
                    ops.insert(entity_id, op);
                }
                Err(error) => {
                    error!(%error, "Error parsing store notification; skipping");
                }
            }
        }
        Ok(StoreNotification { ops })
    }
}

#[cfg(test)]
mod tests {
    use sqlx::SqlitePool;
    use uuid::Uuid;

    use crate::{ChatId, MessageId};

    use super::*;

    #[sqlx::test]
    async fn queue_dequeue_notification(pool: SqlitePool) -> anyhow::Result<()> {
        let mut notification = StoreNotification::default();
        notification.ops.insert(
            StoreEntityId::User(UserId::random("localhost".parse()?)),
            StoreOperation::Add.into(),
        );
        notification.ops.insert(
            StoreEntityId::Chat(ChatId {
                uuid: Uuid::new_v4(),
            }),
            StoreOperation::Update.into(),
        );
        notification.ops.insert(
            StoreEntityId::Message(MessageId {
                uuid: uuid::Uuid::new_v4(),
            }),
            StoreOperation::Remove | StoreOperation::Update,
        );

        notification.enqueue(pool.acquire().await?.as_mut()).await?;

        let dequeued_notification = StoreNotification::dequeue(&pool).await?;
        assert_eq!(notification, dequeued_notification);

        let dequeued_notification = StoreNotification::dequeue(&pool).await?;
        assert!(dequeued_notification.is_empty());

        Ok(())
    }

    #[sqlx::test]
    async fn queue_notification_with_conflict(pool: SqlitePool) -> anyhow::Result<()> {
        let chat_id = ChatId::new(Uuid::new_v4());

        let mut notification = StoreNotification::default();
        notification
            .ops
            .insert(StoreEntityId::Chat(chat_id), StoreOperation::Add.into());
        notification.enqueue(pool.acquire().await?.as_mut()).await?;

        let mut notification = StoreNotification::default();
        notification
            .ops
            .insert(StoreEntityId::Chat(chat_id), StoreOperation::Update.into());
        notification.enqueue(pool.acquire().await?.as_mut()).await?;

        let mut notification = StoreNotification::default();
        notification
            .ops
            .insert(StoreEntityId::Chat(chat_id), StoreOperation::Remove.into());
        notification.enqueue(pool.acquire().await?.as_mut()).await?;

        let dequeued_notification = StoreNotification::dequeue(&pool).await?;
        let expected = StoreNotification {
            ops: [(
                StoreEntityId::Chat(chat_id),
                StoreOperation::Add | StoreOperation::Update | StoreOperation::Remove,
            )]
            .into(),
        };
        assert_eq!(dequeued_notification, expected);

        Ok(())
    }
}
