// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Accepting a connection request as a persistent job.
//!
//! Accepting enqueues a row in `connection_accept_queue`. The client externally
//! joins the connection group and finalizes the accepted state. Transient
//! failures are retried. The row is removed when the join lands, or when the
//! chat is deleted. A permanent failure is recorded on the row and pauses the
//! retries until the user accepts again.

use aircommon::{
    credentials::LeafCredential,
    crypto::indexed_aead::keys::UserProfileKey,
    identifiers::{QualifiedGroupId, UserId},
    messages::{
        client_as::ConnectionOfferHash,
        client_ds::AadMessage,
        client_ds_out::ExternalCommitInfoIn,
        connection_package::{ConnectionPackage, ConnectionPackageHash},
    },
    time::TimeStamp,
};
use airprotos::client::group_bootstrap::{AcceptContext, ConnectionContext, GroupBootstrapCarrier};
use anyhow::{Context, anyhow, ensure};
use mimi_room_policy::RoleIndex;
use openmls::{
    prelude::MlsMessageOut,
    treesync::{RatchetTreeIn, errors::LeafNodeValidationError},
};
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    Chat, ChatId, ChatType,
    chats::PendingConnectionInfo,
    clients::{
        CoreUser, api_clients::DS_ECHO_RETRY_DELAYS, connection_offer::payload::ConnectionInfo,
    },
    groups::{Group, group_bootstrap::secret_bytes, self_group::SelfGroup},
    key_stores::indexed_keys::StorableIndexedKey,
    outbound_service::{
        OutboundServiceContext,
        error::{OutboundServiceError, classify_ds_error},
    },
    usernames::connection_packages::StorableConnectionPackage,
};

/// A queued connection accept.
pub(crate) struct ConnectionAccept;

/// State of a queued connection accept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionAcceptStatus {
    /// Queued or in flight.
    Pending,
    /// Failed permanently. Retried only when the user accepts again.
    Failed { reason: String },
}

impl CoreUser {
    /// The state of the queued accept of the given chat's connection request,
    /// or `None` when no accept is queued.
    pub async fn connection_accept_status(
        &self,
        chat_id: ChatId,
    ) -> anyhow::Result<Option<ConnectionAcceptStatus>> {
        Ok(ConnectionAccept::status(self.db().read().await?, chat_id).await?)
    }
}

/// The commit joining the connection group, its group info, and the sealed
/// group bootstrap for sibling clients, if any.
type JoinMessages = (MlsMessageOut, MlsMessageOut, Option<Vec<u8>>);

/// The pending state an accept operates on.
struct AcceptData {
    sender_user_id: UserId,
    pending_connection_info: PendingConnectionInfo,
    own_user_profile_key: UserProfileKey,
}

impl OutboundServiceContext {
    pub(super) async fn perform_queued_connection_accepts(
        &self,
        run_token: &CancellationToken,
    ) -> anyhow::Result<()> {
        // Used to identify rows locked by this task
        let task_id = Uuid::new_v4();
        loop {
            if run_token.is_cancelled() {
                return Ok(()); // the task is being stopped
            }

            let Some(chat_id) = self
                .db
                .with_write_transaction(async |txn| ConnectionAccept::dequeue(txn, task_id).await)
                .await?
            else {
                return Ok(());
            };
            info!(%chat_id, "Performing queued connection accept");

            match Box::pin(self.execute_connection_accept(chat_id)).await {
                Ok(()) => {}
                Err(OutboundServiceError::Recoverable(error)) => {
                    info!(%error, "Connection accept did not go through; will retry later");
                }
                Err(OutboundServiceError::Fatal(error)) => {
                    error!(%error, "Connection accept failed permanently");
                    ConnectionAccept::mark_failed(
                        self.db.write().await?,
                        chat_id,
                        &error.to_string(),
                    )
                    .await?;
                }
            }
        }
    }

    /// Runs one accept attempt.
    ///
    /// The attempt is idempotent in case an earlier confirmation was lost and
    /// the client is actually already a member. Recoverable errors leave the
    /// queue row in place for a later run.
    async fn execute_connection_accept(&self, chat_id: ChatId) -> Result<(), OutboundServiceError> {
        let Some(accept_data) = self.load_accept_data(chat_id).await? else {
            return Ok(());
        };
        let AcceptData {
            sender_user_id,
            pending_connection_info,
            own_user_profile_key,
        } = accept_data;
        let PendingConnectionInfo {
            chat_id: _,
            created_at: _,
            connection_info,
            handle: _,
            connection_offer_hash,
            connection_package_hash,
        } = pending_connection_info;

        let mut eci = self.fetch_connection_group_info(&connection_info).await?;
        let mut member = tree_contains_user(&eci.ratchet_tree_in, self.user_id());

        // An earlier attempt may have landed on the DS while its confirmation
        // was lost. The DS then already counts us as a member and would reject
        // another external commit. A leftover local group marks such an
        // attempt, so give the DS state a moment to show the join before the
        // destructive rejoin below.
        if !member && self.load_local_group(chat_id).await?.is_some() {
            for delay in DS_ECHO_RETRY_DELAYS {
                sleep(delay).await;
                eci = self.fetch_connection_group_info(&connection_info).await?;
                member = tree_contains_user(&eci.ratchet_tree_in, self.user_id());
                if member {
                    break;
                }
            }
        }
        if member {
            return self.finalize_landed_join(chat_id, &eci).await;
        }

        let (aad, qgid) = CoreUser::prepare_group(
            &self.key_store,
            self.user_id(),
            &connection_info,
            &own_user_profile_key,
        )
        .map_err(OutboundServiceError::fatal)?;

        // Join the group with an external commit. A group left over from an
        // attempt the DS never saw is replaced.
        let (commit, group_info, group_bootstrap) = self
            .join_connection_group(
                chat_id,
                &sender_user_id,
                &connection_info,
                connection_offer_hash,
                connection_package_hash,
                aad,
                eci,
            )
            .await?
            .map_err(|error| {
                OutboundServiceError::fatal(anyhow!("Incompatible client: {error}"))
            })?;

        // Send the join to the DS.
        let api_client = self
            .api_clients
            .get(qgid.owning_domain())
            .map_err(OutboundServiceError::fatal)?;
        let qs_client_reference = self
            .key_store
            .create_own_client_reference(&self.qs_client_id);
        let response = api_client
            .ds_join_connection_group(
                commit,
                group_info,
                qs_client_reference,
                &connection_info.connection_group_ear_key,
                group_bootstrap,
            )
            .await;

        match response {
            Ok(_) => self
                .db
                .with_write_transaction(async |txn| -> anyhow::Result<_> {
                    CoreUser::finalize_accepted_connection(txn, chat_id, TimeStamp::now()).await
                })
                .await
                .map_err(OutboundServiceError::fatal),
            // The join stays committed locally. The next attempt sees the
            // group and either finalizes (if the request landed) or rejoins.
            Err(error) if error.is_network_error() => Err(OutboundServiceError::recoverable(error)),
            Err(error) => {
                // The DS rejected the commit.
                let eci = self.fetch_connection_group_info(&connection_info).await?;
                if tree_contains_user(&eci.ratchet_tree_in, self.user_id()) {
                    self.finalize_landed_join(chat_id, &eci).await
                } else if error.is_definitive_rejection() {
                    Err(OutboundServiceError::fatal(error))
                } else {
                    // E.g. an internal server error. The commit may go
                    // through on a retry.
                    Err(OutboundServiceError::recoverable(error))
                }
            }
        }
    }

    /// Loads the pending state the accept operates on.
    ///
    /// Returns `None` and removes the queue row when the job is orphaned.
    async fn load_accept_data(
        &self,
        chat_id: ChatId,
    ) -> Result<Option<AcceptData>, OutboundServiceError> {
        self.db
            .with_write_transaction(async |txn| -> anyhow::Result<_> {
                let Some(chat) = Chat::load(&mut *txn, &chat_id).await? else {
                    ConnectionAccept::remove(&mut *txn, chat_id).await?;
                    return Ok(None);
                };
                let ChatType::PendingConnection(sender_user_id) = chat.chat_type() else {
                    ConnectionAccept::remove(&mut *txn, chat_id).await?;
                    return Ok(None);
                };
                let sender_user_id = sender_user_id.clone();

                let pending_connection_info = PendingConnectionInfo::load(&mut *txn, chat_id)
                    .await?
                    .with_context(|| {
                        format!("No pending connection info found for chat: {chat_id}")
                    })?;

                // The finalize at the end of the accept needs the partial
                // contact, so a missing one fails the job up front.
                CoreUser::load_partial_contact(
                    &mut *txn,
                    chat_id,
                    &pending_connection_info,
                    &sender_user_id,
                )
                .await?;

                let own_user_profile_key = UserProfileKey::load_own(&mut *txn).await?;

                Ok(Some(AcceptData {
                    sender_user_id,
                    pending_connection_info,
                    own_user_profile_key,
                }))
            })
            .await
            .map_err(OutboundServiceError::fatal)
    }

    /// Fetches the connection group state from the DS.
    async fn fetch_connection_group_info(
        &self,
        connection_info: &ConnectionInfo,
    ) -> Result<ExternalCommitInfoIn, OutboundServiceError> {
        let qgid: QualifiedGroupId = connection_info
            .connection_group_id
            .clone()
            .try_into()
            .map_err(OutboundServiceError::fatal)?;
        let api_client = self
            .api_clients
            .get(qgid.owning_domain())
            .map_err(OutboundServiceError::fatal)?;
        api_client
            .ds_connection_group_info(
                connection_info.connection_group_id.clone(),
                &connection_info.connection_group_ear_key,
            )
            .await
            .map_err(classify_ds_error)
    }

    /// A group that resulted from an earlier join, if any.
    async fn load_local_group(
        &self,
        chat_id: ChatId,
    ) -> Result<Option<Group>, OutboundServiceError> {
        let connection = self.db.read().await.map_err(OutboundServiceError::fatal)?;
        Group::load_with_chat_id(connection, chat_id)
            .await
            .map_err(OutboundServiceError::fatal)
    }

    /// Finalizes the accept, but only when the local group matches the DS group
    /// state.
    async fn finalize_landed_join(
        &self,
        chat_id: ChatId,
        eci: &ExternalCommitInfoIn,
    ) -> Result<(), OutboundServiceError> {
        let Some(group) = self.load_local_group(chat_id).await? else {
            return Err(OutboundServiceError::fatal(anyhow!(
                "the join landed on the DS, but there is no local group. \
                Recovery requires a resync"
            )));
        };
        let local = group.mls_group().public_group().group_context();
        let remote = eci.verifiable_group_info.group_context();
        if local.epoch() < remote.epoch() {
            return Err(OutboundServiceError::recoverable(anyhow!(
                "the local group lags behind the DS group state. \
                Retrying after the queued messages are processed"
            )));
        }
        if local.epoch() > remote.epoch()
            || local.confirmed_transcript_hash() != remote.confirmed_transcript_hash()
        {
            return Err(OutboundServiceError::fatal(anyhow!(
                "the join landed on the DS, but the local group state does not match. \
                Recovery requires a resync"
            )));
        }

        self.db
            .with_write_transaction(async |txn| -> anyhow::Result<_> {
                CoreUser::finalize_accepted_connection(txn, chat_id, TimeStamp::now()).await
            })
            .await
            .map_err(OutboundServiceError::fatal)
    }

    /// Builds and locally commits the external commit joining the connection
    /// group, replacing any group left over from an earlier attempt.
    #[expect(clippy::too_many_arguments)]
    async fn join_connection_group(
        &self,
        chat_id: ChatId,
        sender_user_id: &UserId,
        connection_info: &ConnectionInfo,
        connection_offer_hash: Option<ConnectionOfferHash>,
        connection_package_hash: Option<ConnectionPackageHash>,
        aad: AadMessage,
        eci: ExternalCommitInfoIn,
    ) -> Result<Result<JoinMessages, LeafNodeValidationError>, OutboundServiceError> {
        let user_id = self.user_id().clone();
        Box::pin(self.db.with_write_transaction(
            async |txn| -> anyhow::Result<Result<JoinMessages, LeafNodeValidationError>> {
                if let Some(group) = Group::load_with_chat_id(&mut *txn, chat_id).await? {
                    warn!(%chat_id, "Group for pending chat already exists");
                    Group::delete_from_db(txn, group.group_id()).await?;
                    if let Some(hash) = connection_offer_hash {
                        Group::delete_connection_offer_psk(&mut *txn, hash)?;
                    }
                }

                let self_group = SelfGroup::load(&mut *txn).await?;
                let vc_group_id = self_group.as_ref().map(|group| group.group_id().clone());

                // Join group
                let res = Group::join_group_externally(
                    txn,
                    &self.api_clients,
                    eci,
                    self.signing_key(),
                    connection_info.connection_group_ear_key.clone(),
                    connection_info
                        .connection_group_identity_link_wrapper_key
                        .clone(),
                    aad,
                    connection_offer_hash,
                    vc_group_id,
                )
                .await?;
                let (mut group, commit, group_info, mut member_profile_info) = match res {
                    Ok(value) => value,
                    Err(error) => return Ok(Err(error)),
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
                    members.contains(&user_id) && members.contains(sender_user_id),
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
                CoreUser::schedule_fetch_user_profile(&mut *txn, contact_profile_info).await?;

                group.room_state_change_role(sender_user_id, &user_id, RoleIndex::Regular)?;

                let now = TimeStamp::now();
                group.store_update(&mut *txn, Some(now), Some(now)).await?;

                if let Some(hash) = connection_package_hash {
                    // Delete the connection package if it's not last resort
                    let is_last_resort =
                        <ConnectionPackage as StorableConnectionPackage>::is_last_resort(
                            &mut *txn, &hash,
                        )
                        .await?
                        .unwrap_or(false);
                    if !is_last_resort {
                        ConnectionPackage::delete(&mut *txn, &hash)
                            .await
                            .context("Failed to delete connection package")?;
                    }
                }

                let group_bootstrap = match &self_group {
                    Some(self_group) => {
                        let friendship_package = &connection_info.friendship_package;
                        let connection = ConnectionContext::Accept(AcceptContext {
                            user_id: Some(sender_user_id.clone().into()),
                            friendship_token: Some(friendship_package.friendship_token.clone()),
                            wai_ear_key: Some(secret_bytes(&friendship_package.wai_ear_key)),
                            user_profile_base_secret: Some(secret_bytes(
                                &friendship_package.user_profile_base_secret,
                            )),
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

                Ok(Ok((commit, group_info, group_bootstrap)))
            },
        ))
        .await
        .map_err(OutboundServiceError::fatal)
    }
}

/// Whether the ratchet tree contains a leaf carrying `user_id`'s credential.
fn tree_contains_user(ratchet_tree: &RatchetTreeIn, user_id: &UserId) -> bool {
    ratchet_tree.leaves().any(|leaf| {
        LeafCredential::from_credential(leaf.credential())
            .ok()
            .and_then(LeafCredential::into_user)
            .is_some_and(|credential| credential.user_id() == user_id)
    })
}

mod persistence {
    use sqlx::{query, query_scalar};

    use crate::db::access::{ReadConnection, WriteConnection, WriteDbTransaction};

    use super::*;

    impl ConnectionAccept {
        /// Enqueues the accept of the given chat's connection request.
        ///
        /// An existing accept that permanently failed in the past will be
        /// retried.
        pub(crate) async fn enqueue(
            mut connection: impl WriteConnection,
            chat_id: ChatId,
        ) -> sqlx::Result<()> {
            query!(
                "INSERT INTO connection_accept_queue (chat_id) VALUES (?1)
                ON CONFLICT (chat_id) DO UPDATE SET failed_reason = NULL",
                chat_id,
            )
            .execute(connection.as_mut())
            .await?;
            connection.notifier().update(chat_id);
            Ok(())
        }

        /// Dequeues an accept for processing that is not failed and has not
        /// been locked by this task.
        pub(super) async fn dequeue(
            txn: &mut WriteDbTransaction<'_>,
            task_id: Uuid,
        ) -> sqlx::Result<Option<ChatId>> {
            query_scalar!(
                r#"UPDATE connection_accept_queue
                SET locked_by = ?1
                WHERE chat_id = (
                    SELECT chat_id
                    FROM connection_accept_queue
                    WHERE failed_reason IS NULL
                        AND (locked_by IS NULL OR locked_by != ?1)
                    LIMIT 1
                )
                RETURNING chat_id AS "chat_id: ChatId""#,
                task_id,
            )
            .fetch_optional(txn.as_mut())
            .await
        }

        pub(super) async fn mark_failed(
            mut connection: impl WriteConnection,
            chat_id: ChatId,
            reason: &str,
        ) -> sqlx::Result<()> {
            query!(
                "UPDATE connection_accept_queue SET failed_reason = ?2 WHERE chat_id = ?1",
                chat_id,
                reason,
            )
            .execute(connection.as_mut())
            .await?;
            connection.notifier().update(chat_id);
            Ok(())
        }

        pub(crate) async fn remove(
            mut connection: impl WriteConnection,
            chat_id: ChatId,
        ) -> sqlx::Result<()> {
            query!(
                "DELETE FROM connection_accept_queue WHERE chat_id = ?",
                chat_id
            )
            .execute(connection.as_mut())
            .await?;
            connection.notifier().update(chat_id);
            Ok(())
        }

        pub(crate) async fn status(
            mut connection: impl ReadConnection,
            chat_id: ChatId,
        ) -> sqlx::Result<Option<ConnectionAcceptStatus>> {
            let row = query!(
                "SELECT failed_reason FROM connection_accept_queue WHERE chat_id = ?",
                chat_id
            )
            .fetch_optional(connection.as_mut())
            .await?;
            Ok(row.map(|row| match row.failed_reason {
                Some(reason) => ConnectionAcceptStatus::Failed { reason },
                None => ConnectionAcceptStatus::Pending,
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use sqlx::SqlitePool;

    use crate::{
        chats::persistence::tests::test_chat,
        db::access::{DbAccess, WriteConnection},
    };

    use super::*;

    #[sqlx::test]
    async fn enqueue_dequeue_fail_rearm_remove(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);
        let mut connection = pool.write().await?;
        let mut txn = connection.begin().await?;

        let chat = test_chat();
        chat.store(&mut txn).await?;
        let chat_id = chat.id();

        assert_eq!(ConnectionAccept::status(&mut txn, chat_id).await?, None);

        ConnectionAccept::enqueue(&mut txn, chat_id).await?;
        assert_eq!(
            ConnectionAccept::status(&mut txn, chat_id).await?,
            Some(ConnectionAcceptStatus::Pending)
        );

        // A row locked by this task is not dequeued again.
        let task_id = Uuid::new_v4();
        assert_eq!(
            ConnectionAccept::dequeue(&mut txn, task_id).await?,
            Some(chat_id)
        );
        assert_eq!(ConnectionAccept::dequeue(&mut txn, task_id).await?, None);

        // A new task dequeues it again.
        let other_task_id = Uuid::new_v4();
        assert_eq!(
            ConnectionAccept::dequeue(&mut txn, other_task_id).await?,
            Some(chat_id)
        );

        // A failed row is not dequeued, but stays visible.
        ConnectionAccept::mark_failed(&mut txn, chat_id, "boom").await?;
        assert_eq!(
            ConnectionAccept::dequeue(&mut txn, Uuid::new_v4()).await?,
            None
        );
        assert_eq!(
            ConnectionAccept::status(&mut txn, chat_id).await?,
            Some(ConnectionAcceptStatus::Failed {
                reason: "boom".to_owned()
            })
        );

        // Re-accepting re-arms the job.
        ConnectionAccept::enqueue(&mut txn, chat_id).await?;
        assert_eq!(
            ConnectionAccept::status(&mut txn, chat_id).await?,
            Some(ConnectionAcceptStatus::Pending)
        );
        assert_eq!(
            ConnectionAccept::dequeue(&mut txn, Uuid::new_v4()).await?,
            Some(chat_id)
        );

        ConnectionAccept::remove(&mut txn, chat_id).await?;
        assert_eq!(ConnectionAccept::status(&mut txn, chat_id).await?, None);

        Ok(())
    }
}
