// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::UserId;

use crate::{ChatId, ChatMessage, job::chat_operation::ChatOperation};

use super::CoreUser;

impl CoreUser {
    /// Invite users to an existing chat.
    ///
    /// Since this function causes the creation of an MLS commit, it can cause
    /// more than one effect on the group. As a result this function returns a
    /// vector of [`ChatMessage`]s that represents the changes to the
    /// group. Note that these returned message have already been persisted.
    pub(crate) async fn invite_users(
        &self,
        chat_id: ChatId,
        invited_users: &[UserId],
    ) -> anyhow::Result<Vec<ChatMessage>> {
        let job = ChatOperation::add_members(chat_id, invited_users.to_vec());
        self.execute_job(job).await
    }
}
