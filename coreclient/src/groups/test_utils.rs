// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::codec::PersistenceCodec;
use anyhow::Context as _;
use chrono::{DateTime, Utc};

use crate::{Chat, ChatId, clients::CoreUser};

impl CoreUser {
    /// Replaces the payload of the chat group's persisted MLS state with
    /// bytes that fail to decode, reproducing a group whose stored state we
    /// can no longer read, e.g. after a serialization format change.
    ///
    /// The codec version byte is kept, so the corruption survives a codec
    /// version bump.
    pub async fn corrupt_mls_group_state(&self, chat_id: ChatId) -> anyhow::Result<()> {
        let group_id = self
            .db()
            .with_read_transaction(async |txn| Chat::load(txn, &chat_id).await)
            .await?
            .with_context(|| format!("Can't find chat with id {chat_id}"))?
            .group_id()
            .clone();
        // The MLS storage provider keys its rows by the encoded group id.
        let group_id = PersistenceCodec::to_vec(&group_id)?;

        let mut connection = self.db().write().await?;
        // Concatenation yields TEXT, so the result is cast back to a blob to keep the
        // failure in the codec rather than in the column type.
        sqlx::query(
            "UPDATE group_data
            SET group_data = CAST(substr(group_data, 1, 1) || x'FF' AS BLOB)
            WHERE group_id = ?1",
        )
        .bind(group_id)
        .execute(connection.as_mut())
        .await?;

        Ok(())
    }

    pub async fn self_updated_at(&self, chat_id: ChatId) -> sqlx::Result<Option<DateTime<Utc>>> {
        Chat::self_updated_at(self.db().read().await?, chat_id).await
    }

    pub async fn set_self_updated_at(
        &self,
        chat_id: ChatId,
        self_updated_at: DateTime<Utc>,
    ) -> sqlx::Result<()> {
        Chat::set_self_updated_at(self.db().write().await?, chat_id, self_updated_at).await
    }

    pub async fn pq_self_updated_at(&self, chat_id: ChatId) -> sqlx::Result<Option<DateTime<Utc>>> {
        Chat::pq_self_updated_at(self.db().read().await?, chat_id).await
    }

    pub async fn set_pq_self_updated_at(
        &self,
        chat_id: ChatId,
        self_updated_at: DateTime<Utc>,
    ) -> sqlx::Result<()> {
        Chat::set_pq_self_updated_at(self.db().write().await?, chat_id, self_updated_at).await
    }
}
