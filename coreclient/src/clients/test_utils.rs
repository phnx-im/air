// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
use openmls::group::Member;

use crate::outbound_service::timed_tasks_queue::{TaskKind, TimedTaskQueue};

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
        use crate::utils::persistence::open_db_in_memory;

        info!(?user_id, "creating new ephemeral user");

        // Open the air db to store the client record
        let air_db = open_db_in_memory().await?;

        // Open client specific db
        let client_db = open_db_in_memory().await?;

        let lock_path = std::env::temp_dir().join(format!(
            "air_lock_ephemeral_{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let global_lock = GlobalLock::from_path(lock_path)?;

        Self::new_with_connections(
            user_id,
            server_url,
            push_token,
            air_db,
            client_db,
            global_lock,
            invitation_code,
        )
        .await
    }

    pub async fn mls_members(&self, chat_id: ChatId) -> Result<Option<Vec<Member>>> {
        let mut connection = self.pool().acquire().await?;
        let Some(chat_id) = Chat::load(&mut connection, &chat_id).await? else {
            return Ok(None);
        };
        let Some(group) = Group::load(&mut connection, chat_id.group_id()).await? else {
            return Ok(None);
        };
        let members = group.mls_group().members().collect();
        Ok(Some(members))
    }

    pub async fn group_members(&self, chat_id: ChatId) -> Option<HashSet<UserId>> {
        let mut connection = self.pool().acquire().await.ok()?;
        let chat = Chat::load(&mut connection, &chat_id).await.ok()??;
        let group = Group::load(&mut connection, chat.group_id()).await.ok()??;
        Some(group.members(&mut *connection).await.into_iter().collect())
    }

    pub async fn schedule_key_package_upload(
        &self,
        due_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        let mut connection = self.pool().acquire().await?;
        let task_kind = TaskKind::KeyPackageUpload;
        TimedTaskQueue::set_due_date(&mut *connection, task_kind, due_at).await?;
        Ok(())
    }
}
