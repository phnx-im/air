// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    crypto::ear::keys::{FriendshipPackageEarKey, WelcomeAttributionInfoEarKey},
    identifiers::{Fqdn, UserHandle, UserId},
    messages::FriendshipToken,
};
use chrono::Utc;
use sqlx::{SqliteExecutor, SqliteTransaction, query, query_as};
use tokio_stream::StreamExt;
use uuid::Uuid;

use crate::{
    ChatId, Contact,
    clients::connection_offer::FriendshipPackage,
    contacts::{PartialContact, PartialContactType, TargetedMessageContact},
    store::StoreNotifier,
};

use super::HandleContact;

struct SqlContact {
    user_uuid: Uuid,
    user_domain: Fqdn,
    chat_id: ChatId,
    wai_ear_key: WelcomeAttributionInfoEarKey,
    friendship_token: FriendshipToken,
}

impl From<SqlContact> for Contact {
    fn from(
        SqlContact {
            user_uuid,
            user_domain,
            wai_ear_key,
            friendship_token,
            chat_id,
        }: SqlContact,
    ) -> Self {
        Self {
            user_id: UserId::new(user_uuid, user_domain),
            wai_ear_key,
            friendship_token,
            chat_id,
        }
    }
}

impl Contact {
    pub(crate) async fn load(
        executor: impl SqliteExecutor<'_>,
        user_id: &UserId,
    ) -> sqlx::Result<Option<Self>> {
        let uuid = user_id.uuid();
        let domain = user_id.domain();
        query_as!(
            SqlContact,
            r#"SELECT
                user_uuid AS "user_uuid: _",
                user_domain AS "user_domain: _",
                chat_id AS "chat_id: _",
                wai_ear_key AS "wai_ear_key: _",
                friendship_token AS "friendship_token: _"
            FROM contact
            WHERE user_uuid = ? AND user_domain = ?"#,
            uuid,
            domain
        )
        .fetch_optional(executor)
        .await
        .map(|res| res.map(From::from))
    }

    pub(crate) async fn load_all(executor: impl SqliteExecutor<'_>) -> sqlx::Result<Vec<Self>> {
        query_as!(
            SqlContact,
            r#"SELECT
                user_uuid AS "user_uuid: _",
                user_domain AS "user_domain: _",
                chat_id AS "chat_id: _",
                wai_ear_key AS "wai_ear_key: _",
                friendship_token AS "friendship_token: _"
            FROM contact"#
        )
        .fetch(executor)
        .map(|res| res.map(From::from))
        .collect()
        .await
    }

    pub(crate) async fn upsert(
        &self,
        executor: impl SqliteExecutor<'_>,
        notifier: &mut StoreNotifier,
    ) -> sqlx::Result<()> {
        let uuid = self.user_id.uuid();
        let domain = self.user_id.domain();
        query!(
            "INSERT OR REPLACE INTO contact (
                user_uuid,
                user_domain,
                chat_id,
                wai_ear_key,
                friendship_token
            ) VALUES (?, ?, ?, ?, ?)",
            uuid,
            domain,
            self.chat_id,
            self.wai_ear_key,
            self.friendship_token,
        )
        .execute(executor)
        .await?;
        notifier.add(self.user_id.clone()).update(self.chat_id);
        Ok(())
    }
}

impl HandleContact {
    pub(crate) async fn upsert(
        &self,
        executor: impl SqliteExecutor<'_>,
        notifier: &mut StoreNotifier,
    ) -> sqlx::Result<()> {
        let created_at = Utc::now();
        query!(
            "INSERT OR REPLACE INTO user_handle_contact (
                user_handle,
                chat_id,
                friendship_package_ear_key,
                created_at,
                connection_offer_hash
            ) VALUES (?, ?, ?, ?, ?)",
            self.handle,
            self.chat_id,
            self.friendship_package_ear_key,
            created_at,
            self.connection_offer_hash
        )
        .execute(executor)
        .await?;
        notifier.update(self.chat_id);
        Ok(())
    }

    pub(crate) async fn load(
        executor: impl SqliteExecutor<'_>,
        handle: &UserHandle,
    ) -> sqlx::Result<Option<Self>> {
        query_as!(
            Self,
            r#"SELECT
                user_handle AS "handle: _",
                chat_id AS "chat_id: _",
                friendship_package_ear_key AS "friendship_package_ear_key: _",
                connection_offer_hash AS "connection_offer_hash: _"
            FROM user_handle_contact
            WHERE user_handle = ?"#,
            handle,
        )
        .fetch_optional(executor)
        .await
    }

    pub(crate) async fn load_all(executor: impl SqliteExecutor<'_>) -> sqlx::Result<Vec<Self>> {
        query_as!(
            Self,
            r#"SELECT
                user_handle AS "handle: _",
                chat_id AS "chat_id: _",
                friendship_package_ear_key AS "friendship_package_ear_key: _",
                connection_offer_hash AS "connection_offer_hash: _"
            FROM user_handle_contact"#,
        )
        .fetch_all(executor)
        .await
    }

    async fn delete(&self, executor: impl SqliteExecutor<'_>) -> sqlx::Result<()> {
        query!(
            "DELETE FROM user_handle_contact WHERE user_handle = ?",
            self.handle
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    /// Creates and persists a [`Contact`] from this [`HandleContact`] and the additional data
    pub(crate) async fn mark_as_complete(
        self,
        txn: &mut SqliteTransaction<'_>,
        notifier: &mut StoreNotifier,
        user_id: UserId,
        friendship_package: FriendshipPackage,
    ) -> anyhow::Result<Contact> {
        let contact = Contact {
            user_id,
            chat_id: self.chat_id,
            wai_ear_key: friendship_package.wai_ear_key,
            friendship_token: friendship_package.friendship_token,
        };

        self.delete(txn.as_mut()).await?;
        contact.upsert(txn.as_mut(), notifier).await?;

        Ok(contact)
    }
}

struct Record {
    user_id: Uuid,
    user_domain: Fqdn,
    chat_id: ChatId,
    friendship_package_ear_key: FriendshipPackageEarKey,
}

impl From<Record> for TargetedMessageContact {
    fn from(
        Record {
            user_id,
            user_domain,
            chat_id,
            friendship_package_ear_key,
        }: Record,
    ) -> Self {
        Self {
            user_id: UserId::new(user_id, user_domain),
            chat_id,
            friendship_package_ear_key,
        }
    }
}

impl TargetedMessageContact {
    pub(crate) async fn upsert(
        &self,
        executor: impl SqliteExecutor<'_>,
        notifier: &mut StoreNotifier,
    ) -> sqlx::Result<()> {
        let created_at = Utc::now();
        let uuid = self.user_id.uuid();
        let domain = self.user_id.domain();
        query!(
            "INSERT OR REPLACE INTO targeted_message_contact (
                user_uuid,
                user_domain,
                chat_id,
                friendship_package_ear_key,
                created_at
            ) VALUES (?, ?,?, ?, ?)",
            uuid,
            domain,
            self.chat_id,
            self.friendship_package_ear_key,
            created_at,
        )
        .execute(executor)
        .await?;
        notifier.update(self.chat_id);
        Ok(())
    }

    pub(crate) async fn load(
        executor: impl SqliteExecutor<'_>,
        user_id: &UserId,
    ) -> sqlx::Result<Option<Self>> {
        let uuid = user_id.uuid();
        let domain = user_id.domain();
        query_as!(
            Record,
            r#"SELECT
                user_uuid AS "user_id: _",
                user_domain AS "user_domain: _",
                chat_id AS "chat_id: _",
                friendship_package_ear_key AS "friendship_package_ear_key: _"
            FROM targeted_message_contact
            WHERE user_uuid = ? AND user_domain = ?"#,
            uuid,
            domain,
        )
        .fetch_optional(executor)
        .await
        .map(|res| res.map(From::from))
    }

    pub(crate) async fn load_all(executor: impl SqliteExecutor<'_>) -> sqlx::Result<Vec<Self>> {
        query_as!(
            Record,
            r#"SELECT
                user_uuid AS "user_id: _",
                user_domain AS "user_domain: _",
                chat_id AS "chat_id: _",
                friendship_package_ear_key AS "friendship_package_ear_key: _"
            FROM targeted_message_contact"#,
        )
        .fetch_all(executor)
        .await
        .map(|records| records.into_iter().map(From::from).collect())
    }

    async fn delete(&self, executor: impl SqliteExecutor<'_>) -> sqlx::Result<()> {
        let uuid = self.user_id.uuid();
        let domain = self.user_id.domain();
        query!(
            "DELETE FROM targeted_message_contact WHERE user_uuid = ? AND user_domain = ?",
            uuid,
            domain
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    /// Creates and persists a [`Contact`] from this [`HandleContact`] and the additional data
    pub(crate) async fn mark_as_complete(
        self,
        txn: &mut SqliteTransaction<'_>,
        notifier: &mut StoreNotifier,
        friendship_package: FriendshipPackage,
    ) -> anyhow::Result<Contact> {
        self.delete(txn.as_mut()).await?;

        let contact = Contact {
            user_id: self.user_id,
            chat_id: self.chat_id,
            wai_ear_key: friendship_package.wai_ear_key,
            friendship_token: friendship_package.friendship_token,
        };

        contact.upsert(txn.as_mut(), notifier).await?;

        Ok(contact)
    }
}

impl PartialContact {
    pub(crate) async fn load(
        executor: impl SqliteExecutor<'_>,
        contact_type: PartialContactType,
    ) -> sqlx::Result<Option<Self>> {
        match contact_type {
            PartialContactType::Handle(handle) => Ok(HandleContact::load(executor, &handle)
                .await?
                .map(PartialContact::Handle)),
            PartialContactType::TargetedMessage(user_id) => {
                Ok(TargetedMessageContact::load(executor, &user_id)
                    .await?
                    .map(PartialContact::TargetedMessage))
            }
        }
    }

    pub(crate) async fn mark_as_complete(
        self,
        txn: &mut SqliteTransaction<'_>,
        notifier: &mut StoreNotifier,
        user_id: UserId,
        friendship_package: FriendshipPackage,
    ) -> anyhow::Result<Contact> {
        match self {
            PartialContact::Handle(handle_contact) => {
                handle_contact
                    .mark_as_complete(txn, notifier, user_id, friendship_package)
                    .await
            }
            PartialContact::TargetedMessage(targeted_message_contact) => {
                targeted_message_contact
                    .mark_as_complete(txn, notifier, friendship_package)
                    .await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::vec;

    use aircommon::{
        crypto::{
            ear::keys::{FriendshipPackageEarKey, WelcomeAttributionInfoEarKey},
            indexed_aead::keys::UserProfileKey,
        },
        messages::{FriendshipToken, client_as::ConnectionOfferHash},
    };
    use sqlx::SqlitePool;

    use crate::{
        ChatId, chats::persistence::tests::test_chat, key_stores::indexed_keys::StorableIndexedKey,
    };

    use super::*;

    fn test_contact(chat_id: ChatId) -> Contact {
        let user_id = UserId::random("localhost".parse().unwrap());
        Contact {
            user_id,
            wai_ear_key: WelcomeAttributionInfoEarKey::random().unwrap(),
            friendship_token: FriendshipToken::random().unwrap(),
            chat_id,
        }
    }

    #[sqlx::test]
    async fn contact_store_load(pool: SqlitePool) -> anyhow::Result<()> {
        let mut store_notifier = StoreNotifier::noop();

        let chat = test_chat();
        chat.store(pool.acquire().await?.as_mut(), &mut store_notifier)
            .await?;

        let contact = test_contact(chat.id());
        contact.upsert(&pool, &mut store_notifier).await?;

        let loaded = Contact::load(&pool, &contact.user_id).await?.unwrap();
        assert_eq!(loaded, contact);

        Ok(())
    }

    #[sqlx::test]
    async fn handle_contact_upsert_load(pool: SqlitePool) -> anyhow::Result<()> {
        let mut store_notifier = StoreNotifier::noop();
        let chat = test_chat();
        chat.store(pool.acquire().await?.as_mut(), &mut store_notifier)
            .await?;

        let handle = UserHandle::new("ellie_".to_owned()).unwrap();
        let handle_contact = HandleContact {
            handle: handle.clone(),
            chat_id: chat.id(),
            friendship_package_ear_key: FriendshipPackageEarKey::random().unwrap(),
            connection_offer_hash: ConnectionOfferHash::new_for_test(vec![1, 2, 3, 4, 5]),
        };

        handle_contact.upsert(&pool, &mut store_notifier).await?;

        let loaded = HandleContact::load(&pool, &handle).await?.unwrap();
        assert_eq!(loaded, handle_contact);

        Ok(())
    }

    #[sqlx::test]
    async fn handle_contact_mark_as_complete(pool: SqlitePool) -> anyhow::Result<()> {
        let mut store_notifier = StoreNotifier::noop();
        let chat = test_chat();
        chat.store(pool.acquire().await?.as_mut(), &mut store_notifier)
            .await?;

        let handle = UserHandle::new("ellie_".to_owned()).unwrap();
        let handle_contact = HandleContact {
            handle: handle.clone(),
            chat_id: chat.id(),
            friendship_package_ear_key: FriendshipPackageEarKey::random().unwrap(),
            connection_offer_hash: ConnectionOfferHash::new_for_test(vec![1, 2, 3, 4, 5]),
        };

        let user_id = UserId::random("localhost".parse().unwrap());
        let user_profile_key = UserProfileKey::random(&user_id)?;
        user_profile_key.store(&pool).await?;

        let friendship_package = FriendshipPackage {
            friendship_token: FriendshipToken::random().unwrap(),
            wai_ear_key: WelcomeAttributionInfoEarKey::random().unwrap(),
            user_profile_base_secret: user_profile_key.base_secret().clone(),
        };

        let mut txn = pool.begin().await?;

        let contact = handle_contact
            .mark_as_complete(&mut txn, &mut store_notifier, user_id, friendship_package)
            .await?;

        txn.commit().await?;

        let loaded_handle_contact = HandleContact::load(&pool, &handle).await?;
        assert!(loaded_handle_contact.is_none());

        let loaded_contact = Contact::load(&pool, &contact.user_id).await?.unwrap();
        assert_eq!(loaded_contact, contact);

        Ok(())
    }

    #[sqlx::test]
    async fn handle_contact_delete(pool: SqlitePool) -> anyhow::Result<()> {
        tracing_subscriber::fmt::try_init().ok();

        let mut store_notifier = StoreNotifier::noop();
        let chat = test_chat();
        chat.store(pool.acquire().await?.as_mut(), &mut store_notifier)
            .await?;

        let handle = UserHandle::new("ellie_".to_owned()).unwrap();
        let handle_contact = HandleContact {
            handle: handle.clone(),
            chat_id: chat.id(),
            friendship_package_ear_key: FriendshipPackageEarKey::random().unwrap(),
            connection_offer_hash: ConnectionOfferHash::new_for_test(vec![1, 2, 3, 4, 5]),
        };

        handle_contact.upsert(&pool, &mut store_notifier).await?;

        let mut txn = pool.begin().await?;
        handle_contact.delete(txn.as_mut()).await?;
        txn.commit().await?;

        let loaded = HandleContact::load(&pool, &handle).await?;
        assert!(loaded.is_none());

        Ok(())
    }

    #[sqlx::test]
    async fn handle_contact_upsert_idempotent(pool: SqlitePool) -> anyhow::Result<()> {
        let mut store_notifier = StoreNotifier::noop();
        let chat = test_chat();
        chat.store(pool.acquire().await?.as_mut(), &mut store_notifier)
            .await?;

        let handle = UserHandle::new("ellie_".to_owned()).unwrap();
        let handle_contact = HandleContact {
            handle: handle.clone(),
            chat_id: chat.id(),
            friendship_package_ear_key: FriendshipPackageEarKey::random().unwrap(),
            connection_offer_hash: ConnectionOfferHash::new_for_test(vec![1, 2, 3, 4, 5]),
        };

        handle_contact.upsert(&pool, &mut store_notifier).await?;
        handle_contact.upsert(&pool, &mut store_notifier).await?; // Upsert again

        let loaded = HandleContact::load(&pool, &handle).await?.unwrap();
        assert_eq!(loaded, handle_contact);

        Ok(())
    }
}
