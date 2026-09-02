// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    crypto::{aead::AeadEncryptable, indexed_aead::keys::UserProfileKey},
    identifiers::{QualifiedGroupId, UserId, Username},
    messages::{
        client_as::ConnectionOfferHash,
        client_ds::{AadMessage, AadPayload, JoinConnectionGroupParamsAad},
        connection_package::ConnectionPackageHash,
    },
    time::TimeStamp,
};
use anyhow::{Context, bail};
use tls_codec::DeserializeBytes;
use tracing::instrument;

use crate::{
    Chat, ChatId, ChatType, PartialContact, SystemMessage, TargetedMessageContact,
    chats::messages::TimestampedMessage,
    clients::{
        CoreUser,
        connection_offer::{FriendshipPackage, payload::ConnectionInfo},
    },
    contacts::UsernameContact,
    db::access::{ReadConnection, WriteConnection, WriteDbTransaction},
    groups::Group,
    key_stores::MemoryUserKeyStore,
    outbound_service::connection_accepts::ConnectionAccept,
};

pub(crate) struct PendingConnectionInfo {
    pub(crate) chat_id: ChatId,
    pub(crate) created_at: TimeStamp,
    pub(crate) connection_info: ConnectionInfo,
    pub(crate) handle: Option<Username>,
    pub(crate) connection_offer_hash: Option<ConnectionOfferHash>,
    pub(crate) connection_package_hash: Option<ConnectionPackageHash>,
}

impl CoreUser {
    /// Queues accepting a contact request.
    ///
    /// The accept itself runs in the outbound service, which retries until the
    /// join lands on the DS (see `outbound_service::connection_accepts`). The
    /// queued job persists across restarts. Accepting a request whose earlier
    /// accept failed permanently re-arms the job.
    #[instrument(skip(self), err)]
    pub async fn accept_contact_request(&self, chat_id: ChatId) -> anyhow::Result<()> {
        self.db()
            .with_write_transaction(async |txn| -> anyhow::Result<_> {
                let chat: Chat = Chat::load(&mut *txn, &chat_id)
                    .await?
                    .with_context(|| format!("Can't find chat with id {chat_id}"))?;
                let ChatType::PendingConnection(sender_user_id) = chat.chat_type() else {
                    bail!("Chat is not a pending connection");
                };
                let pending_connection_info = PendingConnectionInfo::load(&mut *txn, chat_id)
                    .await?
                    .with_context(|| {
                        format!("No pending connection info found for chat: {chat_id}")
                    })?;

                // Fail early when the partial contact is gone. The accept job
                // needs it to finalize.
                let sender_user_id = sender_user_id.clone();
                Self::load_partial_contact(
                    &mut *txn,
                    chat_id,
                    &pending_connection_info,
                    &sender_user_id,
                )
                .await?;

                ConnectionAccept::enqueue(txn, chat_id).await?;
                Ok(())
            })
            .await?;

        self.outbound_service().notify_connection_accepts();

        Ok(())
    }

    /// Completes an accepted connection after the DS accepted our external
    /// join.
    pub(crate) async fn finalize_accepted_connection(
        txn: &mut WriteDbTransaction<'_>,
        chat_id: ChatId,
        timestamp: TimeStamp,
    ) -> anyhow::Result<()> {
        // Remove the queue row even when the pending info is already gone.
        // The accept may have been finished by another path, e.g. the join
        // echo, and a leftover row would keep the accept marked as pending.
        ConnectionAccept::remove(&mut *txn, chat_id).await?;

        let Some(pending_connection_info) = PendingConnectionInfo::load(&mut *txn, chat_id).await?
        else {
            return Ok(());
        };
        let chat = Chat::load(&mut *txn, &chat_id)
            .await?
            .with_context(|| format!("Can't find chat with id {chat_id}"))?;
        let ChatType::PendingConnection(sender_user_id) = chat.chat_type() else {
            bail!("Chat is not a pending connection");
        };
        let sender_user_id = sender_user_id.clone();

        let partial_contact = Self::load_partial_contact(
            &mut *txn,
            chat_id,
            &pending_connection_info,
            &sender_user_id,
        )
        .await?;

        chat.set_chat_type(&mut *txn, &ChatType::Connection(sender_user_id.clone()))
            .await?;

        let accepted_message = TimestampedMessage::system_message(
            SystemMessage::AcceptedConnectionRequest {
                contact: sender_user_id.clone(),
                user_handle: pending_connection_info.handle,
            },
            timestamp,
        );
        Self::store_new_messages(&mut *txn, chat_id, vec![accepted_message]).await?;

        partial_contact
            .mark_as_complete(
                &mut *txn,
                sender_user_id,
                pending_connection_info.connection_info.friendship_package,
            )
            .await?;
        PendingConnectionInfo::delete(&mut *txn, chat_id).await?;
        if let Some(hash) = pending_connection_info.connection_offer_hash {
            Group::delete_connection_offer_psk(txn, hash)?;
        }
        Ok(())
    }

    /// Load the partial contact behind a pending connection:
    /// - UsernameContact: by chat_id since multiple senders can target the same username
    /// - TargetedMessageContact: by user_id
    pub(crate) async fn load_partial_contact(
        mut connection: impl ReadConnection,
        chat_id: ChatId,
        pending_connection_info: &PendingConnectionInfo,
        sender_user_id: &UserId,
    ) -> anyhow::Result<PartialContact> {
        let partial_contact = if pending_connection_info.handle.is_some() {
            UsernameContact::load_by_chat_id(&mut connection, chat_id)
                .await?
                .map(PartialContact::Username)
        } else {
            TargetedMessageContact::load(&mut connection, sender_user_id)
                .await?
                .map(PartialContact::TargetedMessage)
        };
        partial_contact.with_context(|| format!("No partial contact found for chat: {chat_id}"))
    }

    pub(crate) fn prepare_group(
        key_store: &MemoryUserKeyStore,
        user_id: &UserId,
        connection_info: &ConnectionInfo,
        own_user_profile_key: &UserProfileKey,
    ) -> anyhow::Result<(AadMessage, QualifiedGroupId)> {
        // We create a new group and signal that fact to the user,
        // so the user can decide if they want to accept the
        // connection.

        let encrypted_user_profile_key = own_user_profile_key.encrypt(
            &connection_info.connection_group_identity_link_wrapper_key,
            user_id,
        )?;

        let encrypted_friendship_package = FriendshipPackage {
            friendship_token: key_store.friendship_token.clone(),
            wai_ear_key: key_store.wai_ear_key.clone(),
            user_profile_base_secret: own_user_profile_key.base_secret().clone(),
        }
        .encrypt(&connection_info.friendship_package_ear_key)?;

        let aad: AadMessage = AadPayload::JoinConnectionGroup(JoinConnectionGroupParamsAad {
            encrypted_friendship_package,
            encrypted_user_profile_key,
        })
        .into();
        let qgid = QualifiedGroupId::tls_deserialize_exact_bytes(
            connection_info.connection_group_id.as_slice(),
        )?;

        Ok((aad, qgid))
    }
}

mod persistence {
    use sqlx::{query, query_as};

    use crate::db::access::ReadConnection;

    use super::*;

    impl PendingConnectionInfo {
        pub(crate) async fn load(
            mut connection: impl ReadConnection,
            chat_id: ChatId,
        ) -> sqlx::Result<Option<PendingConnectionInfo>> {
            query_as!(
                PendingConnectionInfo,
                r#"SELECT
                    chat_id AS "chat_id: ChatId",
                    created_at AS "created_at: TimeStamp",
                    connection_info AS "connection_info: ConnectionInfo",
                    handle AS "handle: _",
                    connection_offer_hash AS "connection_offer_hash: _",
                    connection_package_hash AS "connection_package_hash: _"
                FROM pending_connection_info
                WHERE chat_id = ?"#,
                chat_id,
            )
            .fetch_optional(connection.as_mut())
            .await
        }

        pub(crate) async fn store(&self, mut connection: impl WriteConnection) -> sqlx::Result<()> {
            query!(
                "INSERT OR REPLACE INTO pending_connection_info (
                    chat_id,
                    created_at,
                    connection_info,
                    handle,
                    connection_offer_hash,
                    connection_package_hash
                )
                VALUES (?,  ?, ?, ?, ?, ?)",
                self.chat_id,
                self.created_at,
                self.connection_info,
                self.handle,
                self.connection_offer_hash,
                self.connection_package_hash,
            )
            .execute(connection.as_mut())
            .await?;
            connection.notifier().update(self.chat_id);
            Ok(())
        }

        pub(crate) async fn delete(
            mut connection: impl WriteConnection,
            chat_id: ChatId,
        ) -> sqlx::Result<()> {
            query!(
                "DELETE FROM pending_connection_info WHERE chat_id = ?",
                chat_id
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use aircommon::{
        crypto::{
            aead::keys::{
                FriendshipPackageEarKey, GroupStateEarKey, IdentityLinkWrapperKey,
                WelcomeAttributionInfoEarKey,
            },
            indexed_aead::keys::UserProfileBaseSecret,
        },
        messages::FriendshipToken,
    };
    use openmls::group::GroupId;
    use sqlx::SqlitePool;

    use crate::{ChatMessage, Contact, chats::persistence::tests::test_chat, db::access::DbAccess};

    use super::*;

    fn test_connection_info() -> ConnectionInfo {
        ConnectionInfo {
            connection_group_id: GroupId::from_slice(&[0; 32]),
            connection_group_ear_key: GroupStateEarKey::random().unwrap(),
            connection_group_identity_link_wrapper_key: IdentityLinkWrapperKey::random().unwrap(),
            friendship_package_ear_key: FriendshipPackageEarKey::random().unwrap(),
            friendship_package: FriendshipPackage {
                friendship_token: FriendshipToken::random().unwrap(),
                wai_ear_key: WelcomeAttributionInfoEarKey::random().unwrap(),
                user_profile_base_secret: UserProfileBaseSecret::random().unwrap(),
            },
        }
    }

    #[sqlx::test]
    async fn finalize_accepted_connection_is_idempotent(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;

        let sender_user_id = UserId::random("localhost".parse().unwrap());
        let mut chat = test_chat();
        chat.chat_type = ChatType::PendingConnection(sender_user_id.clone());
        chat.store(&mut txn).await?;

        TargetedMessageContact::new(
            sender_user_id.clone(),
            chat.id(),
            FriendshipPackageEarKey::random()?,
        )
        .upsert(&mut txn)
        .await?;

        PendingConnectionInfo {
            chat_id: chat.id(),
            created_at: TimeStamp::now(),
            connection_info: test_connection_info(),
            handle: None,
            connection_offer_hash: None,
            connection_package_hash: None,
        }
        .store(&mut txn)
        .await?;

        ConnectionAccept::enqueue(&mut txn, chat.id()).await?;

        CoreUser::finalize_accepted_connection(&mut txn, chat.id(), TimeStamp::now()).await?;

        let finalized = Chat::load(&mut txn, &chat.id()).await?.unwrap();
        assert_eq!(
            finalized.chat_type(),
            &ChatType::Connection(sender_user_id.clone())
        );
        assert!(
            PendingConnectionInfo::load(&mut txn, chat.id())
                .await?
                .is_none()
        );
        assert!(
            ConnectionAccept::status(&mut txn, chat.id())
                .await?
                .is_none()
        );
        assert!(
            TargetedMessageContact::load(&mut txn, &sender_user_id)
                .await?
                .is_none()
        );
        let contact = Contact::load(&mut txn, &sender_user_id).await?.unwrap();
        assert_eq!(contact.chat_id, chat.id());
        let messages = ChatMessage::load_multiple(&mut txn, chat.id(), 10).await?;
        assert_eq!(messages.len(), 1);

        // The response and echo paths race. Whoever comes second is a no-op.
        CoreUser::finalize_accepted_connection(&mut txn, chat.id(), TimeStamp::now()).await?;

        let messages = ChatMessage::load_multiple(&mut txn, chat.id(), 10).await?;
        assert_eq!(messages.len(), 1);

        // A leftover queue row is removed even when the pending info is
        // already gone.
        ConnectionAccept::enqueue(&mut txn, chat.id()).await?;
        CoreUser::finalize_accepted_connection(&mut txn, chat.id(), TimeStamp::now()).await?;
        assert!(
            ConnectionAccept::status(&mut txn, chat.id())
                .await?
                .is_none()
        );

        Ok(())
    }
}
