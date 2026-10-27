// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

#[cfg(any(test, feature = "test_utils"))]
use aircommon::messages::client_ds_out::SendMessageCollisionTag;
use openmls::group::{GroupEpoch, Member};

use aircommon::{
    credentials::RoomPolicyIdentity,
    identifiers::{QualifiedGroupId, Username},
    messages::connection_package::ConnectionPackageHash,
};
use openmls::prelude::GroupId;
use uuid::Uuid;

use airprotos::client::{
    app_data::GroupAppDataExt,
    group::{EncryptedGroupTitle, GroupData},
};

use aircommon::{
    credentials::keys::{LeafSigningKey, SelfGroupSigningKey, UserSigningKey},
    crypto::{
        aead::keys::{GroupStateEarKey, IdentityLinkWrapperKey},
        indexed_aead::keys::UserProfileKey,
    },
    messages::client_ds_out::CreateGroupParamsOut,
};

use crate::{
    ChatAttributes,
    groups::{NewGroupContext, openmls_provider::AirOpenMlsProvider, self_group::SelfGroup},
    job::{
        chat_operation::{ChatOperation, DerivationEpoch},
        create_chat::CreateChat,
        pending_chat_operation::{PendingChatOperation, test_utils::PendingChatOperationInfo},
    },
    key_stores::indexed_keys::StorableIndexedKey,
    outbound_service::{
        account_deletion::AccountDeletion,
        error::is_ds_not_found_error,
        resync::{Resync, ResyncReason, ResyncStatus},
    },
    usernames::{UsernameRecord, connection_packages::ConnectionPackageRecord},
};

use super::*;

impl CoreUser {
    /// The same as [`Self::new()`], except that databases are ephemeral and are
    /// dropped together with this instance of [`CoreUser`].
    pub async fn new_ephemeral(
        user_id: UserId,
        server_url: Url,
        push_token: Option<PushToken>,
        challenge: Option<RegistrationChallenge>,
    ) -> Result<Self> {
        use crate::{
            db::notification::DbNotificationsSender,
            utils::persistence::{open_air_db_in_memory, open_db_in_memory},
        };

        info!(?user_id, "creating new ephemeral user");

        let notifier_tx = DbNotificationsSender::new();

        // Open the air db to store the client record
        let air_db =
            DbAccess::with_single_pool(open_air_db_in_memory().await?, notifier_tx.clone());

        // Open client specific db
        let client_db = DbAccess::with_single_pool(open_db_in_memory().await?, notifier_tx);

        let temp_file = tempfile::NamedTempFile::new()?;
        let global_lock = GlobalLock::from_path(temp_file.path())?;

        Self::new_with_dbs(
            user_id,
            Uuid::new_v4(),
            Some(server_url),
            push_token,
            air_db,
            client_db,
            global_lock,
            challenge,
        )
        .await
    }

    /// Legacy version of chat deletion that removes all other members.
    pub async fn delete_chat_without_tombstone(&self, chat_id: ChatId) -> Result<Vec<ChatMessage>> {
        let job = ChatOperation::delete_chat_without_tombstone(chat_id);
        Ok(self.execute_job(job).await?)
    }

    pub fn qs_user_id(&self) -> aircommon::identifiers::QsUserId {
        self.inner.qs_user_id
    }

    pub fn qs_client_id(&self) -> aircommon::identifiers::QsClientId {
        self.inner.qs_client_id
    }

    /// Creates a QS client record of this user that belongs to no device. An
    /// unlinked device leaves such a record behind.
    pub async fn create_detached_qs_client(
        &self,
        push_token: PushToken,
    ) -> anyhow::Result<QsClientId> {
        use aircommon::crypto::aead::AeadEncryptable;

        let key_store = &self.inner.key_store;
        let encrypted_push_token = push_token.encrypt(&key_store.push_token_ear_key)?;
        let response = self
            .api_client()?
            .qs_create_client(
                self.inner.qs_user_id,
                QsClientSigningKey::generate()?.verifying_key().clone(),
                RatchetDecryptionKey::generate()?.encryption_key().clone(),
                Some(encrypted_push_token),
                RatchetSecret::random()?,
                &key_store.qs_user_signing_key,
            )
            .await?;
        Ok(response.qs_client_id)
    }

    /// Deletes the QS user record without touching anything else.
    pub async fn qs_delete_user(&self) -> Result<(), QsRequestError> {
        self.inner
            .api_clients
            .default_client()
            .map_err(|_| QsRequestError::LibraryError)?
            .qs_delete_user(
                self.inner.qs_user_id,
                &self.inner.key_store.qs_user_signing_key,
            )
            .await
    }

    /// Publishes an empty list of key packages for this client.
    pub async fn qs_publish_empty_key_packages(&self) -> Result<(), QsRequestError> {
        self.inner
            .api_clients
            .default_client()
            .map_err(|_| QsRequestError::LibraryError)?
            .qs_publish_key_packages(
                self.inner.qs_client_id,
                Vec::new(),
                &self.inner.key_store.qs_client_signing_key,
            )
            .await
    }

    /// Creates a fresh QS client under this user.
    pub async fn qs_create_client(
        &self,
    ) -> Result<aircommon::identifiers::QsClientId, QsRequestError> {
        use aircommon::crypto::{
            RatchetDecryptionKey, kdf::keys::RatchetSecret, signatures::keys::QsClientSigningKey,
        };

        let signing_key =
            QsClientSigningKey::generate().map_err(|_| QsRequestError::LibraryError)?;
        let decryption_key =
            RatchetDecryptionKey::generate().map_err(|_| QsRequestError::LibraryError)?;
        let ratchet_secret = RatchetSecret::random().map_err(|_| QsRequestError::LibraryError)?;
        let response = self
            .inner
            .api_clients
            .default_client()
            .map_err(|_| QsRequestError::LibraryError)?
            .qs_create_client(
                self.inner.qs_user_id,
                signing_key.verifying_key().clone(),
                decryption_key.encryption_key().clone(),
                None,
                ratchet_secret,
                &self.inner.key_store.qs_user_signing_key,
            )
            .await?;
        Ok(response.qs_client_id)
    }

    pub async fn self_group(&self) -> anyhow::Result<Option<SelfGroup>> {
        Ok(SelfGroup::load(self.db().read().await?).await?)
    }

    pub async fn self_group_member_count(&self) -> anyhow::Result<Option<usize>> {
        let mut read = self.db().read().await?;
        let Some(group_id) = OwnClientInfo::load(&mut read).await?.self_group_id else {
            return Ok(None);
        };
        let Some(group) = Group::load(read, &group_id).await? else {
            return Ok(None);
        };
        Ok(Some(group.mls_group().members().count()))
    }

    /// Returns whether the user has a persisted self-group that is an APQ (T+PQ) group. `None` if
    /// there is no self-group yet, or it is not persisted.
    pub async fn self_group_is_apq(&self) -> anyhow::Result<Option<bool>> {
        let mut read = self.db().read().await?;
        let own_client_info = OwnClientInfo::load(&mut read).await?;
        let Some(group_id) = own_client_info.self_group_id else {
            return Ok(None);
        };
        let Some(group) = Group::load(read, &group_id).await? else {
            return Ok(None);
        };
        Ok(Some(group.is_apq() && group.pq().is_some()))
    }

    /// Returns the (T, PQ) epochs of the self-group, or `None` if there is no persisted self-group.
    pub async fn self_group_epochs(
        &self,
    ) -> anyhow::Result<Option<(GroupEpoch, Option<GroupEpoch>)>> {
        let mut read = self.db().read().await?;
        let own_client_info = OwnClientInfo::load(&mut read).await?;
        let Some(group_id) = own_client_info.self_group_id else {
            return Ok(None);
        };
        let Some(group) = Group::load(read, &group_id).await? else {
            return Ok(None);
        };
        let t_epoch = group.mls_group().epoch();
        let pq_epoch = group.pq().map(|pq| pq.mls_group.epoch());
        Ok(Some((t_epoch, pq_epoch)))
    }

    /// Whether the self-group is registered as the emulation group of the
    /// virtual client, i.e. whether it holds a derivation epoch.
    pub async fn self_group_has_derivation_epoch(&self) -> anyhow::Result<bool> {
        let Some(group) = self.self_group().await? else {
            return Ok(false);
        };
        self.db()
            .with_write_transaction(async |txn| {
                use openmls_traits::OpenMlsProvider as _;
                let provider = AirOpenMlsProvider::new(txn.as_mut());
                let epoch_id = group
                    .group()
                    .mls_group()
                    .newest_vc_derivation_epoch(provider.storage())?;
                anyhow::Ok(epoch_id.is_some())
            })
            .await
    }

    /// Whether the account deletion this device started was aborted.
    pub async fn account_deletion_is_aborted(&self) -> anyhow::Result<bool> {
        Ok(matches!(
            AccountDeletion::load(self.db().read().await?).await?,
            Some(AccountDeletion::Aborted { .. })
        ))
    }

    /// Deletes the username on the server but keeps its local record, as if
    /// the device stopped before it deleted the record.
    pub async fn delete_username_on_server_only(&self, username: &Username) -> anyhow::Result<()> {
        let record = UsernameRecord::load_all(self.db().read().await?)
            .await?
            .into_iter()
            .find(|record| &record.username == username)
            .context("no username found")?;
        let (token_request, _token_state) = crate::privacy_pass::prepare_delete_token_request(
            self.db().read().await?,
            self.user_id().domain(),
        )
        .await?
        .context("no VOPRF keys available for delete token request")?;
        self.api_client()?
            .as_delete_username(record.hash, &record.signing_key, token_request)
            .await?;
        Ok(())
    }

    /// Returns the pending chat operation info for the self-group, if any.
    pub async fn self_group_pending_operation_info(
        &self,
    ) -> anyhow::Result<Option<PendingChatOperationInfo>> {
        let mut read = self.db().read().await?;
        let own_client_info = OwnClientInfo::load(&mut read).await?;
        let Some(group_id) = own_client_info.self_group_id else {
            return Ok(None);
        };
        PendingChatOperationInfo::load_by_group_id(read, &group_id).await
    }

    /// Corrupts the batch id of the pending self-group upload job (bumps the
    /// generation), simulating a state fork between the job and the commit
    /// the DS accepted.
    pub async fn corrupt_self_group_upload_batch_id(&self) -> anyhow::Result<()> {
        self.db()
            .with_write_transaction(async |txn| {
                let own_client_info = OwnClientInfo::load(&mut *txn).await?;
                let group_id = own_client_info.self_group_id.context("no self-group")?;
                PendingChatOperation::corrupt_self_group_upload_batch_id(txn, &group_id).await
            })
            .await
    }

    /// Returns the raw (plain, APQ) key package refs currently marked as live, sorted for stable
    /// comparison.
    pub async fn live_key_package_refs(&self) -> anyhow::Result<(Vec<Vec<u8>>, Vec<Vec<u8>>)> {
        let mut read = self.db().read().await?;
        let mut plain: Vec<Vec<u8>> =
            sqlx::query_scalar("SELECT key_package_ref FROM key_package_refs WHERE is_live = 1")
                .fetch_all(read.as_mut())
                .await?;
        let mut apq: Vec<Vec<u8>> = sqlx::query_scalar(
            "SELECT key_package_ref FROM apq_key_package_refs WHERE is_live = 1",
        )
        .fetch_all(read.as_mut())
        .await?;
        plain.sort_unstable();
        apq.sort_unstable();
        Ok((plain, apq))
    }

    pub async fn mls_members(&self, chat_id: ChatId) -> Result<Option<Vec<Member>>> {
        Ok(self
            .db()
            .with_read_transaction(async |txn| Group::load_with_chat_id(txn, chat_id).await)
            .await?
            .map(|group| group.mls_group().members().collect()))
    }

    /// The group's epoch and our own leaf index in it.
    ///
    /// Two emulator clients of the same virtual client must agree on both once
    /// one of them has onboarded into the group: they share a single leaf.
    pub async fn group_epoch_and_own_index(&self, chat_id: ChatId) -> Result<Option<(u64, u32)>> {
        Ok(self
            .db()
            .with_read_transaction(async |txn| Group::load_with_chat_id(txn, chat_id).await)
            .await?
            .map(|group| (group.mls_group().epoch().as_u64(), group.own_index().u32())))
    }

    pub async fn group_members(&self, chat_id: ChatId) -> Option<HashSet<UserId>> {
        self.db()
            .with_read_transaction(async |txn| Group::load_with_chat_id(&mut *txn, chat_id).await)
            .await
            .ok()
            .flatten()
            .map(|group| group.members().collect())
    }

    /// Whether the DS still serves the chat's group to an external joiner.
    pub async fn ds_has_group(&self, chat_id: ChatId) -> anyhow::Result<bool> {
        let group = Group::load_with_chat_id(self.db().read().await?, chat_id)
            .await?
            .context("group not found")?;
        let qgid: QualifiedGroupId = group.group_id().try_into()?;
        let result = self
            .api_clients()
            .get(qgid.owning_domain())?
            .ds_external_commit_info(
                group.group_id().clone(),
                group.pq_group_id(),
                group.group_state_ear_key(),
            )
            .await;
        match result {
            Ok(_) => Ok(true),
            Err(error) => {
                let error = anyhow::Error::from(error);
                if is_ds_not_found_error(&error) {
                    Ok(false)
                } else {
                    Err(error)
                }
            }
        }
    }

    /// The users the DS lists in the room state it serves to an external joiner
    /// of this chat's group.
    pub async fn ds_room_state_users(&self, chat_id: ChatId) -> anyhow::Result<HashSet<UserId>> {
        let group = Group::load_with_chat_id(self.db().read().await?, chat_id)
            .await?
            .context("group not found")?;
        let qgid: QualifiedGroupId = group.group_id().try_into()?;
        let external_commit_info = self
            .api_clients()
            .get(qgid.owning_domain())?
            .ds_external_commit_info(
                group.group_id().clone(),
                group.pq_group_id(),
                group.group_state_ear_key(),
            )
            .await?;
        external_commit_info
            .room_state
            .users()
            .keys()
            .map(|identity| match RoomPolicyIdentity::from_bytes(identity)? {
                RoomPolicyIdentity::User(user_id) => Ok(user_id),
                RoomPolicyIdentity::Client(client_id) => Err(anyhow::anyhow!(
                    "unexpected client identity {client_id} in the room state"
                )),
            })
            .collect()
    }

    /// Enqueues a resync with a fabricated group_id that does not exist on the
    /// server. Uses the real group's keys so the request reaches the server and
    /// gets a "not found" response.
    pub async fn enqueue_resync_for_nonexistent_group(
        &self,
        chat_id: ChatId,
        domain: &str,
    ) -> anyhow::Result<()> {
        let group = Group::load_with_chat_id(self.db().read().await?, chat_id)
            .await?
            .context("group not found")?;

        let fake_qgid = QualifiedGroupId::new(Uuid::new_v4(), domain.parse()?);
        let fake_group_id: GroupId = fake_qgid.into();

        let resync = Resync {
            chat_id: Some(chat_id),
            group_id: fake_group_id,
            pq_group_id: group.pq_group_id(),
            group_state_ear_key: group.group_state_ear_key().clone(),
            identity_link_wrapper_key: group.identity_link_wrapper_key().clone(),
            original_leaf_index: group.own_index(),
            shares_vc_leaf: false,
            connection_contact: None,
            outgoing_request: None,
            accepted_friendship_package: None,
            reason: ResyncReason::Manual,
            attempts: 0,
        };
        resync.enqueue(self.db().write().await?).await?;
        Ok(())
    }

    pub async fn resync_status(&self, chat_id: ChatId) -> anyhow::Result<Option<ResyncStatus>> {
        let connection = self.db().read().await?;
        Ok(Resync::status_for_chat(connection, &chat_id).await?)
    }

    /// Whether any setting changes are still waiting to be synchronized.
    pub async fn has_pending_setting_changes(&self) -> anyhow::Result<bool> {
        use crate::clients::user_settings::SettingChanges;
        Ok(SettingChanges::has_pending(self.db().read().await?).await?)
    }

    /// The serialized commit of the chat's pending operation.
    pub async fn pending_commit_message_bytes(&self, chat_id: ChatId) -> anyhow::Result<Vec<u8>> {
        let job = self
            .db()
            .with_read_transaction(async |txn| PendingChatOperation::load(txn, &chat_id).await)
            .await?
            .context("No pending chat operation")?;
        job.staged_commit_message_bytes()
    }

    /// Returns (operation_type, request_status, number_of_attempts) for the
    /// pending chat operation of the given chat, if any.
    pub async fn pending_chat_operation_info(
        &self,
        chat_id: ChatId,
    ) -> anyhow::Result<Option<PendingChatOperationInfo>> {
        self.db()
            .with_read_transaction(async |txn| PendingChatOperationInfo::load(txn, &chat_id).await)
            .await
    }

    /// Stages a group-title-change commit and stores the pending chat operation
    /// WITHOUT merging it, reproducing the window before the inline merge / DS
    /// commit response. Returns the serialized commit the DS would echo back to
    /// us, which can be fed to [`CoreUser::process_incoming_mls_message`] to
    /// drive the `OwnPendingCommit` path.
    pub async fn stage_group_title_commit(
        &self,
        chat_id: ChatId,
        title: String,
    ) -> anyhow::Result<Vec<u8>> {
        let job = self
            .db()
            .with_write_transaction(async |txn| {
                let group = Group::load_with_chat_id(&mut *txn, chat_id)
                    .await?
                    .context("No group")?;
                let encrypted_title =
                    EncryptedGroupTitle::encrypt(&title, group.identity_link_wrapper_key())?;
                let group_data = GroupData {
                    encrypted_title: Some(encrypted_title),
                    external_group_profile: None,
                };
                PendingChatOperation::create_update_with_group_data(
                    txn,
                    self.signing_key(),
                    chat_id,
                    Some(group_data),
                    None,
                    DerivationEpoch::Keep,
                    &self.create_own_client_reference(),
                )
                .await
            })
            .await?;
        job.staged_commit_message_bytes()
    }

    /// Creates a group whose group profile is stored in the group profile component.
    pub async fn create_chat_with_profile_component(
        &self,
        title: String,
        is_apq: bool,
    ) -> anyhow::Result<ChatId> {
        let chat_attributes = ChatAttributes::new(title, None);
        let client_reference = self.create_own_client_reference();
        let job =
            CreateChat::new(chat_attributes, client_reference, is_apq).with_profile_component();
        Ok(self.execute_job(job).await?)
    }

    pub fn signing_key_for_test(&self) -> &UserSigningKey {
        self.signing_key()
    }

    /// Builds the parameters that create a fresh APQ group with the given ids
    /// on the DS. Nothing is kept locally, so the same ids can be built again,
    /// and nothing is sent to the DS.
    pub async fn group_creation_params_for_test(
        &self,
        group_id: GroupId,
        pq_group_id: GroupId,
    ) -> anyhow::Result<(CreateGroupParamsOut, GroupStateEarKey)> {
        let signer = LeafSigningKey::User(self.signing_key().clone());
        self.apq_group_creation_params(group_id, pq_group_id, signer, |key| {
            Ok(NewGroupContext::Chat(GroupData {
                encrypted_title: Some(EncryptedGroupTitle::encrypt("test", key)?),
                external_group_profile: None,
            }))
        })
        .await
    }

    /// Like [`Self::group_creation_params_for_test`], for a self-group.
    pub async fn self_group_creation_params_for_test(
        &self,
        group_id: GroupId,
        pq_group_id: GroupId,
    ) -> anyhow::Result<(CreateGroupParamsOut, GroupStateEarKey)> {
        let client_id = OwnClientInfo::load(self.db().read().await?)
            .await?
            .client_id;
        let signer = LeafSigningKey::SelfGroup(SelfGroupSigningKey::generate(client_id)?);
        let (mut params, ear_key) = self
            .apq_group_creation_params(group_id, pq_group_id, signer, |_| {
                Ok(NewGroupContext::SelfGroup(GroupData::empty()))
            })
            .await?;
        params.creator_user_credential = Some(self.signing_key().credential().clone());
        Ok((params, ear_key))
    }

    async fn apq_group_creation_params(
        &self,
        group_id: GroupId,
        pq_group_id: GroupId,
        signer: LeafSigningKey,
        context: impl FnOnce(&IdentityLinkWrapperKey) -> anyhow::Result<NewGroupContext>,
    ) -> anyhow::Result<(CreateGroupParamsOut, GroupStateEarKey)> {
        let own_user_id = self.user_id().clone();
        let client_reference = self.create_own_client_reference();
        let is_self_group = matches!(signer, LeafSigningKey::SelfGroup(_));
        let identity_link_wrapper_key = IdentityLinkWrapperKey::random()?;
        let context = context(&identity_link_wrapper_key)?;
        self.db()
            .with_write_transaction(async |txn| {
                let (group, partial_params) = Group::create_apq_group(
                    &mut *txn,
                    &signer,
                    own_user_id.clone(),
                    identity_link_wrapper_key,
                    group_id,
                    pq_group_id,
                    context,
                    None,
                    is_self_group.then_some(&client_reference),
                )?;
                let encrypted_user_profile_key = UserProfileKey::load_own(&mut *txn)
                    .await?
                    .encrypt(group.identity_link_wrapper_key(), &own_user_id)?;
                let params =
                    partial_params.into_params(client_reference, encrypted_user_profile_key);
                // Deleting also drops the OpenMLS state, but only of a stored group
                group.store(&mut *txn).await?;
                Group::delete_from_db(txn, group.group_id()).await?;
                Ok((params, group.group_state_ear_key().clone()))
            })
            .await
    }

    /// Whether the group context of the chat carries the group profile component.
    pub async fn has_group_profile_component(&self, chat_id: ChatId) -> anyhow::Result<bool> {
        let Some(group) = self
            .db()
            .with_read_transaction(async |txn| Group::load_with_chat_id(txn, chat_id).await)
            .await?
        else {
            return Ok(false);
        };
        Ok(group.mls_group().extensions().has_group_profile_component())
    }

    pub async fn group_data(&self, chat_id: ChatId) -> anyhow::Result<Option<GroupData>> {
        let Some(group) = self
            .db()
            .with_read_transaction(async |txn| Group::load_with_chat_id(txn, chat_id).await)
            .await?
        else {
            return Ok(None);
        };
        group.group_data()
    }

    /// Send a message to the DS using the given collision tags instead of
    /// auto-derived ones. Used in tests to simulate a second emulator client
    /// sending with the same generation.
    pub async fn send_message_with_fixed_collision_tags(
        &self,
        chat_id: ChatId,
        collision_tags: Vec<SendMessageCollisionTag>,
    ) -> Result<(), airapiclient::ds_api::DsRequestError> {
        use anyhow::Context as _;
        use mimi_content::MimiContent;

        use crate::groups::{Group, openmls_provider::AirOpenMlsProvider};

        let content = MimiContent::simple_markdown_message("collision-test".into(), [0u8; 16]);

        let chat = self
            .db()
            .with_read_transaction(async |conn| crate::Chat::load(conn, &chat_id).await)
            .await
            .expect("db error")
            .expect("chat not found");

        let (group_state_ear_key, params) = self
            .db()
            .with_write_transaction(async |txn| -> anyhow::Result<_> {
                let group_id = chat.group_id();
                let mut group = Group::load_clean(&mut *txn, group_id)
                    .await?
                    .context("group not found")?;
                let provider = AirOpenMlsProvider::new(txn.as_mut());
                group.ensure_collision_key(&provider)?;
                let mut params =
                    group.create_message(&provider, self.signing_key(), content, None)?;
                params.collision_tags = collision_tags;
                Ok((group.group_state_ear_key().clone(), params))
            })
            .await
            .expect("failed to create message");

        let api_client = self.api_client().expect("no api client");
        api_client
            .ds_send_message(params, self.signing_key(), &group_state_ear_key)
            .await?;

        Ok(())
    }

    /// Sends the queued chat message to the DS the way the outbound service
    /// would, but skips the confirmation and all local post-processing,
    /// reproducing a send whose DS response was lost. The message row stays
    /// unsent and enqueued.
    pub async fn send_chat_message_without_confirmation(
        &self,
        message_id: MessageId,
    ) -> anyhow::Result<()> {
        use crate::Message;

        let (chat_id, content) = self
            .db()
            .with_read_transaction(async |txn| -> anyhow::Result<_> {
                let message = ChatMessage::load(&mut *txn, message_id)
                    .await?
                    .context("message not found")?;
                let Message::Content(content) = message.message() else {
                    anyhow::bail!("message {message_id:?} is not a content message");
                };
                Ok((message.chat_id(), content.content().clone()))
            })
            .await?;

        self.send_mimi_content_without_confirmation(chat_id, content)
            .await
    }

    /// Sends `content` into the chat's group without any local bookkeeping,
    /// reproducing a replay of an already delivered message.
    pub async fn send_mimi_content_without_confirmation(
        &self,
        chat_id: ChatId,
        content: mimi_content::MimiContent,
    ) -> anyhow::Result<()> {
        use crate::groups::openmls_provider::AirOpenMlsProvider;

        let chat = self
            .db()
            .with_read_transaction(async |txn| -> anyhow::Result<_> {
                Chat::load(&mut *txn, &chat_id)
                    .await?
                    .context("chat not found")
            })
            .await?;

        let signer = OwnClientInfo::signer_for_group(
            self.db().read().await?,
            chat.group_id(),
            self.signing_key(),
        )
        .await?;

        let (group_state_ear_key, params) = self
            .db()
            .with_write_transaction(async |txn| -> anyhow::Result<_> {
                let group_id = chat.group_id();
                let mut group = Group::load_clean(&mut *txn, group_id)
                    .await?
                    .context("group not found")?;
                let provider = AirOpenMlsProvider::new(txn.as_mut());
                let params = group.create_message(&provider, &signer, content, None)?;
                Ok((group.group_state_ear_key().clone(), params))
            })
            .await?;

        self.api_clients()
            .get(&chat.owner_domain())?
            .ds_send_message(params, &signer, &group_state_ear_key)
            .await?;

        Ok(())
    }

    /// Deletes the connection package keys of `username` to simulate a device
    /// still waiting for them.
    pub async fn forget_connection_package_keys(&self, username: &Username) -> anyhow::Result<()> {
        let keys =
            ConnectionPackageRecord::load_keys_for_username(self.db().read().await?, username)
                .await?;
        let mut connection = self.db().write().await?;
        for (record, _) in keys {
            ConnectionPackageRecord::delete(&mut connection, &record.hash).await?;
        }
        Ok(())
    }

    /// When `username` was last refreshed, as far as this device knows.
    pub async fn username_refreshed_at(
        &self,
        username: &Username,
    ) -> anyhow::Result<Option<DateTime<Utc>>> {
        let record =
            UsernameRecord::load_with_timestamps(self.db().read().await?, username).await?;
        Ok(record.map(|(_, _, refreshed_at)| refreshed_at))
    }

    /// Sets the last refresh time of `username` to simulate a missed refresh by
    /// a sibling.
    pub async fn set_username_refreshed_at(
        &self,
        username: &Username,
        refreshed_at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let (record, _, _) =
            UsernameRecord::load_with_timestamps(self.db().read().await?, username)
                .await?
                .context("no such username")?;
        UsernameRecord::update_refreshed_at(self.db().write().await?, &record.hash, refreshed_at)
            .await?;
        Ok(())
    }

    /// The hashes of the connection packages of `username` whose decryption
    /// keys this device holds.
    pub async fn connection_package_hashes(
        &self,
        username: &Username,
    ) -> anyhow::Result<Vec<ConnectionPackageHash>> {
        let keys =
            ConnectionPackageRecord::load_keys_for_username(self.db().read().await?, username)
                .await?;
        Ok(keys.into_iter().map(|(record, _)| record.hash).collect())
    }
}
