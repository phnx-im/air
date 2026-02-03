// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::UserId;

use crate::{ChatId, ChatMessage, job::chat_operation::ChatOperation};

use super::CoreUser;

impl CoreUser {
    /// Remove users from the chat with the given [`ChatId`].
    ///
    /// Since this function causes the creation of an MLS commit, it can cause
    /// more than one effect on the group. As a result this function returns a
    /// vector of [`ChatMessage`]s that represents the changes to the
    /// group. Note that these returned message have already been persisted.
    pub(crate) async fn remove_users(
        &self,
        chat_id: ChatId,
        target_users: Vec<UserId>,
    ) -> anyhow::Result<Vec<ChatMessage>> {
        let job = ChatOperation::remove_members(chat_id, target_users);
        self.execute_job(job).await
    }
}
