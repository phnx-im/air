// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    crypto::aead::keys::{FriendshipPackageEarKey, WelcomeAttributionInfoEarKey},
    identifiers::{Fqdn, UserId, Username},
    messages::FriendshipToken,
};
use chrono::Utc;
use sqlx::{query, query_as};
use tokio_stream::StreamExt;
use uuid::Uuid;

use crate::{
    ChatId, Contact,
    clients::connection_offer::FriendshipPackage,
    contacts::{PartialContact, PartialContactType, TargetedMessageContact},
    db_access::{ReadConnection, WriteConnection, WriteDbTransaction},
};

use super::UsernameContact;

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
        mut connection: impl ReadConnection,
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
        .fetch_optional(connection.as_mut())
        .await
        .map(|res| res.map(From::from))
    }

    pub(crate) async fn load_all(mut connection: impl ReadConnection) -> sqlx::Result<Vec<Self>> {
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
        .fetch(connection.as_mut())
        .map(|res| res.map(From::from))
        .collect()
        .await
    }

    pub(crate) async fn upsert(&self, mut connection: impl WriteConnection) -> sqlx::Result<()> {
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
        .execute(connection.as_mut())
        .await?;
        connection
            .notifier()
            .add(self.user_id.clone())
            .update(self.chat_id);
        Ok(())
    }
}

impl UsernameContact {
    pub(crate) async fn upsert(&self, mut connection: impl WriteConnection) -> sqlx::Result<()> {
        let created_at = Utc::now();
        query!(
            "INSERT INTO username_contact (
                chat_id,
                username,
                friendship_package_ear_key,
                created_at,
                connection_offer_hash
            ) VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(chat_id) DO UPDATE SET
                username = excluded.username,
                friendship_package_ear_key = excluded.friendship_package_ear_key,
                created_at = excluded.created_at,
                connection_offer_hash = excluded.connection_offer_hash",
            self.chat_id,
            self.username,
            self.friendship_package_ear_key,
            created_at,
            self.connection_offer_hash
        )
        .execute(connection.as_mut())
        .await?;
        connection.notifier().update(self.chat_id);
        Ok(())
    }

    pub(crate) async fn load(
        mut connection: impl ReadConnection,
        username: &Username,
    ) -> sqlx::Result<Option<Self>> {
        query_as!(
            Self,
            r#"SELECT
                username AS "username: _",
                chat_id AS "chat_id: _",
                friendship_package_ear_key AS "friendship_package_ear_key: _",
                connection_offer_hash AS "connection_offer_hash: _"
            FROM username_contact
            WHERE username = ?"#,
            username,
        )
        .fetch_optional(connection.as_mut())
        .await
    }

    pub(crate) async fn load_by_chat_id(
        mut connection: impl ReadConnection,
        chat_id: ChatId,
    ) -> sqlx::Result<Option<Self>> {
        query_as!(
            Self,
            r#"SELECT
                username AS "username: _",
                chat_id AS "chat_id: _",
                friendship_package_ear_key AS "friendship_package_ear_key: _",
                connection_offer_hash AS "connection_offer_hash: _"
            FROM username_contact
            WHERE chat_id = ?"#,
            chat_id,
        )
        .fetch_optional(connection.as_mut())
        .await
    }

    pub(crate) async fn load_all(mut connection: impl ReadConnection) -> sqlx::Result<Vec<Self>> {
        query_as!(
            Self,
            r#"SELECT
                username AS "username: _",
                chat_id AS "chat_id: _",
                friendship_package_ear_key AS "friendship_package_ear_key: _",
                connection_offer_hash AS "connection_offer_hash: _"
            FROM username_contact"#,
        )
        .fetch_all(connection.as_mut())
        .await
    }

    async fn delete(&self, mut connection: impl WriteConnection) -> sqlx::Result<()> {
        query!(
            "DELETE FROM username_contact WHERE chat_id = ?",
            self.chat_id
        )
        .execute(connection.as_mut())
        .await?;
        Ok(())
    }

    /// Creates and persists a [`Contact`] from this username contact and the additional data
    pub(crate) async fn mark_as_complete(
        self,
        txn: &mut WriteDbTransaction<'_>,
        user_id: UserId,
        friendship_package: FriendshipPackage,
    ) -> anyhow::Result<Contact> {
        let contact = Contact {
            user_id,
            chat_id: self.chat_id,
            wai_ear_key: friendship_package.wai_ear_key,
            friendship_token: friendship_package.friendship_token,
        };

        self.delete(&mut *txn).await?;
        contact.upsert(txn).await?;

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
    pub(crate) async fn upsert(&self, mut connection: impl WriteConnection) -> sqlx::Result<()> {
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
        .execute(connection.as_mut())
        .await?;
        connection.notifier().update(self.chat_id);
        Ok(())
    }

    pub(crate) async fn load(
        mut connection: impl ReadConnection,
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
        .fetch_optional(connection.as_mut())
        .await
        .map(|res| res.map(From::from))
    }

    pub(crate) async fn load_all(mut connection: impl ReadConnection) -> sqlx::Result<Vec<Self>> {
        query_as!(
            Record,
            r#"SELECT
                user_uuid AS "user_id: _",
                user_domain AS "user_domain: _",
                chat_id AS "chat_id: _",
                friendship_package_ear_key AS "friendship_package_ear_key: _"
            FROM targeted_message_contact"#,
        )
        .fetch_all(connection.as_mut())
        .await
        .map(|records| records.into_iter().map(From::from).collect())
    }

    async fn delete(&self, mut connection: impl WriteConnection) -> sqlx::Result<()> {
        let uuid = self.user_id.uuid();
        let domain = self.user_id.domain();
        query!(
            "DELETE FROM targeted_message_contact WHERE user_uuid = ? AND user_domain = ?",
            uuid,
            domain
        )
        .execute(connection.as_mut())
        .await?;
        Ok(())
    }

    /// Creates and persists a [`Contact`] from this username contact and the additional data
    pub(crate) async fn mark_as_complete(
        self,
        txn: &mut WriteDbTransaction<'_>,
        friendship_package: FriendshipPackage,
    ) -> anyhow::Result<Contact> {
        self.delete(&mut *txn).await?;

        let contact = Contact {
            user_id: self.user_id,
            chat_id: self.chat_id,
            wai_ear_key: friendship_package.wai_ear_key,
            friendship_token: friendship_package.friendship_token,
        };

        contact.upsert(txn).await?;

        Ok(contact)
    }
}

impl PartialContact {
    pub(crate) async fn upsert(&self, connection: impl WriteConnection) -> sqlx::Result<()> {
        match self {
            PartialContact::Username(username_contact) => username_contact.upsert(connection).await,
            PartialContact::TargetedMessage(targeted_message_contact) => {
                targeted_message_contact.upsert(connection).await
            }
        }
    }

    pub(crate) async fn load(
        connection: impl ReadConnection,
        contact_type: &PartialContactType,
    ) -> sqlx::Result<Option<Self>> {
        match contact_type {
            PartialContactType::Handle(username) => Ok(UsernameContact::load(connection, username)
                .await?
                .map(PartialContact::Username)),
            PartialContactType::TargetedMessage(user_id) => {
                Ok(TargetedMessageContact::load(connection, user_id)
                    .await?
                    .map(PartialContact::TargetedMessage))
            }
        }
    }

    pub(crate) async fn mark_as_complete(
        self,
        txn: &mut WriteDbTransaction<'_>,
        user_id: UserId,
        friendship_package: FriendshipPackage,
    ) -> anyhow::Result<Contact> {
        match self {
            PartialContact::Username(username_contact) => {
                username_contact
                    .mark_as_complete(txn, user_id, friendship_package)
                    .await
            }
            PartialContact::TargetedMessage(targeted_message_contact) => {
                targeted_message_contact
                    .mark_as_complete(txn, friendship_package)
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
            aead::keys::{FriendshipPackageEarKey, WelcomeAttributionInfoEarKey},
            indexed_aead::keys::UserProfileKey,
        },
        messages::{FriendshipToken, client_as::ConnectionOfferHash},
    };
    use sqlx::SqlitePool;

    use crate::{
        ChatId, chats::persistence::tests::test_chat, db_access::DbAccess,
        key_stores::indexed_keys::StorableIndexedKey,
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
        let pool = DbAccess::for_tests(pool);

        let chat = test_chat();
        chat.store(pool.write().await?).await?;

        let contact = test_contact(chat.id());
        contact.upsert(pool.write().await?).await?;

        let loaded = Contact::load(pool.read().await?, &contact.user_id)
            .await?
            .unwrap();
        assert_eq!(loaded, contact);

        Ok(())
    }

    #[sqlx::test]
    async fn handle_contact_upsert_load(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        let chat = test_chat();
        chat.store(pool.write().await?).await?;

        let username = Username::new("ellie-".to_owned()).unwrap();
        let username_contact = UsernameContact {
            username: username.clone(),
            chat_id: chat.id(),
            friendship_package_ear_key: FriendshipPackageEarKey::random().unwrap(),
            connection_offer_hash: ConnectionOfferHash::new_for_test(vec![1, 2, 3, 4, 5]),
        };

        username_contact.upsert(pool.write().await?).await?;

        let loaded = UsernameContact::load(pool.read().await?, &username)
            .await?
            .unwrap();
        assert_eq!(loaded, username_contact);

        Ok(())
    }

    #[sqlx::test]
    async fn handle_contact_mark_as_complete(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let chat = test_chat();
        chat.store(pool.write().await?).await?;

        let username = Username::new("ellie-".to_owned()).unwrap();
        let username_contact = UsernameContact {
            username: username.clone(),
            chat_id: chat.id(),
            friendship_package_ear_key: FriendshipPackageEarKey::random().unwrap(),
            connection_offer_hash: ConnectionOfferHash::new_for_test(vec![1, 2, 3, 4, 5]),
        };

        let user_id = UserId::random("localhost".parse().unwrap());
        let user_profile_key = UserProfileKey::random(&user_id)?;
        user_profile_key.store(pool.write().await?).await?;

        let friendship_package = FriendshipPackage {
            friendship_token: FriendshipToken::random().unwrap(),
            wai_ear_key: WelcomeAttributionInfoEarKey::random().unwrap(),
            user_profile_base_secret: user_profile_key.base_secret().clone(),
        };

        let contact = pool
            .with_write_transaction(async |txn| {
                username_contact
                    .mark_as_complete(txn, user_id, friendship_package)
                    .await
            })
            .await?;

        let loaded_username_contact = UsernameContact::load(pool.read().await?, &username).await?;
        assert!(loaded_username_contact.is_none());

        let loaded_contact = Contact::load(pool.read().await?, &contact.user_id)
            .await?
            .unwrap();
        assert_eq!(loaded_contact, contact);

        Ok(())
    }

    #[sqlx::test]
    async fn handle_contact_delete(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        tracing_subscriber::fmt::try_init().ok();

        let chat = test_chat();
        chat.store(pool.write().await?).await?;

        let username = Username::new("ellie-".to_owned()).unwrap();
        let username_contact = UsernameContact {
            username: username.clone(),
            chat_id: chat.id(),
            friendship_package_ear_key: FriendshipPackageEarKey::random().unwrap(),
            connection_offer_hash: ConnectionOfferHash::new_for_test(vec![1, 2, 3, 4, 5]),
        };

        username_contact.upsert(pool.write().await?).await?;
        username_contact.delete(pool.write().await?).await?;

        let loaded = UsernameContact::load(pool.read().await?, &username).await?;
        assert!(loaded.is_none());

        Ok(())
    }

    #[sqlx::test]
    async fn handle_contact_upsert_idempotent(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let chat = test_chat();
        chat.store(pool.write().await?).await?;

        let username = Username::new("ellie-".to_owned()).unwrap();
        let username_contact = UsernameContact {
            username: username.clone(),
            chat_id: chat.id(),
            friendship_package_ear_key: FriendshipPackageEarKey::random().unwrap(),
            connection_offer_hash: ConnectionOfferHash::new_for_test(vec![1, 2, 3, 4, 5]),
        };

        username_contact.upsert(pool.write().await?).await?;
        username_contact.upsert(pool.write().await?).await?; // Upsert again

        let loaded = UsernameContact::load(pool.read().await?, &username)
            .await?
            .unwrap();
        assert_eq!(loaded, username_contact);

        Ok(())
    }

    #[sqlx::test]
    async fn username_contact_multiple_senders_same_username(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        // Create two chats
        let chat_a = test_chat();
        let chat_b = test_chat();
        chat_a.store(pool.write().await?).await?;
        chat_b.store(pool.write().await?).await?;

        let username = Username::new("alice".to_owned()).unwrap();

        // Sender A sends connection request to username "alice"
        let contact_a = UsernameContact {
            username: username.clone(),
            chat_id: chat_a.id(),
            friendship_package_ear_key: FriendshipPackageEarKey::random().unwrap(),
            connection_offer_hash: ConnectionOfferHash::new_for_test(vec![1, 2, 3]),
        };
        contact_a.upsert(pool.write().await?).await?;

        // Verify A's UsernameContact exists
        let loaded_a = UsernameContact::load(pool.read().await?, &username)
            .await?
            .unwrap();
        assert_eq!(loaded_a.chat_id, chat_a.id());

        // Sender B sends connection request to same username "alice"
        let contact_b = UsernameContact {
            username: username.clone(),
            chat_id: chat_b.id(),
            friendship_package_ear_key: FriendshipPackageEarKey::random().unwrap(),
            connection_offer_hash: ConnectionOfferHash::new_for_test(vec![4, 5, 6]),
        };
        contact_b.upsert(pool.write().await?).await?;

        // Both contacts should exist (each has unique chat_id)
        let loaded_a_by_chat =
            UsernameContact::load_by_chat_id(pool.read().await?, chat_a.id()).await?;
        assert!(loaded_a_by_chat.is_some());

        let loaded_b_by_chat =
            UsernameContact::load_by_chat_id(pool.read().await?, chat_b.id()).await?;
        assert!(loaded_b_by_chat.is_some());

        Ok(())
    }
}
