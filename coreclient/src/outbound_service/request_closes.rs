// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Closes the connection groups of redundant incoming requests, see
//! [`ClosingConnectionRequest`].
//!
//! The closer joins the group and deletes it with an `AlreadyConnected`
//! tombstone in one DS call. Several of our devices may close the same group.
//! The DS applies the first close and finds the group closed for the others.

use aircommon::{
    credentials::keys::LeafSigningKey,
    crypto::indexed_aead::keys::UserProfileKey,
    identifiers::QualifiedGroupId,
    messages::{
        client_ds::{AadMessage, AadPayload, CloseConnectionGroupParamsAad},
        client_ds_out::{DeleteGroupParamsOut, ExternalCommitInfoIn},
    },
    time::TimeStamp,
};
use airprotos::client::component::GroupTombstone;
use apqmls::commit_builder::ApqCommitMessageBundle;
use mls_assist::messages::AssistedMessageOut;
use tls_codec::DeserializeBytes;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::{
    chats::{ClosingConnectionRequest, DueClose},
    db::access::WriteDbTransaction,
    groups::{ConnectionGroupJoin, Group, GroupClosed},
    key_stores::indexed_keys::StorableIndexedKey,
    outbound_service::{error::OutboundServiceError, retry::RetryPolicy},
};

use super::OutboundServiceContext;

/// The join and the delete of a close, ready to send.
enum CloseCommits {
    T {
        join: Box<AssistedMessageOut>,
        delete: Box<DeleteGroupParamsOut>,
    },
    Apq {
        join: Box<ApqCommitMessageBundle>,
        delete: Box<ApqCommitMessageBundle>,
    },
}

impl OutboundServiceContext {
    /// Closes the connection groups of the parked redundant requests.
    ///
    /// A request stays parked until the close is done or the group is gone,
    /// unless the close failed fatally or its retry budget is used up.
    pub(super) async fn close_redundant_requests(
        &self,
        run_token: &CancellationToken,
    ) -> Result<(), OutboundServiceError> {
        let due =
            ClosingConnectionRequest::load_due(self.db.read().await?, TimeStamp::now()).await?;
        let policy = RetryPolicy::REQUEST_CLOSES;
        for DueClose { request, attempts } in due {
            if run_token.is_cancelled() {
                return Ok(());
            }
            let request_id = request.request_id;
            let result = Box::pin(self.close_request(&request)).await;
            match policy.fatal_when_exhausted(result, attempts) {
                Ok(()) => {
                    info!(%request_id, "Closed a redundant connection request");
                    self.drop_close(&request).await?;
                }
                Err(OutboundServiceError::Fatal(error)) => {
                    error!(
                        %error,
                        %request_id,
                        "Failed to close a redundant connection request; dropping"
                    );
                    self.drop_close(&request).await?;
                }
                Err(OutboundServiceError::Recoverable(error)) => {
                    warn!(
                        %error,
                        %request_id,
                        "Failed to close a redundant connection request; retrying later"
                    );
                    if let Some((attempts, retry_at)) = policy.defer(error.cause, attempts) {
                        ClosingConnectionRequest::defer(
                            self.db.write().await?,
                            request_id,
                            attempts,
                            retry_at.into(),
                        )
                        .await?;
                    }
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    /// Joins and deletes the connection group of `request`. A group that is
    /// gone or closed needs nothing more.
    async fn close_request(
        &self,
        request: &ClosingConnectionRequest,
    ) -> Result<(), OutboundServiceError> {
        let info = &request.connection_info;
        let qgid =
            QualifiedGroupId::tls_deserialize_exact_bytes(info.connection_group_id.as_slice())
                .map_err(OutboundServiceError::fatal)?;
        let api_client = self.api_clients.get(qgid.owning_domain())?;
        let ear_key = &info.connection_group_ear_key;

        let external_commit_info = match api_client
            .ds_connection_group_info(info.connection_group_id.clone(), ear_key)
            .await
        {
            Ok(external_commit_info) => external_commit_info,
            Err(error) if error.is_not_found() => return Ok(()),
            Err(error) => return Err(error.into()),
        };

        let staged = Box::pin(self.db.with_write_transaction(async |txn| {
            Box::pin(self.stage_close(txn, request, external_commit_info)).await
        }))
        .await;
        let commits = match staged {
            Ok(commits) => commits,
            // The group info carries a tombstone, a sibling closed it already.
            Err(error) if error.downcast_ref::<GroupClosed>().is_some() => return Ok(()),
            Err(error) => return Err(OutboundServiceError::fatal(error)),
        };

        let qs_client_reference = self
            .key_store
            .create_own_client_reference(&self.qs_client_id);
        let sent = match commits {
            CloseCommits::T { join, delete } => {
                api_client
                    .ds_close_connection_group(*join, *delete, qs_client_reference, ear_key)
                    .await
            }
            CloseCommits::Apq { join, delete } => {
                api_client
                    .ds_apq_close_connection_group(*join, *delete, qs_client_reference, ear_key)
                    .await
            }
        };
        match sent {
            Ok(_) => Ok(()),
            // Not found covers a group a sibling closed first.
            Err(error) if error.is_not_found() => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    /// Joins the connection group of `request` and stages its delete.
    ///
    /// The join adds a leaf of this device alone, since the group is gone
    /// right after it.
    async fn stage_close(
        &self,
        txn: &mut WriteDbTransaction<'_>,
        request: &ClosingConnectionRequest,
        external_commit_info: ExternalCommitInfoIn,
    ) -> anyhow::Result<CloseCommits> {
        let own_user_id = self.user_id();
        let info = &request.connection_info;
        let profile_key = UserProfileKey::load_own(&mut *txn).await?;
        let encrypted_user_profile_key = profile_key.encrypt(
            &info.connection_group_identity_link_wrapper_key,
            own_user_id,
        )?;
        let aad: AadMessage = AadPayload::CloseConnectionGroup(CloseConnectionGroupParamsAad {
            encrypted_user_profile_key,
        })
        .into();
        let connection_group = Some(ConnectionGroupJoin {
            inviter: &request.sender,
            connection_offer_hash: Some(request.connection_offer_hash),
        });
        let signer = LeafSigningKey::User(self.signing_key().clone());
        let tombstone = Some(GroupTombstone::already_connected(TimeStamp::now()));

        if external_commit_info.is_apq()? {
            let (mut group, join, _profile_infos) = Box::pin(Group::join_apq_group_externally(
                txn,
                &self.api_clients,
                external_commit_info,
                &signer,
                own_user_id,
                info.connection_group_ear_key.clone(),
                info.connection_group_identity_link_wrapper_key.clone(),
                aad,
                None,
                connection_group,
            ))
            .await??;
            let delete = group
                .stage_apq_delete(&mut *txn, &signer, tombstone)
                .await?;
            Ok(CloseCommits::Apq {
                join: Box::new(join),
                delete: Box::new(delete),
            })
        } else {
            let (mut group, commit, group_info, _profile_infos) =
                Box::pin(Group::join_group_externally(
                    txn,
                    &self.api_clients,
                    external_commit_info,
                    self.signing_key(),
                    info.connection_group_ear_key.clone(),
                    info.connection_group_identity_link_wrapper_key.clone(),
                    aad,
                    connection_group,
                    None,
                ))
                .await??;
            let delete = group.stage_delete(&mut *txn, &signer, tombstone).await?;
            Ok(CloseCommits::T {
                join: Box::new(AssistedMessageOut::new(commit, Some(group_info))),
                delete: Box::new(delete),
            })
        }
    }

    /// Forgets `request` with its local connection group and the PSK its join
    /// referenced.
    async fn drop_close(
        &self,
        request: &ClosingConnectionRequest,
    ) -> Result<(), OutboundServiceError> {
        self.db
            .with_write_transaction(async |txn| -> anyhow::Result<()> {
                Group::delete_from_db(txn, &request.connection_info.connection_group_id).await?;
                Group::delete_connection_offer_psk(&mut *txn, request.connection_offer_hash)?;
                ClosingConnectionRequest::delete(&mut *txn, request.request_id).await?;
                Ok(())
            })
            .await
            .map_err(OutboundServiceError::fatal)
    }
}
