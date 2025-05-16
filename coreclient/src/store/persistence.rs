// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{borrow::Cow, collections::BTreeMap};

use enumset::EnumSet;
use phnxtypes::{codec::PhnxCodec, identifiers::AsClientId};
use serde::{Deserialize, Serialize};
use sqlx::{
    Acquire, Decode, Encode, Sqlite, SqliteExecutor, Type, encode::IsNull, error::BoxDynError,
    query, query_as,
};
use tokio_stream::StreamExt;
use tracing::error;
use uuid::Uuid;

use crate::{ConversationId, ConversationMessageId};

use super::{StoreEntityId, StoreNotification, StoreOperation, notification::StoreEntityKind};

#[derive(Serialize, Deserialize)]
struct StoredAsClientId<'a>(Cow<'a, AsClientId>);

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
            StoreEntityId::User(as_client_id) => {
                let bytes = PhnxCodec::to_vec(&StoredAsClientId(Cow::Borrowed(as_client_id)))?;
                Encode::<Sqlite>::encode(bytes, buf)
            }
            StoreEntityId::Conversation(conversation_id) => {
                Encode::<Sqlite>::encode_by_ref(&conversation_id.uuid, buf)
            }
            StoreEntityId::Message(conversation_message_id) => {
                Encode::<Sqlite>::encode_by_ref(&conversation_message_id.uuid, buf)
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
                let StoredAsClientId(as_client_id) = PhnxCodec::from_slice(&entity_id)?;
                StoreEntityId::User(as_client_id.into_owned())
            }
            StoreEntityKind::Conversation => {
                StoreEntityId::Conversation(ConversationId::new(Uuid::from_slice(&entity_id)?))
            }
            StoreEntityKind::Message => {
                StoreEntityId::Message(ConversationMessageId::new(Uuid::from_slice(&entity_id)?))
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
                "INSERT INTO store_notifications (entity_id, kind, added, updated, removed)
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
            r#"DELETE FROM store_notifications RETURNING
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

    use crate::{ConversationId, ConversationMessageId};

    use super::*;

    #[sqlx::test]
    async fn queue_dequeue_notification(pool: SqlitePool) -> anyhow::Result<()> {
        let mut notification = StoreNotification::default();
        notification.ops.insert(
            StoreEntityId::User(AsClientId::random("localhost".parse()?)),
            StoreOperation::Add.into(),
        );
        notification.ops.insert(
            StoreEntityId::Conversation(ConversationId {
                uuid: Uuid::new_v4(),
            }),
            StoreOperation::Update.into(),
        );
        notification.ops.insert(
            StoreEntityId::Message(ConversationMessageId {
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
        let conversation_id = ConversationId::new(Uuid::new_v4());

        let mut notification = StoreNotification::default();
        notification.ops.insert(
            StoreEntityId::Conversation(conversation_id),
            StoreOperation::Add.into(),
        );
        notification.enqueue(pool.acquire().await?.as_mut()).await?;

        let mut notification = StoreNotification::default();
        notification.ops.insert(
            StoreEntityId::Conversation(conversation_id),
            StoreOperation::Update.into(),
        );
        notification.enqueue(pool.acquire().await?.as_mut()).await?;

        let mut notification = StoreNotification::default();
        notification.ops.insert(
            StoreEntityId::Conversation(conversation_id),
            StoreOperation::Remove.into(),
        );
        notification.enqueue(pool.acquire().await?.as_mut()).await?;

        let dequeued_notification = StoreNotification::dequeue(&pool).await?;
        let expected = StoreNotification {
            ops: [(
                StoreEntityId::Conversation(conversation_id),
                StoreOperation::Add | StoreOperation::Update | StoreOperation::Remove,
            )]
            .into(),
        };
        assert_eq!(dequeued_notification, expected);

        Ok(())
    }
}
