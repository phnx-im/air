// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Expiry of connection requests.
//!
//! A request expires [`REQUEST_LIFETIME`] after it was sent. Every device
//! counts down on its own, from the server's time for an incoming request and
//! from the announcement for an outgoing one. All devices of a user hold the
//! same time, so they expire a request together without telling each other.
//!
//! An expired outgoing request also loses its connection group on the DS, so a
//! late accept finds it gone. The siblings get the delete commit, and close
//! the request as expired instead of erasing it as retracted, since its
//! deadline has passed.

use aircommon::credentials::keys::UserSigningKey;
use chrono::{DateTime, Duration, Utc};
use tracing::warn;

use crate::{
    Chat, ChatId, ChatStatus, ChatType, SystemMessage,
    chats::{
        DeclinedConnectionRequest, PendingConnectionRequest, connection_requests,
        messages::{ChatMessage, EventMessage, Message},
        outgoing_requests, persistence,
    },
    clients::{
        CoreUser,
        user_settings::{DeleteExpiredContactRequestsSetting, UserSetting, UserSettingRecord},
    },
    db::access::{ReadConnection, ReadTransaction, WriteDbTransaction},
    job::pending_chat_operation::PendingChatOperation,
};

/// How long a connection request stays open.
pub(crate) const REQUEST_LIFETIME: Duration = Duration::days(30);

pub(crate) fn expires_at(sent_at: DateTime<Utc>) -> DateTime<Utc> {
    sent_at + REQUEST_LIFETIME
}

pub(crate) fn has_expired(sent_at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    expires_at(sent_at) <= now
}

/// When the outgoing request of the chat was sent, which is the time of its
/// announcement, the oldest message of the chat.
pub(crate) async fn outgoing_sent_at(
    connection: impl ReadConnection,
    chat_id: ChatId,
) -> sqlx::Result<Option<DateTime<Utc>>> {
    ChatMessage::first_timestamp(connection, chat_id).await
}

/// Closes the outgoing request `chat` stands for as expired, if its deadline
/// passed at `now`.
///
/// Returns whether the deadline passed, also for a request that was closed
/// before.
pub(crate) async fn close_if_expired(
    txn: &mut WriteDbTransaction<'_>,
    chat: &mut Chat,
    now: DateTime<Utc>,
) -> anyhow::Result<bool> {
    let Some(sent_at) = outgoing_sent_at(&mut *txn, chat.id()).await? else {
        return Ok(false);
    };
    if !has_expired(sent_at, now) {
        return Ok(false);
    }
    outgoing_requests::close_expired(txn, chat, expires_at(sent_at).into()).await?;
    Ok(true)
}

/// Closes the incoming and outgoing requests whose deadline passed, and
/// deletes the records of expired requests if the user chose so.
///
/// Also forgets the declines older than a request's lifetime. A copy of such a
/// request expired before it could arrive, so it is dropped as expired.
pub(crate) async fn expire_due(
    txn: &mut WriteDbTransaction<'_>,
    signer: &UserSigningKey,
    now: DateTime<Utc>,
) -> anyhow::Result<()> {
    let cutoff = now - REQUEST_LIFETIME;

    for request_id in PendingConnectionRequest::load_ids_received_until(&mut *txn, cutoff).await? {
        connection_requests::expire_incoming(txn, request_id).await?;
    }

    for chat_id in Chat::load_open_outgoing_request_ids(&mut *txn, cutoff).await? {
        let Some(mut chat) = Chat::load(&mut *txn, &chat_id).await? else {
            continue;
        };
        let Some(sent_at) = outgoing_sent_at(&mut *txn, chat_id).await? else {
            continue;
        };
        let at = expires_at(sent_at).into();
        if outgoing_requests::close_expired(txn, &mut chat, at).await? {
            stage_group_delete(txn, signer, chat_id, now).await?;
        }
    }

    DeclinedConnectionRequest::delete_declined_until(&mut *txn, cutoff).await?;

    if delete_expired_enabled(&mut *txn).await? {
        for chat in deletable_expired_records(&mut *txn).await? {
            persistence::erase(txn, &chat).await?;
        }
    }
    Ok(())
}

/// Whether [`expire_due`] has anything to do at `now`.
pub(crate) async fn is_due(
    mut txn: impl ReadTransaction,
    now: DateTime<Utc>,
) -> anyhow::Result<bool> {
    let cutoff = now - REQUEST_LIFETIME;
    let due = !PendingConnectionRequest::load_ids_received_until(&mut txn, cutoff)
        .await?
        .is_empty()
        || !Chat::load_open_outgoing_request_ids(&mut txn, cutoff)
            .await?
            .is_empty()
        || DeclinedConnectionRequest::exists_declined_until(&mut txn, cutoff).await?;
    if due {
        return Ok(true);
    }
    Ok(delete_expired_enabled(&mut txn).await?
        && !deletable_expired_records(&mut txn).await?.is_empty())
}

/// Parks the deletion of the group of an expired outgoing request, so a late
/// accept finds it gone. The outbound service sends it.
///
/// Nothing is parked while another operation of the group is pending.
async fn stage_group_delete(
    txn: &mut WriteDbTransaction<'_>,
    signer: &UserSigningKey,
    chat_id: ChatId,
    now: DateTime<Utc>,
) -> anyhow::Result<()> {
    if PendingChatOperation::is_pending_for_chat(&mut *txn, chat_id).await? {
        return Ok(());
    }
    // We only log a failure. Returning it would roll back the other expiries
    // of the run, and a failure that repeats would block them for good. The
    // request is closed either way. Only its group stays on the DS.
    let deleted = Some(now.into());
    if let Err(error) = PendingChatOperation::create_delete(txn, signer, chat_id, deleted).await {
        warn!(%error, %chat_id, "Could not park deleting the group of an expired request");
    }
    Ok(())
}

async fn delete_expired_enabled(connection: impl ReadConnection) -> anyhow::Result<bool> {
    let setting = UserSettingRecord::load(connection, DeleteExpiredContactRequestsSetting::KEY)
        .await?
        .map(DeleteExpiredContactRequestsSetting::decode)
        .transpose()
        .inspect_err(|error| warn!(%error, "Invalid setting for expired contact requests"))
        .ok()
        .flatten();
    Ok(setting.is_some_and(|DeleteExpiredContactRequestsSetting(enabled)| enabled))
}

/// The chats of requests that expired. A sent one whose group is still being
/// deleted on the DS is left out until that is done, since erasing the chat
/// would drop the parked delete.
async fn deletable_expired_records(mut txn: impl ReadTransaction) -> anyhow::Result<Vec<Chat>> {
    let mut records = Vec::new();
    for chat_id in Chat::load_closed_request_ids(&mut txn).await? {
        let Some(chat) = Chat::load(&mut txn, &chat_id).await? else {
            continue;
        };
        if is_expired_record(&mut txn, &chat).await?
            && !PendingChatOperation::is_pending_for_chat(&mut txn, chat_id).await?
        {
            records.push(chat);
        }
    }
    Ok(records)
}

pub(crate) async fn is_expired_record(
    connection: impl ReadConnection,
    chat: &Chat,
) -> anyhow::Result<bool> {
    let expected = match chat.chat_type() {
        ChatType::PendingConnection(_) => SystemMessage::ConnectionRequestExpired,
        ChatType::HandleConnection(_) | ChatType::TargetedMessageConnection(_) => {
            SystemMessage::SentConnectionRequestExpired
        }
        ChatType::Group(_) | ChatType::Connection(_) => return Ok(false),
    };
    let last = ChatMessage::last_message(connection, chat.id()).await?;
    Ok(last.is_some_and(|message| {
        matches!(
            message.into_message(),
            Message::Event(EventMessage::System(system)) if system == expected
        )
    }))
}

/// When the open contact request the chat shows expires, if it shows one. For
/// incoming requests, that is the sender's newest request.
async fn open_request_expires_at(
    connection: impl ReadConnection,
    chat: &Chat,
) -> anyhow::Result<Option<DateTime<Utc>>> {
    let ChatStatus::Active = chat.status() else {
        return Ok(None);
    };
    let sent_at = match chat.chat_type() {
        ChatType::PendingConnection(_) => {
            PendingConnectionRequest::load_for_chat(connection, chat.id())
                .await?
                .first()
                .map(|newest| *newest.received_at)
        }
        ChatType::HandleConnection(_) | ChatType::TargetedMessageConnection(_) => {
            outgoing_sent_at(connection, chat.id()).await?
        }
        ChatType::Group(_) | ChatType::Connection(_) => None,
    };
    Ok(sent_at.map(expires_at))
}

impl CoreUser {
    /// When the open contact request the chat shows expires, if it shows one.
    /// For incoming requests, that is the sender's newest request.
    pub async fn contact_request_expires_at(
        &self,
        chat: &Chat,
    ) -> anyhow::Result<Option<DateTime<Utc>>> {
        open_request_expires_at(self.db().read().await?, chat).await
    }
}

#[cfg(feature = "test_utils")]
mod test_utils {
    use sqlx::Row;

    use super::*;

    impl CoreUser {
        /// Moves the times the contact request of the chat counts from back
        /// by `by`: the times of its messages, and when its pending requests
        /// were received.
        pub async fn backdate_contact_request(
            &self,
            chat_id: ChatId,
            by: Duration,
        ) -> anyhow::Result<()> {
            self.db()
                .with_write_transaction(async |txn| -> anyhow::Result<()> {
                    let messages =
                        sqlx::query("SELECT message_id, timestamp FROM message WHERE chat_id = ?")
                            .bind(chat_id)
                            .fetch_all(txn.as_mut())
                            .await?;
                    for message in messages {
                        let message_id: Vec<u8> = message.try_get("message_id")?;
                        let timestamp: DateTime<Utc> = message.try_get("timestamp")?;
                        sqlx::query("UPDATE message SET timestamp = ? WHERE message_id = ?")
                            .bind(timestamp - by)
                            .bind(message_id)
                            .execute(txn.as_mut())
                            .await?;
                    }
                    let requests = sqlx::query(
                        "SELECT request_id, received_at FROM pending_connection_request
                        WHERE chat_id = ?",
                    )
                    .bind(chat_id)
                    .fetch_all(txn.as_mut())
                    .await?;
                    for request in requests {
                        let request_id: Vec<u8> = request.try_get("request_id")?;
                        let received_at: DateTime<Utc> = request.try_get("received_at")?;
                        sqlx::query(
                            "UPDATE pending_connection_request SET received_at = ?
                            WHERE request_id = ?",
                        )
                        .bind(received_at - by)
                        .bind(request_id)
                        .execute(txn.as_mut())
                        .await?;
                    }
                    Ok(())
                })
                .await
        }
    }
}

#[cfg(test)]
mod tests {
    use aircommon::{
        credentials::test_utils::create_test_credentials,
        crypto::{
            aead::keys::{
                FriendshipPackageEarKey, GroupStateEarKey, IdentityLinkWrapperKey,
                WelcomeAttributionInfoEarKey,
            },
            indexed_aead::keys::UserProfileBaseSecret,
        },
        identifiers::{QualifiedGroupId, UserId, Username},
        messages::{
            FriendshipToken, client_as::ConnectionOfferHash,
            connection_package::ConnectionPackageHash,
        },
    };
    use anyhow::Context as _;
    use openmls::group::GroupId;
    use uuid::Uuid;

    use crate::{
        ChatAttributes,
        chats::connection_requests::{IncomingRequest, IncomingRequestSource, StoredRequest},
        clients::{
            connection_offer::{FriendshipPackage, payload::ConnectionInfo},
            own_client_info::OwnClientInfo,
        },
        contacts::{TargetedMessageContact, UsernameContact},
        db::access::{DbAccess, ReadDbTransaction},
        utils::persistence::open_db_in_memory,
    };

    use super::*;

    fn group_id() -> GroupId {
        QualifiedGroupId::new(Uuid::new_v4(), "example.com".parse().unwrap()).into()
    }

    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp(1_767_225_600 + seconds, 0).unwrap()
    }

    fn signer() -> UserSigningKey {
        create_test_credentials(UserId::random("example.com".parse().unwrap())).1
    }

    async fn db() -> anyhow::Result<DbAccess> {
        let db = DbAccess::for_tests(open_db_in_memory().await?);
        db.with_write_transaction(async |txn| OwnClientInfo::store_for_test(txn, None).await)
            .await?;
        Ok(db)
    }

    async fn receive(
        txn: &mut WriteDbTransaction<'_>,
        sender: &UserId,
        username: &str,
        received_at: DateTime<Utc>,
    ) -> anyhow::Result<StoredRequest> {
        IncomingRequest {
            connection_info: ConnectionInfo {
                connection_group_id: group_id(),
                connection_group_ear_key: GroupStateEarKey::random()?,
                connection_group_identity_link_wrapper_key: IdentityLinkWrapperKey::random()?,
                friendship_package_ear_key: FriendshipPackageEarKey::random()?,
                friendship_package: FriendshipPackage {
                    friendship_token: FriendshipToken::random()?,
                    wai_ear_key: WelcomeAttributionInfoEarKey::random()?,
                    user_profile_base_secret: UserProfileBaseSecret::random()?,
                },
            },
            sender: sender.clone(),
            source: IncomingRequestSource::Username {
                username: Username::new(username.to_owned())?,
                connection_offer_hash: ConnectionOfferHash::new_for_test(vec![1; 32]),
                connection_package_hash: ConnectionPackageHash::new_for_test(vec![2; 32]),
            },
            received_at: received_at.into(),
        }
        .store(txn)
        .await
    }

    /// Stores an open outgoing request to `username`, announced at `sent_at`.
    async fn send(
        txn: &mut WriteDbTransaction<'_>,
        username: &Username,
        sent_at: DateTime<Utc>,
    ) -> anyhow::Result<Chat> {
        let chat = Chat::new_handle_chat(group_id(), username.clone());
        chat.store(&mut *txn).await?;
        ChatMessage::new_system_message(
            chat.id(),
            sent_at.into(),
            SystemMessage::NewHandleConnectionChat(username.clone()),
        )
        .store(&mut *txn)
        .await?;
        UsernameContact::new(
            username.clone(),
            chat.id(),
            FriendshipPackageEarKey::random()?,
            ConnectionOfferHash::new_for_test(vec![3; 32]),
        )
        .upsert(&mut *txn)
        .await?;
        Ok(chat)
    }

    /// Stores an open outgoing request to `user_id` through a group chat,
    /// announced at `sent_at`.
    async fn send_targeted(
        txn: &mut WriteDbTransaction<'_>,
        user_id: &UserId,
        sent_at: DateTime<Utc>,
    ) -> anyhow::Result<Chat> {
        let chat = Chat::new_targeted_message_chat(group_id(), user_id.clone());
        chat.store(&mut *txn).await?;
        ChatMessage::new_system_message(
            chat.id(),
            sent_at.into(),
            SystemMessage::NewDirectConnectionChat(user_id.clone()),
        )
        .store(&mut *txn)
        .await?;
        TargetedMessageContact::new(
            user_id.clone(),
            chat.id(),
            FriendshipPackageEarKey::random()?,
            None,
        )
        .upsert(&mut *txn)
        .await?;
        Ok(chat)
    }

    async fn last_system_message(
        txn: &mut WriteDbTransaction<'_>,
        chat_id: ChatId,
    ) -> anyhow::Result<Option<(SystemMessage, DateTime<Utc>)>> {
        Ok(ChatMessage::last_message(&mut *txn, chat_id)
            .await?
            .and_then(|message| {
                let timestamp = message.timestamp();
                match message.into_message() {
                    Message::Event(EventMessage::System(system)) => Some((system, timestamp)),
                    _ => None,
                }
            }))
    }

    async fn expiry(
        txn: &mut ReadDbTransaction<'_>,
        chat_id: ChatId,
    ) -> anyhow::Result<Option<DateTime<Utc>>> {
        let chat = Chat::load(&mut *txn, &chat_id)
            .await?
            .with_context(|| format!("no chat {chat_id}"))?;
        open_request_expires_at(txn, &chat).await
    }

    async fn status(
        txn: &mut WriteDbTransaction<'_>,
        chat_id: ChatId,
    ) -> anyhow::Result<ChatStatus> {
        let chat = Chat::load(&mut *txn, &chat_id)
            .await?
            .with_context(|| format!("no chat {chat_id}"))?;
        Ok(chat.status().clone())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_incoming_request_expires_after_its_lifetime() -> anyhow::Result<()> {
        let db = db().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let sender = UserId::random("example.com".parse()?);
            let stored = receive(txn, &sender, "ellie-03", at(0)).await?;

            let deadline = at(0) + REQUEST_LIFETIME;
            expire_due(txn, &signer(), deadline - Duration::seconds(1)).await?;
            assert_eq!(status(txn, stored.chat_id).await?, ChatStatus::Active);

            expire_due(txn, &signer(), deadline).await?;
            assert!(matches!(
                status(txn, stored.chat_id).await?,
                ChatStatus::Inactive(_)
            ));
            assert_eq!(
                last_system_message(txn, stored.chat_id).await?,
                Some((SystemMessage::ConnectionRequestExpired, deadline))
            );
            assert!(
                PendingConnectionRequest::load_for_chat(&mut *txn, stored.chat_id)
                    .await?
                    .is_empty()
            );
            Ok(())
        })
        .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_older_request_drops_out_silently() -> anyhow::Result<()> {
        let db = db().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let sender = UserId::random("example.com".parse()?);
            receive(txn, &sender, "ellie-03", at(0)).await?;
            let newest = receive(txn, &sender, "ellie-04", at(3600)).await?;

            expire_due(txn, &signer(), at(0) + REQUEST_LIFETIME).await?;

            assert_eq!(status(txn, newest.chat_id).await?, ChatStatus::Active);
            let requests =
                PendingConnectionRequest::load_for_chat(&mut *txn, newest.chat_id).await?;
            let request_ids: Vec<_> = requests.iter().map(|request| request.request_id).collect();
            assert_eq!(request_ids, vec![newest.chat_id]);
            assert!(matches!(
                last_system_message(txn, newest.chat_id).await?,
                Some((
                    SystemMessage::ReceivedAdditionalUsernameConnectionRequest { .. },
                    _
                ))
            ));
            Ok(())
        })
        .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_card_counts_down_to_the_newest_request() -> anyhow::Result<()> {
        let db = db().await?;
        let (received, sent, expired, group) = db
            .with_write_transaction(async |txn| -> anyhow::Result<_> {
                let sender = UserId::random("example.com".parse()?);
                receive(txn, &sender, "ellie-03", at(0)).await?;
                let received = receive(txn, &sender, "ellie-04", at(3600)).await?;
                let sent = send(txn, &Username::new("joel-03".to_owned())?, at(60)).await?;
                let expired = send(txn, &Username::new("joel-04".to_owned())?, at(0)).await?;
                let group = Chat::new_group_chat(
                    group_id(),
                    ChatAttributes::new("Design Team".to_owned(), None),
                );
                group.store(&mut *txn).await?;
                ChatMessage::new_system_message(
                    group.id(),
                    at(0).into(),
                    SystemMessage::Add(None, sender.clone()),
                )
                .store(&mut *txn)
                .await?;
                expire_due(txn, &signer(), at(30) + REQUEST_LIFETIME).await?;
                Ok((received.chat_id, sent.id(), expired.id(), group.id()))
            })
            .await?;
        db.with_read_transaction(async |txn| -> anyhow::Result<()> {
            assert_eq!(
                expiry(txn, received).await?,
                Some(at(3600) + REQUEST_LIFETIME)
            );
            assert_eq!(expiry(txn, sent).await?, Some(at(60) + REQUEST_LIFETIME));
            assert_eq!(expiry(txn, expired).await?, None);
            assert_eq!(expiry(txn, group).await?, None);
            Ok(())
        })
        .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_outgoing_request_expires_and_frees_the_username() -> anyhow::Result<()> {
        let db = db().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let username = Username::new("joel-03".to_owned())?;
            let chat = send(txn, &username, at(0)).await?;

            let deadline = at(0) + REQUEST_LIFETIME;
            expire_due(txn, &signer(), deadline - Duration::seconds(1)).await?;
            assert_eq!(status(txn, chat.id()).await?, ChatStatus::Active);
            assert!(UsernameContact::load(&mut *txn, &username).await?.is_some());

            expire_due(txn, &signer(), deadline).await?;
            assert!(matches!(
                status(txn, chat.id()).await?,
                ChatStatus::Inactive(_)
            ));
            assert_eq!(
                last_system_message(txn, chat.id()).await?,
                Some((SystemMessage::SentConnectionRequestExpired, deadline))
            );
            assert!(UsernameContact::load(&mut *txn, &username).await?.is_none());
            Ok(())
        })
        .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_outgoing_request_through_a_group_expires_and_frees_the_user() -> anyhow::Result<()>
    {
        let db = db().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let user_id = UserId::random("example.com".parse()?);
            let chat = send_targeted(txn, &user_id, at(0)).await?;

            expire_due(txn, &signer(), at(0) + REQUEST_LIFETIME).await?;

            assert!(matches!(
                status(txn, chat.id()).await?,
                ChatStatus::Inactive(_)
            ));
            assert!(matches!(
                last_system_message(txn, chat.id()).await?,
                Some((SystemMessage::SentConnectionRequestExpired, _))
            ));
            assert!(
                TargetedMessageContact::load(&mut *txn, &user_id)
                    .await?
                    .is_none()
            );
            Ok(())
        })
        .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn crossed_requests_through_a_group_both_expire() -> anyhow::Result<()> {
        let db = db().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let user_id = UserId::random("example.com".parse()?);
            // The sibling's request takes over the targeted contact of the user.
            let own = send_targeted(txn, &user_id, at(0)).await?;
            let sibling = send_targeted(txn, &user_id, at(1)).await?;

            expire_due(txn, &signer(), at(1) + REQUEST_LIFETIME).await?;

            for chat_id in [own.id(), sibling.id()] {
                assert!(matches!(
                    status(txn, chat_id).await?,
                    ChatStatus::Inactive(_)
                ));
                assert!(matches!(
                    last_system_message(txn, chat_id).await?,
                    Some((SystemMessage::SentConnectionRequestExpired, _))
                ));
            }
            assert!(
                TargetedMessageContact::load(&mut *txn, &user_id)
                    .await?
                    .is_none()
            );
            Ok(())
        })
        .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn expired_records_go_when_the_user_chose_so() -> anyhow::Result<()> {
        let db = db().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let sender = UserId::random("example.com".parse()?);
            let received = receive(txn, &sender, "ellie-03", at(0)).await?;
            let sent = send(txn, &Username::new("joel-03".to_owned())?, at(0)).await?;
            let open = send(txn, &Username::new("joel-04".to_owned())?, at(60)).await?;
            let now = at(30) + REQUEST_LIFETIME;
            expire_due(txn, &signer(), now).await?;
            assert!(Chat::load(&mut *txn, &received.chat_id).await?.is_some());
            assert!(Chat::load(&mut *txn, &sent.id()).await?.is_some());

            let setting = DeleteExpiredContactRequestsSetting(true);
            UserSettingRecord::store(
                &mut *txn,
                DeleteExpiredContactRequestsSetting::KEY,
                setting.encode()?,
            )
            .await?;
            expire_due(txn, &signer(), now).await?;
            assert!(Chat::load(&mut *txn, &received.chat_id).await?.is_none());
            assert!(Chat::load(&mut *txn, &sent.id()).await?.is_none());
            assert_eq!(status(txn, open.id()).await?, ChatStatus::Active);
            Ok(())
        })
        .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_sibling_deletion_is_an_expiry_only_past_the_deadline() -> anyhow::Result<()> {
        let db = db().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let mut open = send(txn, &Username::new("joel-03".to_owned())?, at(60)).await?;
            let mut expired = send(txn, &Username::new("joel-04".to_owned())?, at(0)).await?;

            let deadline = at(0) + REQUEST_LIFETIME;
            assert!(!close_if_expired(txn, &mut open, deadline).await?);
            assert!(close_if_expired(txn, &mut expired, deadline).await?);
            assert!(matches!(expired.status(), ChatStatus::Inactive(_)));

            assert_eq!(status(txn, open.id()).await?, ChatStatus::Active);
            assert!(matches!(
                last_system_message(txn, open.id()).await?,
                Some((SystemMessage::NewHandleConnectionChat(_), _))
            ));
            assert!(matches!(
                status(txn, expired.id()).await?,
                ChatStatus::Inactive(_)
            ));
            assert_eq!(
                last_system_message(txn, expired.id()).await?,
                Some((SystemMessage::SentConnectionRequestExpired, deadline))
            );
            Ok(())
        })
        .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_sibling_deletion_records_the_expiry_of_a_crossed_request_once() -> anyhow::Result<()>
    {
        let db = db().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let user_id = UserId::random("example.com".parse()?);
            let own = send_targeted(txn, &user_id, at(0)).await?;
            send_targeted(txn, &user_id, at(0)).await?;
            // The removal commit marks the chat inactive before it is closed.
            Chat::update_status(&mut *txn, own.id(), &ChatStatus::inactive(Vec::new())).await?;
            let mut own = Chat::load(&mut *txn, &own.id()).await?.context("no chat")?;

            let deadline = at(0) + REQUEST_LIFETIME;
            assert!(close_if_expired(txn, &mut own, deadline).await?);
            assert!(close_if_expired(txn, &mut own, deadline).await?);

            assert!(matches!(
                last_system_message(txn, own.id()).await?,
                Some((SystemMessage::SentConnectionRequestExpired, _))
            ));
            let messages: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM message WHERE chat_id = ?")
                    .bind(own.id())
                    .fetch_one(txn.as_mut())
                    .await?;
            assert_eq!(messages, 2);
            Ok(())
        })
        .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn work_is_due_from_a_deadline_until_it_is_done() -> anyhow::Result<()> {
        let db = db().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let deadline = at(0) + REQUEST_LIFETIME;
            assert!(!is_due(&mut *txn, deadline).await?);

            let sender = UserId::random("example.com".parse()?);
            receive(txn, &sender, "ellie-03", at(0)).await?;
            assert!(!is_due(&mut *txn, deadline - Duration::seconds(1)).await?);
            assert!(is_due(&mut *txn, deadline).await?);
            expire_due(txn, &signer(), deadline).await?;
            assert!(!is_due(&mut *txn, deadline).await?);

            send(txn, &Username::new("joel-03".to_owned())?, at(0)).await?;
            assert!(is_due(&mut *txn, deadline).await?);
            expire_due(txn, &signer(), deadline).await?;
            assert!(!is_due(&mut *txn, deadline).await?);

            DeclinedConnectionRequest {
                request_id: ChatId::new(Uuid::new_v4()),
                declined_at: at(0),
            }
            .store(&mut *txn)
            .await?;
            assert!(is_due(&mut *txn, deadline).await?);
            expire_due(txn, &signer(), deadline).await?;
            assert!(!is_due(&mut *txn, deadline).await?);

            UserSettingRecord::store(
                &mut *txn,
                DeleteExpiredContactRequestsSetting::KEY,
                DeleteExpiredContactRequestsSetting(true).encode()?,
            )
            .await?;
            assert!(is_due(&mut *txn, deadline).await?);
            expire_due(txn, &signer(), deadline).await?;
            assert!(!is_due(&mut *txn, deadline).await?);
            Ok(())
        })
        .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn declines_older_than_a_request_lifetime_are_forgotten() -> anyhow::Result<()> {
        let db = db().await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let old = ChatId::new(Uuid::new_v4());
            let recent = ChatId::new(Uuid::new_v4());
            for (request_id, declined_at) in [(old, at(0)), (recent, at(60))] {
                sqlx::query(
                    "INSERT INTO declined_connection_request (request_id, declined_at)
                    VALUES (?, ?)",
                )
                .bind(request_id)
                .bind(declined_at)
                .execute(txn.as_mut())
                .await?;
            }

            expire_due(txn, &signer(), at(30) + REQUEST_LIFETIME).await?;

            assert!(!connection_requests::is_known(&mut *txn, old).await?);
            assert!(connection_requests::is_known(&mut *txn, recent).await?);
            Ok(())
        })
        .await
    }

    #[test]
    fn a_request_past_its_deadline_counts_as_expired() {
        assert!(has_expired(at(0), at(0) + REQUEST_LIFETIME));
        assert!(!has_expired(
            at(0),
            at(0) + REQUEST_LIFETIME - Duration::seconds(1)
        ));
    }
}
