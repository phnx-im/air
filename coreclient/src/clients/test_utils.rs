// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
use openmls::group::Member;

use aircommon::identifiers::QualifiedGroupId;
use openmls::prelude::GroupId;
use uuid::Uuid;

use crate::{
    job::pending_chat_operation::test_utils::PendingChatOperationInfo,
    outbound_service::resync::Resync,
};

use super::*;

impl CoreUser {
    /// The same as [`Self::new()`], except that databases are ephemeral and are
    /// dropped together with this instance of [`CoreUser`].
    #[cfg(any(test, feature = "test_utils"))]
    pub async fn new_ephemeral(
        user_id: UserId,
        server_url: Url,
        push_token: Option<PushToken>,
        invitation_code: String,
    ) -> Result<Self> {
        use crate::{store::StoreNotificationsSender, utils::persistence::open_db_in_memory};

        info!(?user_id, "creating new ephemeral user");

        let notifier_tx = StoreNotificationsSender::new();

        // Open the air db to store the client record
        let air_db = DbAccess::new(open_db_in_memory().await?, notifier_tx.clone());

        // Open client specific db
        let client_db = DbAccess::new(open_db_in_memory().await?, notifier_tx);

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

    pub async fn mls_members(&self, chat_id: ChatId) -> Result<Option<Vec<Member>>> {
        let group = self
            .db()
            .with_read_transaction(async |txn| match Chat::load(&mut *txn, &chat_id).await? {
                Some(chat) => Group::load(&mut *txn, chat.group_id()).await,
                None => Ok(None),
            })
            .await?;
        Ok(group.map(|group| group.mls_group().members().collect()))
    }

    pub async fn group_members(&self, chat_id: ChatId) -> Option<HashSet<UserId>> {
        self.db()
            .with_read_transaction(async |txn| match Chat::load(&mut *txn, &chat_id).await? {
                Some(chat) => Group::load(&mut *txn, chat.group_id()).await,
                None => Ok(None),
            })
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
}
