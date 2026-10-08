// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Fetching operations for user and group profiles.

use std::convert::Infallible;

use aircommon::{
    credentials::UserCredential,
    crypto::indexed_aead::{ciphertexts::IndexDecryptable, keys::UserProfileKey},
    identifiers::{QualifiedGroupId, RemoteAttachmentId, UserId},
    messages::{client_as_out::GetUserProfileResponse, client_ds_out::ExternalCommitInfoIn},
    time::TimeStamp,
};
use airprotos::{
    client::group::{ExternalGroupProfile, GroupProfile},
    delivery_service::v1::StorageObjectType,
};
use anyhow::{Context, ensure};
use openmls::group::GroupId;
use serde::{Deserialize, Serialize};
use tls_codec::{DeserializeBytes, Serialize as _};
use tracing::{debug, error, info, warn};

use crate::{
    Chat, ChatAttributes, ChatId, ChatStatus,
    chats::PendingConnectionRequest,
    clients::{
        CoreUser, connection_offer::payload::ConnectionInfo, update_key::update_chat_attributes,
    },
    db::access::WriteConnection,
    groups::{Group, ProfileInfo},
    job::operation::OperationId,
    key_stores::indexed_keys::StorableIndexedKey,
    user_profiles::{VerifiableUserProfile, process::ExistingUserProfile},
};

use super::{
    Job, JobContext, JobError,
    operation::{OperationData, OperationKind},
};

impl CoreUser {
    /// Schedule a user profile fetch operation.
    ///
    /// This will be executed on the next run of the outbound service.
    pub(crate) async fn schedule_fetch_user_profile(
        connection: impl WriteConnection,
        profile_info: impl Into<ProfileInfo>,
    ) -> sqlx::Result<()> {
        let ProfileInfo {
            user_credential,
            user_profile_key,
        } = profile_info.into();
        FetchUserProfileOperation::new(user_credential, user_profile_key)
            .into_operation()
            .enqueue(connection)
            .await
    }

    /// Schedule fetching the profile of the sender of a pending incoming
    /// request.
    pub(crate) async fn schedule_fetch_request_sender_profile(
        connection: impl WriteConnection,
        request_id: ChatId,
        sender_credential: UserCredential,
    ) -> sqlx::Result<()> {
        FetchRequestSenderProfileOperation {
            request_id,
            sender_credential,
        }
        .into_operation()
        .enqueue(connection)
        .await
    }

    /// Fetches the profile of the sender of an incoming connection request. The
    /// profile key can be stale (when the sender rotated it), so it needs to be
    /// fetched from the connection group.
    pub(crate) async fn fetch_request_sender_profile(
        context: &mut JobContext<'_, '_>,
        connection_info: &ConnectionInfo,
        sender_credential: &UserCredential,
    ) -> Result<(), JobError<Infallible>> {
        let sender = sender_credential.user_id();
        let offered_key = UserProfileKey::from_base_secret(
            connection_info
                .friendship_package
                .user_profile_base_secret
                .clone(),
            sender,
        )
        .map_err(JobError::fatal)?;
        let fetch = FetchUserProfileOperation::new(sender_credential.clone(), offered_key);
        let Err(error) = fetch.execute(context).await else {
            return Ok(());
        };
        warn!(%error, "Failed to fetch user profile; falling back to fetching group info");

        let qgid = QualifiedGroupId::tls_deserialize_exact_bytes(
            connection_info.connection_group_id.as_slice(),
        )?;
        let eci = context
            .api_clients
            .get(qgid.owning_domain())?
            .ds_connection_group_info(
                connection_info.connection_group_id.clone(),
                &connection_info.connection_group_ear_key,
            )
            .await?;
        let current_key =
            sender_profile_key(&eci, connection_info, sender).map_err(JobError::fatal)?;
        FetchUserProfileOperation::new(sender_credential.clone(), current_key)
            .execute(context)
            .await
    }

    /// Schedule a group profile fetch operation.
    ///
    /// This will be executed on the next run of the outbound service.
    pub(crate) async fn schedule_fetch_group_profile(
        connection: impl WriteConnection,
        group_id: GroupId,
        sender_id: UserId,
        uploaded_at: TimeStamp,
        external_group_profile: ExternalGroupProfile,
        is_initial_fetch: bool,
    ) -> sqlx::Result<()> {
        FetchGroupProfileOperation {
            group_id,
            sender_id,
            uploaded_at,
            external_group_profile,
            is_initial_fetch,
        }
        .into_operation()
        .enqueue(connection)
        .await
    }
}

/// Decrypts the sender's current profile key from the info of their unjoined
/// connection group.
fn sender_profile_key(
    eci: &ExternalCommitInfoIn,
    connection_info: &ConnectionInfo,
    sender: &UserId,
) -> anyhow::Result<UserProfileKey> {
    let encrypted_user_profile_key = if !eci.indexed_encrypted_user_profile_keys.is_empty() {
        ensure!(
            eci.indexed_encrypted_user_profile_keys.len() == 1,
            "Unjoined connection group must have exactly one user profile key"
        );
        eci.indexed_encrypted_user_profile_keys
            .values()
            .next()
            .expect("logic error: len == 1")
    } else {
        ensure!(
            eci.encrypted_user_profile_keys.len() == 1,
            "Unjoined connection group must have exactly one user profile key"
        );
        &eci.encrypted_user_profile_keys[0]
    };
    Ok(UserProfileKey::decrypt(
        &connection_info.connection_group_identity_link_wrapper_key,
        encrypted_user_profile_key,
        sender,
    )?)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FetchUserProfileOperation {
    // Persisted CBOR field name; predates the rename to user credential.
    #[serde(rename = "client_credential")]
    user_credential: UserCredential,
    user_profile_key: UserProfileKey,
}

impl FetchUserProfileOperation {
    pub(crate) fn new(user_credential: UserCredential, user_profile_key: UserProfileKey) -> Self {
        Self {
            user_credential,
            user_profile_key,
        }
    }
}

impl OperationData for FetchUserProfileOperation {
    fn kind() -> OperationKind {
        OperationKind::FetchUserProfile
    }

    fn generate_id(&self) -> OperationId {
        let mut bytes = Vec::new();
        bytes.push(Self::kind() as u8);
        let user_id = self.user_credential.user_id();
        if let Err(error) = user_id.tls_serialize(&mut bytes) {
            error!(%error, "error white serializing user id");
        }
        OperationId(bytes)
    }
}

impl Job for FetchUserProfileOperation {
    type Output = ();

    type DomainError = Infallible;

    async fn execute_logic(
        self,
        context: &mut JobContext<'_, '_>,
    ) -> Result<Self::Output, JobError<Self::DomainError>> {
        let Self {
            user_credential,
            user_profile_key,
        } = self;

        let user_id = user_credential.user_id();

        // Phase 1: Check if the profile in the DB is up to date.
        let existing_user_profile =
            ExistingUserProfile::load(context.db.read().await?, user_id).await?;
        if existing_user_profile.matches_index(user_profile_key.index()) {
            return Ok(());
        }

        // Phase 2: Fetch the user profile from the server
        let api_client = context.api_clients.get(user_id.domain())?;
        let GetUserProfileResponse {
            encrypted_user_profile,
        } = api_client
            .as_get_user_profile(user_id.clone(), user_profile_key.index().clone())
            .await?;

        // Phase 3: Decrypt and process the user profile
        let verifiable_user_profile =
            VerifiableUserProfile::decrypt_with_index(&user_profile_key, &encrypted_user_profile)
                .map_err(JobError::fatal)?;
        let persistable_user_profile = existing_user_profile
            .process_decrypted_user_profile(verifiable_user_profile, &user_credential)
            .map_err(JobError::fatal)?;

        // Phase 4: Store the user profile and key in the database
        //
        // A rotation of our own profile reaches us through this path too, when
        // a sibling device performs it.
        let is_own_profile = user_id == context.key_store.signing_key.credential().user_id();
        let mut write = context.db.write().await?;
        write
            .with_transaction(async |txn| -> anyhow::Result<()> {
                if is_own_profile {
                    user_profile_key.store_own(&mut *txn).await?;
                } else {
                    user_profile_key.store(&mut *txn).await?;
                }
                persistable_user_profile.persist(&mut *txn).await?;
                if let Some(old_user_profile_index) = persistable_user_profile.old_profile_index() {
                    // Delete the old user profile key
                    UserProfileKey::delete(txn, old_user_profile_index).await?;
                }
                Ok(())
            })
            .await?;

        Ok(())
    }
}

/// Fetches the profile of the sender of a pending request a sibling handed
/// over.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FetchRequestSenderProfileOperation {
    request_id: ChatId,
    sender_credential: UserCredential,
}

impl OperationData for FetchRequestSenderProfileOperation {
    fn kind() -> OperationKind {
        OperationKind::FetchRequestSenderProfile
    }

    fn generate_id(&self) -> OperationId {
        let mut bytes = Vec::new();
        bytes.push(Self::kind() as u8);
        bytes.extend(self.request_id.uuid().as_bytes());
        OperationId(bytes)
    }
}

impl Job for FetchRequestSenderProfileOperation {
    type Output = ();

    type DomainError = Infallible;

    async fn execute_logic(
        self,
        context: &mut JobContext<'_, '_>,
    ) -> Result<Self::Output, JobError<Self::DomainError>> {
        let Self {
            request_id,
            sender_credential,
        } = self;
        let request = PendingConnectionRequest::load(context.db.read().await?, request_id).await?;
        // An accepted request fetched the profile when joining the connection
        // group. A declined one needs none.
        let Some(request) = request else {
            debug!(%request_id, "Request is no longer pending, skipping its profile");
            return Ok(());
        };
        CoreUser::fetch_request_sender_profile(
            context,
            &request.connection_info,
            &sender_credential,
        )
        .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FetchGroupProfileOperation {
    group_id: GroupId,
    sender_id: UserId,
    uploaded_at: TimeStamp,
    external_group_profile: ExternalGroupProfile,
    /// Set when the operation is enqueued from the welcome flow.
    ///
    /// When set the fetch represents the initial download of the group profile and must not produce
    /// "title changed" / "picture changed" system messages.
    #[serde(default)]
    is_initial_fetch: bool,
}

impl OperationData for FetchGroupProfileOperation {
    fn kind() -> OperationKind {
        OperationKind::FetchGroupProfile
    }

    fn generate_id(&self) -> OperationId {
        let mut bytes = Vec::new();
        bytes.push(Self::kind() as u8);
        bytes.extend(self.group_id.as_slice());
        OperationId(bytes)
    }
}

impl Job for FetchGroupProfileOperation {
    type Output = ();

    type DomainError = Infallible;

    async fn execute_logic(
        self,
        context: &mut JobContext<'_, '_>,
    ) -> Result<Self::Output, JobError<Self::DomainError>> {
        let Self {
            group_id,
            sender_id,
            uploaded_at,
            external_group_profile,
            is_initial_fetch,
        } = self;

        info!(
            ?group_id,
            object_id = %external_group_profile.object_id,
            ?uploaded_at,
            "Fetching group profile"
        );

        // Load chat and group
        let Some((mut chat, group)) = context
            .db
            .write()
            .await?
            .with_transaction(async |txn| -> anyhow::Result<_> {
                let chat = Chat::load_by_group_id(&mut *txn, &group_id)
                    .await?
                    .context("Missing chat")?;
                if let ChatStatus::Blocked = chat.status() {
                    return Ok(None);
                }
                let group = Group::load_ref(txn, &group_id)
                    .await?
                    .context("Missing group")?;
                Ok(Some((chat, group)))
            })
            .await?
        else {
            return Ok(()); // blocked chat
        };

        // Fetch group profile from the object storage
        let api_client = context.api_clients.get(&chat.owner_domain())?;
        let remote_attachment_id = RemoteAttachmentId::new(external_group_profile.object_id);
        let url = api_client
            .ds_get_attachment_url(
                StorageObjectType::GroupProfile,
                &context.key_store.signing_key,
                group.attachment_target(),
                remote_attachment_id,
            )
            .await?;
        let bytes = context
            .http_client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;

        // Decrypt and validate group profile
        let group_profile = GroupProfile::decrypt(
            &group.identity_link_wrapper_key,
            &external_group_profile,
            bytes.into(),
        )
        .map_err(JobError::fatal)?;

        debug!(
            ?group_id,
            ?external_group_profile,
            "Fetched and decrypted group profile"
        );

        // Update chat attributes and store new messages
        context
            .db
            .write()
            .await?
            .with_transaction(async |txn| -> anyhow::Result<()> {
                let new_picture = group_profile.picture.map(|p| p.into());

                if is_initial_fetch {
                    // => no system messages
                    chat.set_title(&mut *txn, group_profile.title).await?;
                    chat.set_picture(&mut *txn, new_picture).await?;
                } else {
                    let mut messages = Vec::new();
                    let chat_attributes = ChatAttributes::new(group_profile.title, new_picture);
                    update_chat_attributes(
                        &mut *txn,
                        &mut chat,
                        &sender_id,
                        chat_attributes,
                        uploaded_at,
                        &mut messages,
                    )
                    .await?;
                    CoreUser::store_new_messages(txn, chat.id(), messages).await?;
                }

                debug!(?group_id, chat_id = %chat.id(), "Updated chat attributes");

                Ok(())
            })
            .await?;

        Ok(())
    }
}
