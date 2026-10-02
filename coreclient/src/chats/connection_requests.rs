// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Incoming connection requests lifecycle.
//!
//! All pending incoming requests of one sender share a chat, the one of the
//! newest request. A newer request moves the chat's messages to a chat of its
//! own group.
//!
//! An offer to a username reaches only the device that fetched it from the AS
//! queue. That device forwards the verified request to its siblings through
//! the self-group. A request from a group chat needs no forwarding, since the
//! targeted message reaches every device of the user.

use aircommon::{
    codec::PersistenceCodec,
    credentials::UserCredential,
    identifiers::{UserId, Username},
    messages::{client_as::ConnectionOfferHash, connection_package::ConnectionPackageHash},
    time::TimeStamp,
};
use airprotos::client::self_group::{
    ConnectionRequestEntry, ConnectionRequestGroup, ConnectionRequestReceived,
    ConnectionRequestSource,
};
use anyhow::Context;
use chrono::{DateTime, Utc};
use openmls::group::GroupId;
use tls_codec::{DeserializeBytes, Serialize as _};
use tracing::{debug, warn};
use uuid::Uuid;

use crate::{
    Chat, ChatId, ChatMessage, ChatType, SystemMessage,
    chats::{PendingConnectionRequest, messages::TimestampedMessage},
    clients::{
        CoreUser,
        block_contact::BlockedContact,
        connection_offer::payload::ConnectionInfo,
        own_client_info::OwnClientInfo,
        self_group_outbox::{self, OutboxKind},
    },
    db::access::{ReadDbTransaction, ReadTransaction, WriteDbTransaction},
    groups::{Group, client_auth_info::StorableUserCredential},
    usernames::connection_packages::ConnectionPackageRecord,
    utils::persistence::GroupIdWrapper,
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
        /// The group chat may not exist on this device, when a sibling handed
        /// the request over.
        origin_group_id: GroupId,
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
        let chat_id = match existing_chat {
            Some(chat_id) => chat_id,
            None => {
                let chat = Chat::new_pending_connection_chat(
                    connection_info.connection_group_id.clone(),
                    sender.clone(),
                );
                chat.store(&mut *txn).await?;
                chat.id()
            }
        };

        let request = PendingConnectionRequest {
            request_id,
            chat_id,
            created_at: TimeStamp::now(),
            received_at,
            connection_info,
            username: None,
            connection_offer_hash: None,
            connection_package_hash: None,
            origin_group_id: None,
        };
        let (announcement, request) = match source {
            IncomingRequestSource::Username {
                username,
                connection_offer_hash,
                connection_package_hash,
            } => (
                SystemMessage::received_username_connection_request(
                    sender,
                    username.clone(),
                    is_additional,
                ),
                PendingConnectionRequest {
                    username: Some(username),
                    connection_offer_hash: Some(connection_offer_hash),
                    connection_package_hash: Some(connection_package_hash),
                    ..request
                },
            ),
            IncomingRequestSource::Group { origin_group_id } => (
                SystemMessage::received_group_connection_request(
                    sender,
                    ChatId::try_from(&origin_group_id)
                        .context("invalid origin group id of a request")?,
                    is_additional,
                ),
                PendingConnectionRequest {
                    origin_group_id: Some(GroupIdWrapper(origin_group_id)),
                    ..request
                },
            ),
        };
        request.store(&mut *txn).await?;
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

/// Helper struct for notifications, to track the effect of connection requests.
#[derive(Debug, Default)]
pub(crate) struct ConnectionRequestEffects {
    /// Chats of incoming requests from a sibling.
    pub(crate) new_requests: Vec<ChatId>,
    /// Chats whose notifications are stale, because a newer request moved
    /// them.
    pub(crate) stale_chats: Vec<ChatId>,
}

/// Parks a stored request via a username for the next self-group commit, so
/// the siblings store it too.
pub(crate) async fn park_received(
    txn: &mut WriteDbTransaction<'_>,
    request_id: ChatId,
    sender_credential: &UserCredential,
) -> anyhow::Result<()> {
    if OwnClientInfo::load_self_group_id(&mut *txn)
        .await?
        .is_none()
    {
        return Ok(());
    }
    let Some(request) = PendingConnectionRequest::load(&mut *txn, request_id).await? else {
        return Ok(());
    };
    let Some(received) = received_entry(&request, sender_credential)? else {
        return Ok(());
    };
    let entry = ConnectionRequestEntry::Received(received);
    self_group_outbox::stage(
        &mut *txn,
        OutboxKind::ConnectionRequest,
        request_id.uuid().as_bytes(),
        &PersistenceCodec::to_vec(&entry)?,
        None,
    )
    .await?;
    Ok(())
}

/// The parked entries of the requests that are still pending, sorted by
/// request id for canonical encoding. Drops the other entries.
pub(crate) async fn staged_entries(
    txn: &mut WriteDbTransaction<'_>,
) -> anyhow::Result<Vec<ConnectionRequestEntry>> {
    let staged = self_group_outbox::load_kind(&mut *txn, OutboxKind::ConnectionRequest).await?;
    let mut entries = Vec::with_capacity(staged.len());
    for staged in staged {
        let request_id = ChatId::new(Uuid::from_slice(&staged.key)?);
        if PendingConnectionRequest::load(&mut *txn, request_id)
            .await?
            .is_some()
        {
            entries.push(PersistenceCodec::from_slice(&staged.payload)?);
        } else {
            self_group_outbox::remove(&mut *txn, OutboxKind::ConnectionRequest, &staged.key)
                .await?;
        }
    }
    Ok(entries)
}

/// Drops the parked entries.
pub(crate) async fn complete_sent_entries(
    txn: &mut WriteDbTransaction<'_>,
    sent: &[ConnectionRequestEntry],
) -> anyhow::Result<()> {
    for entry in sent {
        let Some(request_id) = request_id_of(entry) else {
            continue;
        };
        self_group_outbox::complete_sent(
            &mut *txn,
            OutboxKind::ConnectionRequest,
            request_id.uuid().as_bytes(),
            &PersistenceCodec::to_vec(entry)?,
        )
        .await?;
    }
    Ok(())
}

/// Applies the entries of a sibling's accepted connection-requests update.
/// Siblings only forward requests via a username, so an entry via a group is
/// skipped.
pub(crate) async fn apply_connection_requests_update(
    txn: &mut WriteDbTransaction<'_>,
    entries: &[ConnectionRequestEntry],
) -> anyhow::Result<ConnectionRequestEffects> {
    let mut effects = ConnectionRequestEffects::default();
    for entry in entries {
        let Some(request_id) = request_id_of(entry) else {
            debug!("Skipping a connection-request entry this client cannot read");
            continue;
        };
        match entry {
            ConnectionRequestEntry::Received(received) => match &received.source {
                ConnectionRequestSource::Group(_) => {
                    warn!(%request_id, "Skipping a forwarded request via a group");
                }
                ConnectionRequestSource::Username(_) | ConnectionRequestSource::Unknown => {
                    if let Some(stored) = store_received(txn, received).await? {
                        effects.new_requests.push(stored.chat_id);
                        effects.stale_chats.extend(stored.moved_from);
                    }
                }
            },
            ConnectionRequestEntry::Unknown => {}
        }
        self_group_outbox::remove(
            &mut *txn,
            OutboxKind::ConnectionRequest,
            request_id.uuid().as_bytes(),
        )
        .await?;
    }
    Ok(effects)
}

/// Stores the pending requests of a provisioning package.
pub(crate) async fn store_provisioned_requests(
    txn: &mut WriteDbTransaction<'_>,
    entries: &[ConnectionRequestEntry],
) -> anyhow::Result<()> {
    for entry in entries {
        match entry {
            ConnectionRequestEntry::Received(received) => {
                store_received(txn, received).await?;
            }
            ConnectionRequestEntry::Unknown => {
                debug!("Skipping a provisioned connection request this client cannot read");
            }
        }
    }
    Ok(())
}

/// Every pending incoming request, for the provisioning package.
pub(crate) async fn pending_requests_snapshot(
    txn: &mut ReadDbTransaction<'_>,
) -> anyhow::Result<Vec<ConnectionRequestEntry>> {
    let requests = PendingConnectionRequest::load_all(&mut *txn).await?;
    let mut entries = Vec::with_capacity(requests.len());
    for request in requests {
        let Some(chat) = Chat::load(&mut *txn, &request.chat_id).await? else {
            continue;
        };
        let ChatType::PendingConnection(sender) = chat.chat_type() else {
            continue;
        };
        let Some(credential) = StorableUserCredential::load_by_user_id(&mut *txn, sender).await?
        else {
            let request_id = request.request_id;
            warn!(%request_id, "No credential of the sender of a pending request");
            continue;
        };
        let credential = UserCredential::from(credential);
        if let Some(received) = received_entry(&request, &credential)? {
            entries.push(ConnectionRequestEntry::Received(received));
        }
    }
    Ok(entries)
}

/// The id of the request an entry is about.
fn request_id_of(entry: &ConnectionRequestEntry) -> Option<ChatId> {
    match entry {
        ConnectionRequestEntry::Received(received) => {
            let info =
                ConnectionInfo::tls_deserialize_exact_bytes(&received.connection_info).ok()?;
            ChatId::try_from(&info.connection_group_id).ok()
        }
        ConnectionRequestEntry::Unknown => None,
    }
}

/// The received entry of a pending incoming request, if there is one.
fn received_entry(
    request: &PendingConnectionRequest,
    sender_credential: &UserCredential,
) -> anyhow::Result<Option<ConnectionRequestReceived>> {
    let request_id = request.request_id;
    let source = if let Some(username) = &request.username {
        ConnectionRequestSource::Username(username.plaintext().to_owned())
    } else if let Some(GroupIdWrapper(group_id)) = &request.origin_group_id {
        ConnectionRequestSource::Group(ConnectionRequestGroup {
            group_id: Some(group_id.clone()),
        })
    } else {
        debug!(%request_id, "The group chat of a pending request is unknown");
        return Ok(None);
    };
    Ok(Some(ConnectionRequestReceived {
        connection_info: request.connection_info.tls_serialize_detached()?,
        sender_credential: sender_credential.tls_serialize_detached()?,
        source,
        connection_offer_hash: request.connection_offer_hash,
        connection_package_hash: request.connection_package_hash,
        received_at: request.received_at.timestamp_millis().max(0) as u64,
    }))
}

/// Stores a request from  a sibling, unless it is known already or from a
/// blocked sender.
async fn store_received(
    txn: &mut WriteDbTransaction<'_>,
    received: &ConnectionRequestReceived,
) -> anyhow::Result<Option<StoredRequest>> {
    let Some((request, credential)) = parse_received(received) else {
        return Ok(None);
    };
    let request_id = request.request_id()?;
    if is_known(&mut *txn, request_id).await?
        || BlockedContact::check_blocked(&mut *txn, &request.sender).await?
    {
        return Ok(None);
    }

    StorableUserCredential::new(credential.clone())
        .store(&mut *txn)
        .await?;
    let stored = request.store(txn).await?;
    CoreUser::schedule_fetch_request_sender_profile(&mut *txn, request_id, credential).await?;
    Ok(Some(stored))
}

fn parse_received(
    received: &ConnectionRequestReceived,
) -> Option<(IncomingRequest, UserCredential)> {
    let connection_info = ConnectionInfo::tls_deserialize_exact_bytes(&received.connection_info)
        .inspect_err(|error| warn!(%error, "Skipping a request with invalid connection info"))
        .ok()?;
    let credential = UserCredential::tls_deserialize_exact_bytes(&received.sender_credential)
        .inspect_err(|error| warn!(%error, "Skipping a request with an invalid credential"))
        .ok()?;
    let source = match &received.source {
        ConnectionRequestSource::Username(username) => {
            let username = Username::new(username.clone())
                .inspect_err(|error| warn!(%error, "Skipping a request via an invalid username"))
                .ok()?;
            let (Some(connection_offer_hash), Some(connection_package_hash)) = (
                received.connection_offer_hash,
                received.connection_package_hash,
            ) else {
                warn!("Skipping a request via a username without its offer hashes");
                return None;
            };
            IncomingRequestSource::Username {
                username,
                connection_offer_hash,
                connection_package_hash,
            }
        }
        ConnectionRequestSource::Group(ConnectionRequestGroup { group_id }) => {
            let Some(group_id) = group_id else {
                warn!("Skipping a request via a group without its group id");
                return None;
            };
            IncomingRequestSource::Group {
                origin_group_id: group_id.clone(),
            }
        }
        ConnectionRequestSource::Unknown => {
            debug!("Skipping a request from an unknown source");
            return None;
        }
    };
    let Some(received_at) = i64::try_from(received.received_at)
        .ok()
        .and_then(DateTime::<Utc>::from_timestamp_millis)
    else {
        warn!("Skipping a request with an invalid timestamp");
        return None;
    };
    let request = IncomingRequest {
        connection_info,
        sender: credential.user_id().clone(),
        source,
        received_at: received_at.into(),
    };
    Some((request, credential))
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
        credentials::{keys::UsernameSigningKey, test_utils::create_test_credentials},
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
                    SystemMessage::ReceivedAdditionalUsernameConnectionRequest {
                        sender: sender.clone(),
                        username: Username::new("ellie-04".to_owned())?,
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
                    origin_group_id: group.group_id().clone(),
                },
                received_at: at(0),
            }
            .store(txn)
            .await?;

            Chat::delete(&mut *txn, group.id()).await?;

            let request = PendingConnectionRequest::load(&mut *txn, stored.chat_id)
                .await?
                .unwrap();
            let origin_group_id = request.origin_group_id.map(GroupId::from);
            assert_eq!(origin_group_id.as_ref(), Some(group.group_id()));
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

    /// The database of a device that is linked to siblings.
    async fn linked_device() -> anyhow::Result<DbAccess> {
        let db = db().await?;
        db.with_write_transaction(async |txn| {
            OwnClientInfo::store_for_test(txn, Some(group_id())).await
        })
        .await?;
        Ok(db)
    }

    /// A sender whose credential is stored, as processing an offer leaves it.
    async fn sender_with_credential(
        txn: &mut WriteDbTransaction<'_>,
    ) -> anyhow::Result<UserCredential> {
        let (_as_key, signing_key) = create_test_credentials(sender()?);
        let credential = signing_key.credential().clone();
        StorableUserCredential::new(credential.clone())
            .store(&mut *txn)
            .await?;
        Ok(credential)
    }

    /// Receives a request via a username and parks it, as processing its offer
    /// does. Returns the parked entries.
    async fn receive_and_park(
        txn: &mut WriteDbTransaction<'_>,
        credential: &UserCredential,
        received_at: TimeStamp,
    ) -> anyhow::Result<(StoredRequest, Vec<ConnectionRequestEntry>)> {
        let stored =
            receive_request_from(txn, credential.user_id(), "ellie-03", received_at).await?;
        park_received(txn, stored.chat_id, credential).await?;
        let sent = staged_entries(txn).await?;
        assert_eq!(sent.len(), 1, "expected the request to be parked");
        Ok((stored, sent))
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_forwarded_request_is_stored_like_a_received_one() -> anyhow::Result<()> {
        let db = linked_device().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let credential = sender_with_credential(txn).await?;
            let (stored, sent) = receive_and_park(txn, &credential, at(0)).await?;
            let [ConnectionRequestEntry::Received(received)] = sent.as_slice() else {
                panic!("expected one received request, got {sent:?}");
            };
            let before = system_messages(txn, stored.chat_id).await?;

            // Starting over stands in for the sibling that never saw the offer.
            Chat::delete(&mut *txn, stored.chat_id).await?;
            let effects = apply_connection_requests_update(txn, &sent).await?;

            assert_eq!(effects.new_requests, vec![stored.chat_id]);
            let chat = Chat::load(&mut *txn, &stored.chat_id).await?.unwrap();
            assert_eq!(
                chat.chat_type(),
                &ChatType::PendingConnection(credential.user_id().clone())
            );
            let pending = PendingConnectionRequest::load(&mut *txn, stored.chat_id)
                .await?
                .unwrap();
            assert_eq!(
                pending.username,
                Some(Username::new("ellie-03".to_owned())?)
            );
            assert_eq!(
                pending.connection_offer_hash,
                received.connection_offer_hash
            );
            assert_eq!(pending.received_at, at(0));
            assert_eq!(system_messages(txn, stored.chat_id).await?, before);
            Ok(())
        })
        .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_forwarded_newer_request_moves_the_chat() -> anyhow::Result<()> {
        let db = linked_device().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let credential = sender_with_credential(txn).await?;
            let (newer, sent) = receive_and_park(txn, &credential, at(60)).await?;
            Chat::delete(&mut *txn, newer.chat_id).await?;
            let older = receive_request_from(txn, credential.user_id(), "ellie-04", at(0)).await?;

            let effects = apply_connection_requests_update(txn, &sent).await?;

            assert_eq!(effects.new_requests, vec![newer.chat_id]);
            assert_eq!(effects.stale_chats, vec![older.chat_id]);
            assert_eq!(
                request_ids(txn, newer.chat_id).await?,
                vec![newer.chat_id, older.chat_id]
            );
            Ok(())
        })
        .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_forwarded_request_from_a_blocked_sender_is_dropped() -> anyhow::Result<()> {
        let db = linked_device().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let credential = sender_with_credential(txn).await?;
            let (stored, sent) = receive_and_park(txn, &credential, at(0)).await?;
            Chat::delete(&mut *txn, stored.chat_id).await?;
            BlockedContact {
                user_id: credential.user_id().clone(),
                last_display_name: "Ellie".parse()?,
                blocked_at: Utc::now(),
            }
            .store(&mut *txn)
            .await?;

            let effects = apply_connection_requests_update(txn, &sent).await?;

            assert!(effects.new_requests.is_empty());
            assert!(Chat::load(&mut *txn, &stored.chat_id).await?.is_none());
            Ok(())
        })
        .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_unreadable_entry_is_skipped() -> anyhow::Result<()> {
        let db = linked_device().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let credential = sender_with_credential(txn).await?;
            let (_stored, sent) = receive_and_park(txn, &credential, at(0)).await?;
            let [ConnectionRequestEntry::Received(received)] = sent.as_slice() else {
                panic!("expected one received request, got {sent:?}");
            };
            let garbled = ConnectionRequestEntry::Received(ConnectionRequestReceived {
                connection_info: vec![1, 2, 3],
                ..received.clone()
            });
            let unknown_source = ConnectionRequestEntry::Received(ConnectionRequestReceived {
                source: ConnectionRequestSource::Unknown,
                ..received.clone()
            });

            let effects = apply_connection_requests_update(
                txn,
                &[garbled, unknown_source, ConnectionRequestEntry::Unknown],
            )
            .await?;
            assert!(effects.new_requests.is_empty());
            Ok(())
        })
        .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_committed_request_leaves_the_outbox() -> anyhow::Result<()> {
        let db = linked_device().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let credential = sender_with_credential(txn).await?;
            let (_stored, sent) = receive_and_park(txn, &credential, at(0)).await?;

            // The own echo carries the entries as they came off the wire.
            let echoed: Vec<ConnectionRequestEntry> =
                PersistenceCodec::from_slice(&PersistenceCodec::to_vec(&sent)?)?;
            complete_sent_entries(txn, &echoed).await?;
            assert!(staged_entries(txn).await?.is_empty());
            Ok(())
        })
        .await
    }

    /// Both devices fetched the same offer. The one whose commit lost takes the
    /// winner's entry and has nothing left to send.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_sibling_forwarding_the_same_request_clears_ours() -> anyhow::Result<()> {
        let db = linked_device().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let credential = sender_with_credential(txn).await?;
            let (_stored, sent) = receive_and_park(txn, &credential, at(0)).await?;

            let effects = apply_connection_requests_update(txn, &sent).await?;

            assert!(effects.new_requests.is_empty());
            assert!(staged_entries(txn).await?.is_empty());
            Ok(())
        })
        .await
    }

    /// A request parked as received that was accepted before its commit went
    /// out has nothing left to forward.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_request_that_is_no_longer_pending_is_not_forwarded() -> anyhow::Result<()> {
        let db = linked_device().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let credential = sender_with_credential(txn).await?;
            let (stored, _sent) = receive_and_park(txn, &credential, at(0)).await?;
            PendingConnectionRequest::delete(&mut *txn, stored.chat_id).await?;

            assert!(staged_entries(txn).await?.is_empty());
            let key = stored.chat_id.uuid();
            let parked =
                self_group_outbox::load(&mut *txn, OutboxKind::ConnectionRequest, key.as_bytes())
                    .await?;
            assert_eq!(parked, None);
            Ok(())
        })
        .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_device_that_was_never_linked_parks_nothing() -> anyhow::Result<()> {
        let db = db().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            OwnClientInfo::store_for_test(&mut *txn, None).await?;
            let credential = sender_with_credential(txn).await?;
            let stored = receive_request_from(txn, credential.user_id(), "ellie-03", at(0)).await?;

            park_received(txn, stored.chat_id, &credential).await?;
            assert!(staged_entries(txn).await?.is_empty());
            Ok(())
        })
        .await
    }

    /// Stores a request through a new group chat "Design Team", as processing
    /// a targeted message does.
    async fn receive_via_group(
        txn: &mut WriteDbTransaction<'_>,
        sender: &UserId,
    ) -> anyhow::Result<(Chat, StoredRequest)> {
        let group = Chat::new_group_chat(
            group_id(),
            ChatAttributes::new("Design Team".to_owned(), None),
        );
        group.store(&mut *txn).await?;
        let stored = IncomingRequest {
            connection_info: connection_info()?,
            sender: sender.clone(),
            source: IncomingRequestSource::Group {
                origin_group_id: group.group_id().clone(),
            },
            received_at: at(0),
        }
        .store(txn)
        .await?;
        Ok((group, stored))
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_group_request_is_provisioned_after_its_group_chat_is_gone() -> anyhow::Result<()> {
        let db = linked_device().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let credential = sender_with_credential(txn).await?;
            let (group, _stored) = receive_via_group(txn, credential.user_id()).await?;
            Chat::delete(&mut *txn, group.id()).await?;

            let snapshot = pending_requests_snapshot(&mut txn.begin_read().await?).await?;
            let [ConnectionRequestEntry::Received(received)] = snapshot.as_slice() else {
                panic!("expected one received request, got {snapshot:?}");
            };
            assert_eq!(
                received.source,
                ConnectionRequestSource::Group(ConnectionRequestGroup {
                    group_id: Some(group.group_id().clone()),
                })
            );
            assert_eq!(received.connection_offer_hash, None);
            Ok(())
        })
        .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_handed_over_group_request_is_provisioned_again() -> anyhow::Result<()> {
        let db = linked_device().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let credential = sender_with_credential(txn).await?;
            let (group, stored) = receive_via_group(txn, credential.user_id()).await?;
            let snapshot = pending_requests_snapshot(&mut txn.begin_read().await?).await?;

            // Starting over stands in for the new device, which has no group
            // chats yet.
            Chat::delete(&mut *txn, stored.chat_id).await?;
            Chat::delete(&mut *txn, group.id()).await?;
            store_provisioned_requests(txn, &snapshot).await?;

            let pending = PendingConnectionRequest::load(&mut *txn, stored.chat_id)
                .await?
                .unwrap();
            let origin_group_id = pending.origin_group_id.map(GroupId::from);
            assert_eq!(origin_group_id.as_ref(), Some(group.group_id()));
            assert_eq!(
                system_messages(txn, stored.chat_id).await?,
                vec![SystemMessage::ReceivedGroupConnectionRequest {
                    sender: credential.user_id().clone(),
                    origin_chat_id: group.id(),
                }]
            );
            assert_eq!(
                pending_requests_snapshot(&mut txn.begin_read().await?).await?,
                snapshot
            );
            Ok(())
        })
        .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_forwarded_request_via_a_group_is_skipped() -> anyhow::Result<()> {
        let db = linked_device().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let credential = sender_with_credential(txn).await?;
            let (group, stored) = receive_via_group(txn, credential.user_id()).await?;
            let snapshot = pending_requests_snapshot(&mut txn.begin_read().await?).await?;
            Chat::delete(&mut *txn, stored.chat_id).await?;
            Chat::delete(&mut *txn, group.id()).await?;

            let effects = apply_connection_requests_update(txn, &snapshot).await?;
            assert!(effects.new_requests.is_empty());
            assert!(
                PendingConnectionRequest::load(&mut *txn, stored.chat_id)
                    .await?
                    .is_none()
            );
            Ok(())
        })
        .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_group_request_without_its_group_id_is_skipped() -> anyhow::Result<()> {
        let db = linked_device().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let credential = sender_with_credential(txn).await?;
            let (group, stored) = receive_via_group(txn, credential.user_id()).await?;
            let snapshot = pending_requests_snapshot(&mut txn.begin_read().await?).await?;
            let [ConnectionRequestEntry::Received(received)] = snapshot.as_slice() else {
                panic!("expected one received request, got {snapshot:?}");
            };
            let without_group_id = ConnectionRequestEntry::Received(ConnectionRequestReceived {
                source: ConnectionRequestSource::Group(ConnectionRequestGroup { group_id: None }),
                ..received.clone()
            });
            Chat::delete(&mut *txn, stored.chat_id).await?;
            Chat::delete(&mut *txn, group.id()).await?;

            store_provisioned_requests(txn, &[without_group_id]).await?;
            assert!(
                PendingConnectionRequest::load(&mut *txn, stored.chat_id)
                    .await?
                    .is_none()
            );
            Ok(())
        })
        .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_request_without_the_sender_credential_is_not_provisioned() -> anyhow::Result<()> {
        let db = linked_device().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let credential = sender_with_credential(txn).await?;
            receive_request_from(txn, credential.user_id(), "ellie-03", at(0)).await?;
            receive_request_from(txn, &sender()?, "ellie-03", at(0)).await?;

            let snapshot = pending_requests_snapshot(&mut txn.begin_read().await?).await?;
            let [ConnectionRequestEntry::Received(received)] = snapshot.as_slice() else {
                panic!("expected one received request, got {snapshot:?}");
            };
            assert_eq!(
                received.sender_credential,
                credential.tls_serialize_detached()?
            );
            Ok(())
        })
        .await
    }
}
