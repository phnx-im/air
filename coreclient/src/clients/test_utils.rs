// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

#[cfg(any(test, feature = "test_utils"))]
use aircommon::messages::client_ds_out::SendMessageCollisionTag;
use openmls::group::Member;

use aircommon::{codec::PersistenceCodec, identifiers::QualifiedGroupId};
use openmls::prelude::GroupId;
use uuid::Uuid;

use airprotos::client::{component::AirComponent, group::GroupData};

use crate::{
    chats::GroupDataExt,
    groups::{GroupDataBytes, self_group::SelfGroup},
    job::pending_chat_operation::{PendingChatOperation, test_utils::PendingChatOperationInfo},
    outbound_service::resync::Resync,
};

use super::*;

impl CoreUser {
    /// The same as [`Self::new()`], except that databases are ephemeral and are
    /// dropped together with this instance of [`CoreUser`].
    pub async fn new_ephemeral(
        user_id: UserId,
        server_url: Url,
        push_token: Option<PushToken>,
        invitation_code: String,
    ) -> Result<Self> {
        use crate::{
            db::notification::DbNotificationsSender, utils::persistence::open_db_in_memory,
        };

        info!(?user_id, "creating new ephemeral user");

        let notifier_tx = DbNotificationsSender::new();

        // Open the air db to store the client record
        let air_db = DbAccess::with_single_pool(open_db_in_memory().await?, notifier_tx.clone());

        // Open client specific db
        let client_db = DbAccess::with_single_pool(open_db_in_memory().await?, notifier_tx);

        let temp_file = tempfile::NamedTempFile::new()?;
        let global_lock = GlobalLock::from_path(temp_file.path())?;

        Self::new_with_connections(
            user_id,
            Some(server_url),
            push_token,
            air_db,
            client_db,
            global_lock,
            invitation_code,
        )
        .await
    }

    pub fn qs_user_id(&self) -> aircommon::identifiers::QsUserId {
        self.inner.qs_user_id
    }

    pub fn qs_client_id(&self) -> aircommon::identifiers::QsClientId {
        self.inner.qs_client_id
    }

    pub async fn self_group(&self) -> anyhow::Result<Option<SelfGroup>> {
        Ok(SelfGroup::load(self.db().read().await?).await?)
    }

    pub async fn self_chat_title(&self) -> anyhow::Result<Option<String>> {
        let Some(group) = self.self_group().await? else {
            return Ok(None);
        };
        let chat_id = crate::ChatId::try_from(group.group_id())?;
        let chat = self
            .db()
            .with_read_transaction(async |txn| crate::Chat::load(txn, &chat_id).await)
            .await?;
        Ok(chat.and_then(|chat| chat.attributes().map(|attrs| attrs.title().to_owned())))
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

    pub async fn mls_members(&self, chat_id: ChatId) -> Result<Option<Vec<Member>>> {
        Ok(self
            .db()
            .with_read_transaction(async |txn| Group::load_with_chat_id(txn, chat_id).await)
            .await?
            .map(|group| group.mls_group().members().collect()))
    }

    pub async fn group_members(&self, chat_id: ChatId) -> Option<HashSet<UserId>> {
        self.db()
            .with_read_transaction(async |txn| Group::load_with_chat_id(&mut *txn, chat_id).await)
            .await
            .ok()
            .flatten()
            .map(|group| group.members().collect())
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
            chat_id,
            group_id: fake_group_id,
            pq_group_id: group.pq_group_id(),
            group_state_ear_key: group.group_state_ear_key().clone(),
            identity_link_wrapper_key: group.identity_link_wrapper_key().clone(),
            original_leaf_index: group.own_index(),
        };
        resync.enqueue(self.db().write().await?).await?;
        Ok(())
    }

    pub async fn is_resync_pending(&self, chat_id: ChatId) -> anyhow::Result<bool> {
        let connection = self.db().read().await?;
        Ok(Resync::is_pending_for_chat(connection, &chat_id).await?)
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

    /// Set the group title and picture of the given chat in the legacy format.
    ///
    /// Useful for testing migrations of the group data format.
    pub async fn set_legacy_group_data(
        &self,
        chat_id: ChatId,
        title: String,
        picture: Option<Vec<u8>>,
    ) -> anyhow::Result<()> {
        #[derive(serde::Serialize)]
        struct LegacyGroupData {
            title: String,
            picture: Option<Vec<u8>>,
        }

        let legacy_group_data: GroupDataBytes =
            PersistenceCodec::to_vec(&LegacyGroupData { title, picture })?.into();

        let op = self
            .db()
            .with_write_transaction(async |txn| {
                PendingChatOperation::create_update_with_raw_group_data(
                    txn,
                    self.signing_key(),
                    chat_id,
                    Some(legacy_group_data),
                    None,
                )
                .await
            })
            .await?;
        self.execute_job(op).await?;

        Ok(())
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
        let group_data = GroupData {
            encrypted_title: None,
            external_group_profile: None,
            legacy_title: Some(title),
            legacy_picture: None,
        };
        let group_data_bytes = group_data.encode()?;
        let job = self
            .db()
            .with_write_transaction(async |txn| {
                PendingChatOperation::create_update_with_raw_group_data(
                    txn,
                    self.signing_key(),
                    chat_id,
                    Some(group_data_bytes),
                    None,
                )
                .await
            })
            .await?;
        job.staged_commit_message_bytes()
    }

    pub async fn group_data(&self, chat_id: ChatId) -> anyhow::Result<Option<GroupData>> {
        let Some(group) = self
            .db()
            .with_read_transaction(async |txn| Group::load_with_chat_id(txn, chat_id).await)
            .await?
        else {
            return Ok(None);
        };
        let Some(bytes) = group.group_data() else {
            return Ok(None);
        };
        Ok(Some(GroupData::decode(&bytes)?))
    }

    /// Sends a self-update commit that forces the given [`AirComponent`] into the own leaf node.
    ///
    /// Use this in tests to simulate an old client that advertises a different set of feature
    /// flags.
    #[cfg(any(test, feature = "test_utils"))]
    pub async fn set_group_air_component(
        &self,
        chat_id: ChatId,
        air_component: AirComponent,
    ) -> anyhow::Result<()> {
        let op = self
            .db()
            .with_write_transaction(async |txn| {
                PendingChatOperation::create_update_with_air_component(
                    txn,
                    self.signing_key(),
                    chat_id,
                    air_component,
                )
                .await
            })
            .await?;
        self.execute_job(op).await?;
        Ok(())
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
}
