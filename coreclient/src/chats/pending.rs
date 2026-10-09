// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::keys::LeafSigningKey,
    crypto::{aead::AeadEncryptable, indexed_aead::keys::UserProfileKey},
    identifiers::{QualifiedGroupId, Username},
    messages::{
        client_as::ConnectionOfferHash,
        client_ds::{AadMessage, AadPayload, JoinConnectionGroupParamsAad},
        connection_package::ConnectionPackageHash,
    },
    time::TimeStamp,
};
use airprotos::client::group_bootstrap::{AcceptContext, ConnectionContext, GroupBootstrapCarrier};
use anyhow::{Context, bail, ensure};
use apqmls::commit_builder::ApqCommitMessageBundle;
use openmls::{group::GroupId, prelude::MlsMessageOut, treesync::errors::LeafNodeValidationError};
use tls_codec::DeserializeBytes;
use tracing::{instrument, warn};

use crate::{
    Chat, ChatId, ChatType, Contact, SystemMessage,
    chats::{
        connection_requests::{self, delete_consumed_package_key},
        messages::TimestampedMessage,
    },
    clients::{
        CoreUser,
        connection_offer::{FriendshipPackage, payload::ConnectionInfo},
    },
    db::access::WriteConnection,
    groups::{ConnectionGroupJoin, Group, self_group::SelfGroup},
    key_stores::indexed_keys::StorableIndexedKey,
    utils::persistence::GroupIdWrapper,
};

/// A pending incoming connection request.
///
/// All pending requests of one sender share the chat of the newest one.
pub(crate) struct PendingConnectionRequest {
    /// The chat id of the request .
    pub(crate) request_id: ChatId,
    /// The chat that shows the request.
    pub(crate) chat_id: ChatId,
    pub(crate) created_at: TimeStamp,
    /// When the request was sent, server timestamp.
    pub(crate) received_at: TimeStamp,
    pub(crate) connection_info: ConnectionInfo,
    /// The username a request via a username went to.
    pub(crate) username: Option<Username>,
    pub(crate) connection_offer_hash: Option<ConnectionOfferHash>,
    pub(crate) connection_package_hash: Option<ConnectionPackageHash>,
    /// The group a request via a group went through. Its group chat may be
    /// gone.
    pub(crate) origin_group_id: Option<GroupIdWrapper>,
}

impl PendingConnectionRequest {
    pub(crate) fn group_id(&self) -> &GroupId {
        &self.connection_info.connection_group_id
    }
}

/// How accepting the newest request of a chat went.
enum AcceptOutcome {
    Accepted,
    /// The request was gone. `next` is the chat of the sender's remaining
    /// requests, if there are any.
    Unavailable {
        next: Option<ChatId>,
    },
    Failed(AcceptContactRequestError),
}

impl CoreUser {
    /// Accepts the incoming contact request the chat shows.
    ///
    /// The newest of the sender's pending requests is accepted. If its sender
    /// retracted it, the next newest is. Returns the chat of the connection,
    /// which is not `chat_id` if an older request was accepted.
    #[instrument(skip(self), err)]
    pub async fn accept_contact_request(
        &self,
        chat_id: ChatId,
    ) -> anyhow::Result<Result<ChatId, AcceptContactRequestError>> {
        let mut chat_id = chat_id;
        // Every round that does not end the loop removes one request.
        loop {
            match self.accept_newest_request(chat_id).await? {
                AcceptOutcome::Accepted => return Ok(Ok(chat_id)),
                AcceptOutcome::Failed(error) => return Ok(Err(error)),
                AcceptOutcome::Unavailable { next: Some(next) } => chat_id = next,
                AcceptOutcome::Unavailable { next: None } => {
                    return Ok(Err(AcceptContactRequestError::Unavailable));
                }
            }
        }
    }

    async fn accept_newest_request(&self, chat_id: ChatId) -> anyhow::Result<AcceptOutcome> {
        // Load needed data
        let (chat, sender_user_id, request, own_user_profile_key) = self
            .db()
            .with_read_transaction(async |txn| {
                let chat: Chat = Chat::load(&mut *txn, &chat_id)
                    .await?
                    .with_context(|| format!("Can't find chat with id {chat_id}"))?;
                let ChatType::PendingConnection(sender_user_id) = chat.chat_type() else {
                    bail!("Chat is not a pending connection");
                };
                let sender_user_id = sender_user_id.clone();
                // The chat is the one of the newest request.
                let request = PendingConnectionRequest::load(&mut *txn, chat_id)
                    .await?
                    .with_context(|| format!("No pending request found for chat: {chat_id}"))?;
                let own_user_profile_key = UserProfileKey::load_own(&mut *txn).await?;
                Ok((chat, sender_user_id, request, own_user_profile_key))
            })
            .await?;

        let PendingConnectionRequest {
            request_id,
            chat_id: _,
            created_at: _,
            received_at: _,
            connection_info,
            username,
            connection_offer_hash,
            connection_package_hash,
            origin_group_id: _,
        } = request;

        // Prepare group
        let (aad, qgid) = self.prepare_group(&connection_info, &own_user_profile_key)?;

        // Fetch external commit info
        let eci = match self
            .api_clients()
            .get(qgid.owning_domain())?
            .ds_connection_group_info(
                connection_info.connection_group_id.clone(),
                &connection_info.connection_group_ear_key,
            )
            .await
        {
            Ok(eci) => eci,
            // The sender retracted the request.
            Err(error) if error.is_not_found() => {
                let next = self.record_unavailable_request(request_id).await?;
                return Ok(AcceptOutcome::Unavailable { next });
            }
            Err(error) => return Err(error.into()),
        };
        let is_apq = eci.is_apq()?;

        // Create a new group by joining it (if group already exists, it will be replaced)
        let result = Box::pin(self.db().with_write_transaction(
            async |txn| -> anyhow::Result<Result<_, _>> {
                if Group::load_with_chat_id(&mut *txn, chat_id)
                    .await?
                    .is_some()
                {
                    warn!(%chat_id, "Group for pending chat already exists");
                    Group::delete_from_db(txn, chat.group_id()).await?;
                    if let Some(hash) = connection_offer_hash {
                        Group::delete_connection_offer_psk(&mut *txn, hash)?;
                    }
                }

                let self_group = SelfGroup::load(&mut *txn)
                    .await?
                    .filter(SelfGroup::has_linked_devices);
                let vc_group_id = self_group.as_ref().map(|group| group.group_id().clone());

                let connection_group = Some(ConnectionGroupJoin {
                    inviter: &sender_user_id,
                    connection_offer_hash,
                });
                // Join group: APQ or T decided by apq info in the member-signed group info.
                let (mut group, commit, mut member_profile_info) = if is_apq {
                    let res = Box::pin(Group::join_apq_group_externally(
                        txn,
                        self.api_clients(),
                        eci,
                        &LeafSigningKey::User(self.signing_key().clone()),
                        self.user_id(),
                        connection_info.connection_group_ear_key.clone(),
                        connection_info
                            .connection_group_identity_link_wrapper_key
                            .clone(),
                        aad,
                        vc_group_id,
                        connection_group,
                    ))
                    .await?;
                    match res {
                        Ok((group, bundle, infos)) => {
                            (group, ConnectionJoinCommit::Apq(Box::new(bundle)), infos)
                        }
                        Err(error) => return Ok(Err(error)),
                    }
                } else {
                    let res = Group::join_group_externally(
                        txn,
                        self.api_clients(),
                        eci,
                        self.signing_key(),
                        connection_info.connection_group_ear_key.clone(),
                        connection_info
                            .connection_group_identity_link_wrapper_key
                            .clone(),
                        aad,
                        connection_group,
                        vc_group_id,
                    )
                    .await?;
                    match res {
                        Ok((group, commit, group_info, infos)) => {
                            let bundle = TConnectionJoinCommit { commit, group_info };
                            (group, ConnectionJoinCommit::T(Box::new(bundle)), infos)
                        }
                        Err(error) => return Ok(Err(error)),
                    }
                };

                // Verify that the group has only one other member and that it's
                // the sender of the CEP.
                let members: Vec<_> = group.members().collect();

                ensure!(
                    members.len() == 2,
                    "Connection group has more than two members: {:?}",
                    members
                );

                ensure!(
                    members.contains(self.user_id()) && members.contains(&sender_user_id),
                    "Connection group has unexpected members: {:?}",
                    members
                );

                // There should be only one user profile
                let contact_profile_info = member_profile_info
                    .members
                    .pop()
                    .context("No user profile returned when joining connection group")?;

                debug_assert!(
                    member_profile_info.members.is_empty(),
                    "More than one user profile returned when joining connection group"
                );

                // Fetch and store user profile
                Self::schedule_fetch_user_profile(&mut *txn, contact_profile_info).await?;

                let now = TimeStamp::now();
                group.store_update(&mut *txn, Some(now), Some(now)).await?;

                if let Some(hash) = connection_package_hash {
                    delete_consumed_package_key(txn, &hash)
                        .await
                        .context("Failed to delete connection package")?;
                }

                let group_bootstrap = match &self_group {
                    Some(self_group) => {
                        let friendship_package = &connection_info.friendship_package;
                        let connection = ConnectionContext::Accept(AcceptContext {
                            user_id: Some(sender_user_id.clone().into()),
                            friendship_token: Some(friendship_package.friendship_token.clone()),
                            wai_ear_key: Some(friendship_package.wai_ear_key.clone()),
                            user_profile_base_secret: Some(
                                friendship_package.user_profile_base_secret.clone(),
                            ),
                            connection_offer_hash,
                        });
                        Some(self_group.seal_group_bootstrap_param(
                            txn,
                            &group,
                            GroupBootstrapCarrier::JoinEcho,
                            Some(connection),
                        )?)
                    }
                    None => None,
                };

                Ok(Ok((commit, group_bootstrap)))
            },
        ))
        .await?;

        // Propagate the error to the caller if it is a leaf node validation error.
        let (commit, group_bootstrap) = match result {
            Ok(value) => value,
            Err(error) => return Ok(AcceptOutcome::Failed(error.into())),
        };

        // Send confirmation to DS
        let qs_client_reference = self.create_own_client_reference();
        let api_client = self.api_clients().get(qgid.owning_domain())?;
        let joined = match commit {
            ConnectionJoinCommit::T(bundle) => api_client
                .ds_join_connection_group(
                    bundle.commit,
                    bundle.group_info,
                    qs_client_reference,
                    &connection_info.connection_group_ear_key,
                    group_bootstrap,
                )
                .await
                .map(|_| ()),
            ConnectionJoinCommit::Apq(bundle) => api_client
                .ds_apq_join_connection_group(
                    *bundle,
                    qs_client_reference,
                    &connection_info.connection_group_ear_key,
                    group_bootstrap,
                )
                .await
                .map(|_| ()),
        };
        match joined {
            Ok(()) => {}
            // The sender retracted the request after we fetched the group.
            Err(error) if error.is_not_found() => {
                let next = self.record_unavailable_request(request_id).await?;
                return Ok(AcceptOutcome::Unavailable { next });
            }
            Err(error) => return Err(error.into()),
        }

        // The chat becomes the connection with the sender, and the sender's
        // requests are settled.
        self.db()
            .with_write_transaction(async |txn| -> anyhow::Result<_> {
                chat.set_chat_type(&mut *txn, &ChatType::Connection(sender_user_id.clone()))
                    .await?;

                let accepted_message = TimestampedMessage::system_message(
                    SystemMessage::AcceptedConnectionRequest {
                        contact: sender_user_id.clone(),
                        user_handle: username,
                    },
                    TimeStamp::now(),
                );
                Self::store_new_messages(&mut *txn, chat_id, vec![accepted_message]).await?;

                let friendship_package = connection_info.friendship_package;
                Contact {
                    user_id: sender_user_id,
                    wai_ear_key: friendship_package.wai_ear_key,
                    friendship_token: friendship_package.friendship_token,
                    chat_id,
                    supported_features: None,
                }
                .upsert(&mut *txn)
                .await?;
                if let Some(hash) = connection_offer_hash {
                    Group::delete_connection_offer_psk(&mut *txn, hash)?;
                }
                connection_requests::settle_accepted(txn, chat_id).await
            })
            .await?;

        Ok(AcceptOutcome::Accepted)
    }

    fn prepare_group(
        &self,
        connection_info: &ConnectionInfo,
        own_user_profile_key: &UserProfileKey,
    ) -> anyhow::Result<(AadMessage, QualifiedGroupId)> {
        // We create a new group and signal that fact to the user,
        // so the user can decide if they want to accept the
        // connection.

        let encrypted_user_profile_key = own_user_profile_key.encrypt(
            &connection_info.connection_group_identity_link_wrapper_key,
            self.user_id(),
        )?;

        let encrypted_friendship_package = FriendshipPackage {
            friendship_token: self.key_store().friendship_token.clone(),
            wai_ear_key: self.key_store().wai_ear_key.clone(),
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
    use aircommon::identifiers::UserId;
    use sqlx::{query, query_as, query_scalar};

    use crate::db::access::ReadConnection;

    use super::*;

    impl PendingConnectionRequest {
        pub(crate) async fn load(
            mut connection: impl ReadConnection,
            request_id: ChatId,
        ) -> sqlx::Result<Option<Self>> {
            query_as!(
                PendingConnectionRequest,
                r#"SELECT
                    request_id AS "request_id: ChatId",
                    chat_id AS "chat_id: ChatId",
                    created_at AS "created_at: TimeStamp",
                    received_at AS "received_at: TimeStamp",
                    connection_info AS "connection_info: ConnectionInfo",
                    username AS "username: _",
                    connection_offer_hash AS "connection_offer_hash: _",
                    connection_package_hash AS "connection_package_hash: _",
                    origin_group_id AS "origin_group_id: GroupIdWrapper"
                FROM pending_connection_request
                WHERE request_id = ?"#,
                request_id,
            )
            .fetch_optional(connection.as_mut())
            .await
        }

        /// The pending requests the chat shows, newest first.
        pub(crate) async fn load_for_chat(
            mut connection: impl ReadConnection,
            chat_id: ChatId,
        ) -> sqlx::Result<Vec<Self>> {
            query_as!(
                PendingConnectionRequest,
                r#"SELECT
                    request_id AS "request_id: ChatId",
                    chat_id AS "chat_id: ChatId",
                    created_at AS "created_at: TimeStamp",
                    received_at AS "received_at: TimeStamp",
                    connection_info AS "connection_info: ConnectionInfo",
                    username AS "username: _",
                    connection_offer_hash AS "connection_offer_hash: _",
                    connection_package_hash AS "connection_package_hash: _",
                    origin_group_id AS "origin_group_id: GroupIdWrapper"
                FROM pending_connection_request
                WHERE chat_id = ?
                ORDER BY received_at DESC, request_id DESC"#,
                chat_id,
            )
            .fetch_all(connection.as_mut())
            .await
        }

        /// Every pending request, sorted by request id.
        pub(crate) async fn load_all(
            mut connection: impl ReadConnection,
        ) -> sqlx::Result<Vec<Self>> {
            query_as!(
                PendingConnectionRequest,
                r#"SELECT
                    request_id AS "request_id: ChatId",
                    chat_id AS "chat_id: ChatId",
                    created_at AS "created_at: TimeStamp",
                    received_at AS "received_at: TimeStamp",
                    connection_info AS "connection_info: ConnectionInfo",
                    username AS "username: _",
                    connection_offer_hash AS "connection_offer_hash: _",
                    connection_package_hash AS "connection_package_hash: _",
                    origin_group_id AS "origin_group_id: GroupIdWrapper"
                FROM pending_connection_request
                ORDER BY request_id"#,
            )
            .fetch_all(connection.as_mut())
            .await
        }

        /// The chat that shows the pending requests of `sender`, if any.
        pub(crate) async fn chat_of_sender(
            mut connection: impl ReadConnection,
            sender: &UserId,
        ) -> sqlx::Result<Option<ChatId>> {
            let uuid = sender.uuid();
            let domain = sender.domain();
            query_scalar!(
                r#"SELECT r.chat_id AS "chat_id: ChatId"
                FROM pending_connection_request r
                INNER JOIN chat c ON c.chat_id = r.chat_id
                WHERE c.is_incoming = 1
                    AND c.is_confirmed_connection = 0
                    AND c.connection_user_uuid = ?
                    AND c.connection_user_domain = ?
                LIMIT 1"#,
                uuid,
                domain,
            )
            .fetch_optional(connection.as_mut())
            .await
        }

        pub(crate) async fn store(&self, mut connection: impl WriteConnection) -> sqlx::Result<()> {
            let origin_group_id = self
                .origin_group_id
                .as_ref()
                .map(|GroupIdWrapper(group_id)| group_id.as_slice());
            query!(
                "INSERT INTO pending_connection_request (
                    request_id,
                    chat_id,
                    created_at,
                    received_at,
                    connection_info,
                    username,
                    connection_offer_hash,
                    connection_package_hash,
                    origin_group_id
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT (request_id) DO UPDATE SET
                    chat_id = excluded.chat_id,
                    created_at = excluded.created_at,
                    received_at = excluded.received_at,
                    connection_info = excluded.connection_info,
                    username = excluded.username,
                    connection_offer_hash = excluded.connection_offer_hash,
                    connection_package_hash = excluded.connection_package_hash,
                    origin_group_id = excluded.origin_group_id",
                self.request_id,
                self.chat_id,
                self.created_at,
                self.received_at,
                self.connection_info,
                self.username,
                self.connection_offer_hash,
                self.connection_package_hash,
                origin_group_id,
            )
            .execute(connection.as_mut())
            .await?;
            connection.notifier().update(self.chat_id);
            Ok(())
        }

        pub(crate) async fn delete(
            mut connection: impl WriteConnection,
            request_id: ChatId,
        ) -> sqlx::Result<()> {
            query!(
                "DELETE FROM pending_connection_request WHERE request_id = ?",
                request_id
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }

        /// Moves the pending requests of one chat to another.
        pub(crate) async fn move_to_chat(
            mut connection: impl WriteConnection,
            from: ChatId,
            to: ChatId,
        ) -> sqlx::Result<()> {
            query!(
                "UPDATE pending_connection_request SET chat_id = ?1 WHERE chat_id = ?2",
                to,
                from
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }
    }
}

/// Errors that can occur when accepting a contact request.
#[derive(Debug, thiserror::Error)]
pub enum AcceptContactRequestError {
    #[error("Incompatible client: {reason}")]
    IncompatibleClient { reason: String },
    /// The sender retracted every pending request the chat showed. The chat
    /// is now a record of that.
    #[error("The contact request is no longer available")]
    Unavailable,
}

impl From<LeafNodeValidationError> for AcceptContactRequestError {
    fn from(error: LeafNodeValidationError) -> Self {
        Self::IncompatibleClient {
            reason: error.to_string(),
        }
    }
}

/// The external commit to hand to the DS, per group kind.
enum ConnectionJoinCommit {
    T(Box<TConnectionJoinCommit>),
    Apq(Box<ApqCommitMessageBundle>),
}

struct TConnectionJoinCommit {
    commit: MlsMessageOut,
    group_info: MlsMessageOut,
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
        identifiers::UserId,
        messages::FriendshipToken,
    };
    use chrono::{DateTime, Utc};
    use sqlx::{SqlitePool, migrate::Migrator, sqlite::SqlitePoolOptions};
    use uuid::Uuid;

    use crate::{ChatAttributes, ChatMessage, contacts::UsernameContact, db::access::DbAccess};

    use super::*;

    /// The migration that replaced `pending_connection_info`.
    const PENDING_CONNECTION_REQUEST_MIGRATION: i64 = 20260929120000;
    /// The migration that replaced `origin_chat_id`.
    const ORIGIN_GROUP_MIGRATION: i64 = 20261001120000;

    fn at(seconds: i64) -> TimeStamp {
        DateTime::<Utc>::from_timestamp(1_767_225_600 + seconds, 0)
            .unwrap()
            .into()
    }

    fn group_id() -> GroupId {
        QualifiedGroupId::new(Uuid::new_v4(), "example.com".parse().unwrap()).into()
    }

    fn connection_info(group_id: GroupId) -> anyhow::Result<ConnectionInfo> {
        Ok(ConnectionInfo {
            connection_group_id: group_id,
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

    async fn migrate_until(pool: &SqlitePool, version: i64) -> anyhow::Result<()> {
        let mut migrator: Migrator = sqlx::migrate!();
        migrator.migrations = migrator
            .migrations
            .iter()
            .filter(|migration| migration.version < version)
            .cloned()
            .collect::<Vec<_>>()
            .into();
        migrator.run(pool).await?;
        Ok(())
    }

    /// Stores a pending incoming request the way clients did before the
    /// migration.
    async fn store_legacy_request(
        txn: &mut crate::db::access::WriteDbTransaction<'_>,
        sender: &UserId,
        username: Option<&Username>,
        created_at: TimeStamp,
    ) -> anyhow::Result<ChatId> {
        let group_id = group_id();
        let chat = Chat::new_pending_connection_chat(group_id.clone(), sender.clone());
        chat.store(&mut *txn).await?;
        let connection_info = connection_info(group_id)?;
        sqlx::query(
            "INSERT INTO pending_connection_info (
                chat_id, created_at, connection_info, handle
            ) VALUES (?, ?, ?, ?)",
        )
        .bind(chat.id())
        .bind(created_at)
        .bind(&connection_info)
        .bind(username)
        .execute(txn.as_mut())
        .await?;
        Ok(chat.id())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_migration_keeps_pending_requests_and_drops_their_partial_contacts()
    -> anyhow::Result<()> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        migrate_until(&pool, PENDING_CONNECTION_REQUEST_MIGRATION).await?;
        let db = DbAccess::for_tests(pool.clone());

        let username = Username::new("ellie-03".to_owned())?;
        let (via_username, via_group, outgoing) = db
            .with_write_transaction(async |txn| -> anyhow::Result<_> {
                let sender = UserId::random("example.com".parse()?);
                let via_username =
                    store_legacy_request(txn, &sender, Some(&username), at(120)).await?;
                ChatMessage::new_system_message(
                    via_username,
                    at(60),
                    SystemMessage::ReceivedHandleConnectionRequest {
                        sender: sender.clone(),
                        user_handle: username.clone(),
                    },
                )
                .store(&mut *txn)
                .await?;
                UsernameContact::new(
                    username.clone(),
                    via_username,
                    FriendshipPackageEarKey::random()?,
                    ConnectionOfferHash::new_for_test(vec![1; 32]),
                )
                .upsert(&mut *txn)
                .await?;

                let group_sender = UserId::random("example.com".parse()?);
                let via_group = store_legacy_request(txn, &group_sender, None, at(0)).await?;
                // Written by hand, since `origin_group_id` does not exist yet.
                sqlx::query(
                    "INSERT INTO targeted_message_contact (
                        user_uuid, user_domain, chat_id, friendship_package_ear_key, created_at
                    ) VALUES (?, ?, ?, ?, ?)",
                )
                .bind(group_sender.uuid())
                .bind(group_sender.domain())
                .bind(via_group)
                .bind(FriendshipPackageEarKey::random()?)
                .bind(at(0))
                .execute(txn.as_mut())
                .await?;

                // An outgoing request keeps its partial contact.
                let joel = Username::new("joel-03".to_owned())?;
                let outgoing = Chat::new_handle_chat(group_id(), joel.clone());
                outgoing.store(&mut *txn).await?;
                UsernameContact::new(
                    joel,
                    outgoing.id(),
                    FriendshipPackageEarKey::random()?,
                    ConnectionOfferHash::new_for_test(vec![2; 32]),
                )
                .upsert(&mut *txn)
                .await?;
                Ok((via_username, via_group, outgoing.id()))
            })
            .await?;

        sqlx::migrate!().run(&pool).await?;

        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let request = PendingConnectionRequest::load(&mut *txn, via_username)
                .await?
                .unwrap();
            assert_eq!(request.chat_id, via_username);
            assert_eq!(request.username, Some(username.clone()));
            assert_eq!(
                request.received_at,
                at(60),
                "the announcement tells when the request was sent"
            );
            let request = PendingConnectionRequest::load(&mut *txn, via_group)
                .await?
                .unwrap();
            assert_eq!(request.received_at, at(0), "without announcement");

            let remaining: Vec<ChatId> = sqlx::query_scalar(
                "SELECT chat_id FROM username_contact
                UNION ALL SELECT chat_id FROM targeted_message_contact",
            )
            .fetch_all(txn.as_mut())
            .await?;
            assert_eq!(remaining, vec![outgoing]);
            Ok(())
        })
        .await
    }

    /// Stores a pending request the way clients did before the migration that
    /// replaced `origin_chat_id`.
    async fn store_request_with_origin_chat(
        txn: &mut crate::db::access::WriteDbTransaction<'_>,
        origin_chat_id: Option<ChatId>,
    ) -> anyhow::Result<ChatId> {
        let group_id = group_id();
        let sender = UserId::random("example.com".parse()?);
        let chat = Chat::new_pending_connection_chat(group_id.clone(), sender);
        chat.store(&mut *txn).await?;
        sqlx::query(
            "INSERT INTO pending_connection_request (
                request_id, chat_id, created_at, received_at, connection_info, origin_chat_id
            ) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(chat.id())
        .bind(chat.id())
        .bind(at(0))
        .bind(at(0))
        .bind(&connection_info(group_id)?)
        .bind(origin_chat_id)
        .execute(txn.as_mut())
        .await?;
        Ok(chat.id())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_migration_takes_the_group_id_from_the_origin_chat() -> anyhow::Result<()> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        migrate_until(&pool, ORIGIN_GROUP_MIGRATION).await?;
        let db = DbAccess::for_tests(pool.clone());

        let (group, via_group, without_origin) = db
            .with_write_transaction(async |txn| -> anyhow::Result<_> {
                let group = Chat::new_group_chat(
                    group_id(),
                    ChatAttributes::new("Design Team".to_owned(), None),
                );
                group.store(&mut *txn).await?;
                let via_group = store_request_with_origin_chat(txn, Some(group.id())).await?;
                let without_origin = store_request_with_origin_chat(txn, None).await?;
                Ok((group, via_group, without_origin))
            })
            .await?;

        sqlx::migrate!().run(&pool).await?;

        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let request = PendingConnectionRequest::load(&mut *txn, via_group)
                .await?
                .unwrap();
            let origin_group_id = request.origin_group_id.map(GroupId::from);
            assert_eq!(origin_group_id.as_ref(), Some(group.group_id()));

            let request = PendingConnectionRequest::load(&mut *txn, without_origin)
                .await?
                .unwrap();
            assert!(request.origin_group_id.is_none());
            Ok(())
        })
        .await
    }
}
