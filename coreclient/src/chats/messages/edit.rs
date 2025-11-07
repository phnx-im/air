// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{identifiers::MimiId, time::TimeStamp};
use mimi_content::MimiContent;

use crate::MessageId;

pub(crate) struct MessageEdit<'a> {
    mimi_id: &'a MimiId,
    message_id: MessageId,
    created_at: TimeStamp,
    mimi_content: &'a MimiContent,
}

impl<'a> MessageEdit<'a> {
    pub(crate) fn new(
        mimi_id: &'a MimiId,
        message_id: MessageId,
        created_at: TimeStamp,
        mimi_content: &'a MimiContent,
    ) -> Self {
        Self {
            mimi_id,
            message_id,
            created_at,
            mimi_content,
        }
    }
}

mod persistence {
    use aircommon::codec::BlobEncoded;
    use sqlx::{SqliteExecutor, query, query_scalar};

    use crate::chats::messages::persistence::VersionedMessage;

    use super::*;

    impl MessageEdit<'_> {
        pub(crate) async fn store(&self, executor: impl SqliteExecutor<'_>) -> anyhow::Result<()> {
            let versioned_message =
                BlobEncoded(VersionedMessage::from_mimi_content(self.mimi_content)?);
            query!(
                "INSERT INTO message_edit (
                    mimi_id,
                    message_id,
                    created_at,
                    content
                ) VALUES (?, ?, ?, ?)",
                self.mimi_id,
                self.message_id,
                self.created_at,
                versioned_message,
            )
            .execute(executor)
            .await?;
            Ok(())
        }

        pub(crate) async fn find_message_id(
            executor: impl SqliteExecutor<'_>,
            mimi_id: &MimiId,
        ) -> sqlx::Result<Option<MessageId>> {
            query_scalar!(
                r#"SELECT
                    message_id AS "message_id: _"
                FROM message_edit
                WHERE mimi_id = ?"#,
                mimi_id,
            )
            .fetch_optional(executor)
            .await
        }
    }
}
