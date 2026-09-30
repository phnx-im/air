// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Incoming connection requests lifecycle.
//!
//! All pending incoming requests of one sender share a chat, the one of the
//! newest request. A newer request moves the chat's messages to a chat of its
//! own group.

use aircommon::{
    identifiers::{UserId, Username},
    messages::{client_as::ConnectionOfferHash, connection_package::ConnectionPackageHash},
    time::TimeStamp,
};
use anyhow::Context;

use crate::{
    Chat, ChatId, ChatMessage, ChatType, SystemMessage,
    chats::{PendingConnectionRequest, messages::TimestampedMessage},
    clients::{CoreUser, connection_offer::payload::ConnectionInfo},
    db::access::{ReadTransaction, WriteDbTransaction},
    groups::Group,
    usernames::connection_packages::ConnectionPackageRecord,
};

/// An incoming connection request, as it arrives.
pub(crate) struct IncomingRequest {
    pub(crate) connection_info: ConnectionInfo,
    pub(crate) sender: UserId,
    pub(crate) source: IncomingRequestSource,
    /// When the request was sent, as the server saw it.
    pub(crate) received_at: TimeStamp,
}

pub(crate) enum IncomingRequestSource {
    Username {
        username: Username,
        connection_offer_hash: ConnectionOfferHash,
        connection_package_hash: ConnectionPackageHash,
    },
    Group {
        origin_chat_id: ChatId,
        /// The title of the group chat, for the announcement.
        chat_name: String,
    },
}

/// Metadata for stored requests
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoredRequest {
    /// The chat that shows the request.
    pub chat_id: ChatId,
    /// Previous chat, if any.
    pub moved_from: Option<ChatId>,
}

impl IncomingRequest {
    /// The chat id the request's connection group derives.
    pub(crate) fn request_id(&self) -> anyhow::Result<ChatId> {
        Ok(ChatId::try_from(&self.connection_info.connection_group_id)?)
    }

    /// Stores the request as pending, in the chat of the sender's other pending
    /// requests if there are any.
    pub(crate) async fn store(
        self,
        txn: &mut WriteDbTransaction<'_>,
    ) -> anyhow::Result<StoredRequest> {
        let request_id = self.request_id()?;
        let Self {
            connection_info,
            sender,
            source,
            received_at,
        } = self;
        let existing_chat = PendingConnectionRequest::chat_of_sender(&mut *txn, &sender).await?;
        let is_additional = existing_chat.is_some();

        let (announcement, handle, offer_hash, package_hash, origin_chat_id) = match source {
            IncomingRequestSource::Username {
                username,
                connection_offer_hash,
                connection_package_hash,
            } => (
                SystemMessage::received_handle_connection_request(
                    sender.clone(),
                    username.clone(),
                    is_additional,
                ),
                Some(username),
                Some(connection_offer_hash),
                Some(connection_package_hash),
                None,
            ),
            IncomingRequestSource::Group {
                origin_chat_id,
                chat_name,
            } => (
                SystemMessage::received_direct_connection_request(
                    sender.clone(),
                    chat_name,
                    is_additional,
                ),
                None,
                None,
                None,
                Some(origin_chat_id),
            ),
        };

        let chat_id = match existing_chat {
            Some(chat_id) => chat_id,
            None => {
                let chat = Chat::new_pending_connection_chat(
                    connection_info.connection_group_id.clone(),
                    sender,
                );
                chat.store(&mut *txn).await?;
                chat.id()
            }
        };
        PendingConnectionRequest {
            request_id,
            chat_id,
            created_at: TimeStamp::now(),
            received_at,
            connection_info,
            handle,
            connection_offer_hash: offer_hash,
            connection_package_hash: package_hash,
            origin_chat_id,
        }
        .store(&mut *txn)
        .await?;
        let announcement = TimestampedMessage::system_message(announcement, received_at);
        CoreUser::store_new_messages(txn, chat_id, vec![announcement]).await?;

        let moved_to = rehost(txn, chat_id).await?;
        Ok(StoredRequest {
            chat_id: moved_to.unwrap_or(chat_id),
            moved_from: moved_to.map(|_| chat_id),
        })
    }
}

/// Moves a chat of pending requests to the chat of its newest request, if that
/// is another one. Returns the new chat id.
async fn rehost(
    txn: &mut WriteDbTransaction<'_>,
    chat_id: ChatId,
) -> anyhow::Result<Option<ChatId>> {
    let requests = PendingConnectionRequest::load_for_chat(&mut *txn, chat_id).await?;
    let Some(newest) = requests.first() else {
        return Ok(None);
    };
    if newest.request_id == chat_id {
        return Ok(None);
    }
    let old = Chat::load(&mut *txn, &chat_id)
        .await?
        .with_context(|| format!("no chat {chat_id} for its pending requests"))?;
    let ChatType::PendingConnection(sender) = old.chat_type() else {
        anyhow::bail!("chat {chat_id} with pending requests is not an incoming request");
    };
    let chat = Chat::new_pending_connection_chat(newest.group_id().clone(), sender.clone());
    chat.store(&mut *txn).await?;
    ChatMessage::move_to_chat(&mut *txn, chat_id, chat.id()).await?;
    PendingConnectionRequest::move_to_chat(&mut *txn, chat_id, chat.id()).await?;
    Group::delete_from_db(txn, old.group_id()).await?;
    Chat::delete(&mut *txn, chat_id).await?;
    Ok(Some(chat.id()))
}

/// Returns true if the request is known, either as a pending request or as a
/// chat.
pub(crate) async fn is_known(
    mut txn: impl ReadTransaction,
    request_id: ChatId,
) -> sqlx::Result<bool> {
    Ok(PendingConnectionRequest::load(&mut txn, request_id)
        .await?
        .is_some()
        || Chat::load(txn, &request_id).await?.is_some())
}

/// Clean up the pending requests when the users accepted the connection.
pub(crate) async fn settle_accepted(
    txn: &mut WriteDbTransaction<'_>,
    chat_id: ChatId,
) -> anyhow::Result<()> {
    for request in PendingConnectionRequest::load_for_chat(&mut *txn, chat_id).await? {
        discard_request_state(txn, request).await?;
    }
    Ok(())
}

/// Discards the pending requests of the chat, whose sender is a contact
/// already, and removes the chat.
async fn discard_redundant_chat(
    txn: &mut WriteDbTransaction<'_>,
    chat_id: ChatId,
) -> anyhow::Result<()> {
    let Some(chat) = Chat::load(&mut *txn, &chat_id).await? else {
        return Ok(());
    };
    for request in PendingConnectionRequest::load_for_chat(&mut *txn, chat_id).await? {
        discard_request_state(txn, request).await?;
    }
    Group::delete_from_db(txn, chat.group_id()).await?;
    Chat::delete(&mut *txn, chat_id).await?;
    Ok(())
}

/// Discards the pending requests of `sender`, who just became a contact.
pub(crate) async fn discard_requests_from(
    txn: &mut WriteDbTransaction<'_>,
    sender: &UserId,
) -> anyhow::Result<()> {
    if let Some(chat_id) = PendingConnectionRequest::chat_of_sender(&mut *txn, sender).await? {
        discard_redundant_chat(txn, chat_id).await?;
    }
    Ok(())
}

/// Deletes a pending request and what accepting it would have used: its
/// connection-offer PSK and the key of the connection package it consumed.
async fn discard_request_state(
    txn: &mut WriteDbTransaction<'_>,
    request: PendingConnectionRequest,
) -> anyhow::Result<()> {
    if let Some(hash) = request.connection_offer_hash {
        Group::delete_connection_offer_psk(&mut *txn, hash)?;
    }
    if let Some(hash) = request.connection_package_hash {
        delete_consumed_package_key(txn, &hash).await?;
    }
    PendingConnectionRequest::delete(&mut *txn, request.request_id).await?;
    Ok(())
}

/// Deletes the key of a connection package consumed by a request, unless it is
/// the last-resort package, which stays published.
pub(crate) async fn delete_consumed_package_key(
    txn: &mut WriteDbTransaction<'_>,
    hash: &ConnectionPackageHash,
) -> sqlx::Result<()> {
    let is_last_resort = ConnectionPackageRecord::load_is_last_resort(&mut *txn, hash)
        .await?
        .unwrap_or(false);
    if !is_last_resort {
        ConnectionPackageRecord::delete(&mut *txn, hash).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use aircommon::{
        credentials::keys::UsernameSigningKey,
        crypto::{
            aead::keys::{
                FriendshipPackageEarKey, GroupStateEarKey, IdentityLinkWrapperKey,
                WelcomeAttributionInfoEarKey,
            },
            indexed_aead::keys::UserProfileBaseSecret,
        },
        identifiers::QualifiedGroupId,
        messages::{FriendshipToken, connection_package::ConnectionPackage},
    };
    use chrono::{DateTime, Utc};
    use openmls::group::GroupId;
    use uuid::Uuid;

    use crate::{
        ChatAttributes, UsernameRecord,
        chats::messages::{EventMessage, Message},
        clients::connection_offer::FriendshipPackage,
        db::access::DbAccess,
        utils::persistence::open_db_in_memory,
    };

    use super::*;

    fn group_id() -> GroupId {
        QualifiedGroupId::new(Uuid::new_v4(), "example.com".parse().unwrap()).into()
    }

    fn connection_info() -> anyhow::Result<ConnectionInfo> {
        Ok(ConnectionInfo {
            connection_group_id: group_id(),
            connection_group_ear_key: GroupStateEarKey::random()?,
            connection_group_identity_link_wrapper_key: IdentityLinkWrapperKey::random()?,
            friendship_package_ear_key: FriendshipPackageEarKey::random()?,
            friendship_package: FriendshipPackage {
                friendship_token: FriendshipToken::random()?,
                wai_ear_key: WelcomeAttributionInfoEarKey::random()?,
                user_profile_base_secret: UserProfileBaseSecret::random()?,
            },
        })
    }

    async fn db() -> anyhow::Result<DbAccess> {
        Ok(DbAccess::for_tests(open_db_in_memory().await?))
    }

    fn sender() -> anyhow::Result<UserId> {
        Ok(UserId::random("example.com".parse()?))
    }

    fn at(seconds: i64) -> TimeStamp {
        DateTime::<Utc>::from_timestamp(1_767_225_600 + seconds, 0)
            .unwrap()
            .into()
    }

    /// Stores an incoming request via `username`, as processing its offer
    /// does.
    async fn receive_request_from(
        txn: &mut WriteDbTransaction<'_>,
        sender: &UserId,
        username: &str,
        received_at: TimeStamp,
    ) -> anyhow::Result<StoredRequest> {
        IncomingRequest {
            connection_info: connection_info()?,
            sender: sender.clone(),
            source: IncomingRequestSource::Username {
                username: Username::new(username.to_owned())?,
                connection_offer_hash: ConnectionOfferHash::new_for_test(vec![1; 32]),
                connection_package_hash: ConnectionPackageHash::new_for_test(vec![2; 32]),
            },
            received_at,
        }
        .store(txn)
        .await
    }

    async fn system_messages(
        txn: &mut WriteDbTransaction<'_>,
        chat_id: ChatId,
    ) -> anyhow::Result<Vec<SystemMessage>> {
        Ok(ChatMessage::load_multiple(&mut *txn, chat_id, 100)
            .await?
            .into_iter()
            .filter_map(|message| match message.into_message() {
                Message::Event(EventMessage::System(system)) => Some(system),
                _ => None,
            })
            .collect())
    }

    async fn request_ids(
        txn: &mut WriteDbTransaction<'_>,
        chat_id: ChatId,
    ) -> anyhow::Result<Vec<ChatId>> {
        Ok(PendingConnectionRequest::load_for_chat(&mut *txn, chat_id)
            .await?
            .into_iter()
            .map(|request| request.request_id)
            .collect())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_newer_request_of_the_same_sender_takes_over_the_chat() -> anyhow::Result<()> {
        let db = db().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let sender = sender()?;
            let first = receive_request_from(txn, &sender, "ellie-03", at(0)).await?;
            let second = receive_request_from(txn, &sender, "ellie-04", at(60)).await?;

            assert_ne!(second.chat_id, first.chat_id);
            assert_eq!(second.moved_from, Some(first.chat_id));
            assert!(Chat::load(&mut *txn, &first.chat_id).await?.is_none());
            // The chat is the one of the newest request.
            assert_eq!(
                request_ids(txn, second.chat_id).await?,
                vec![second.chat_id, first.chat_id]
            );
            assert_eq!(
                system_messages(txn, second.chat_id).await?,
                vec![
                    SystemMessage::ReceivedHandleConnectionRequest {
                        sender: sender.clone(),
                        user_handle: Username::new("ellie-03".to_owned())?,
                    },
                    SystemMessage::ReceivedAdditionalHandleConnectionRequest {
                        sender: sender.clone(),
                        user_handle: Username::new("ellie-04".to_owned())?,
                    },
                ]
            );
            Ok(())
        })
        .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_older_request_joins_the_chat_without_moving_it() -> anyhow::Result<()> {
        let db = db().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let sender = sender()?;
            let newer = receive_request_from(txn, &sender, "ellie-03", at(60)).await?;
            let older = receive_request_from(txn, &sender, "ellie-04", at(0)).await?;

            assert_eq!(older.chat_id, newer.chat_id);
            assert_eq!(older.moved_from, None);
            assert_eq!(request_ids(txn, newer.chat_id).await?.len(), 2);
            Ok(())
        })
        .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn requests_of_different_senders_stay_apart() -> anyhow::Result<()> {
        let db = db().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let ellie = receive_request_from(txn, &sender()?, "ellie-03", at(0)).await?;
            let joel = receive_request_from(txn, &sender()?, "ellie-03", at(60)).await?;

            assert_ne!(ellie.chat_id, joel.chat_id);
            assert_eq!(joel.moved_from, None);
            assert_eq!(request_ids(txn, ellie.chat_id).await?.len(), 1);
            assert_eq!(request_ids(txn, joel.chat_id).await?.len(), 1);
            Ok(())
        })
        .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_group_request_outlives_the_group_chat_it_came_through() -> anyhow::Result<()> {
        let db = db().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let group = Chat::new_group_chat(
                group_id(),
                ChatAttributes::new("Design Team".to_owned(), None),
            );
            group.store(&mut *txn).await?;
            let stored = IncomingRequest {
                connection_info: connection_info()?,
                sender: sender()?,
                source: IncomingRequestSource::Group {
                    origin_chat_id: group.id(),
                    chat_name: "Design Team".to_owned(),
                },
                received_at: at(0),
            }
            .store(txn)
            .await?;
            let request = PendingConnectionRequest::load(&mut *txn, stored.chat_id)
                .await?
                .unwrap();
            assert_eq!(request.origin_chat_id, Some(group.id()));

            Chat::delete(&mut *txn, group.id()).await?;

            let request = PendingConnectionRequest::load(&mut *txn, stored.chat_id)
                .await?
                .unwrap();
            assert_eq!(request.origin_chat_id, None);
            Ok(())
        })
        .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_request_shown_in_the_chat_of_a_newer_one_is_known() -> anyhow::Result<()> {
        let db = db().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let sender = sender()?;
            let older = receive_request_from(txn, &sender, "ellie-03", at(0)).await?;
            receive_request_from(txn, &sender, "ellie-04", at(60)).await?;

            assert!(Chat::load(&mut *txn, &older.chat_id).await?.is_none());
            assert!(is_known(&mut *txn, older.chat_id).await?);
            assert!(!is_known(&mut *txn, ChatId::new(Uuid::new_v4())).await?);
            Ok(())
        })
        .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn accepting_settles_every_request_of_the_chat() -> anyhow::Result<()> {
        let db = db().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let sender = sender()?;
            receive_request_from(txn, &sender, "ellie-03", at(0)).await?;
            let newest = receive_request_from(txn, &sender, "ellie-04", at(60)).await?;

            settle_accepted(txn, newest.chat_id).await?;

            assert!(request_ids(txn, newest.chat_id).await?.is_empty());
            assert!(Chat::load(&mut *txn, &newest.chat_id).await?.is_some());
            Ok(())
        })
        .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_requests_of_a_new_contact_are_discarded() -> anyhow::Result<()> {
        let db = db().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let contact = sender()?;
            let stranger = sender()?;
            receive_request_from(txn, &contact, "ellie-03", at(0)).await?;
            let newest = receive_request_from(txn, &contact, "ellie-04", at(60)).await?;
            let other = receive_request_from(txn, &stranger, "ellie-03", at(0)).await?;

            discard_requests_from(txn, &contact).await?;

            assert!(Chat::load(&mut *txn, &newest.chat_id).await?.is_none());
            assert!(request_ids(txn, newest.chat_id).await?.is_empty());
            assert!(
                PendingConnectionRequest::chat_of_sender(&mut *txn, &contact)
                    .await?
                    .is_none()
            );
            assert_eq!(request_ids(txn, other.chat_id).await?.len(), 1);
            Ok(())
        })
        .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn deleting_a_consumed_key_keeps_the_last_resort_one() -> anyhow::Result<()> {
        let db = db().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let username = Username::new("ellie-03".to_owned())?;
            let record = UsernameRecord::new(
                username.clone(),
                username.calculate_hash()?,
                UsernameSigningKey::generate()?,
            );
            record.store(&mut *txn).await?;
            let mut hashes = Vec::new();
            for last_resort in [false, true] {
                let (key, _package, metadata) =
                    ConnectionPackage::generate(record.hash, &record.signing_key, last_resort)?;
                hashes.push(metadata.hash);
                ConnectionPackageRecord::from(metadata)
                    .store_for_username(&mut *txn, &username, &key)
                    .await?;
            }
            for hash in &hashes {
                delete_consumed_package_key(txn, hash).await?;
            }
            let [consumed, last_resort] = hashes.as_slice() else {
                unreachable!()
            };
            assert_eq!(
                ConnectionPackageRecord::load_is_last_resort(&mut *txn, consumed).await?,
                None
            );
            assert_eq!(
                ConnectionPackageRecord::load_is_last_resort(&mut *txn, last_resort).await?,
                Some(true)
            );
            Ok(())
        })
        .await
    }
}
