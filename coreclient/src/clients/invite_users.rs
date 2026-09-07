// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::UserId;

use crate::{ChatId, ChatMessage, job::add_members::AddMembers};

use super::CoreUser;

/// Result of inviting users to a chat.
#[derive(Debug)]
pub struct InviteUsersResult {
    /// Messages that represent the changes to the group. Note that these
    /// messages have already been persisted.
    pub messages: Vec<ChatMessage>,
    /// Users that were not added because their client is not compatible
    /// with the group.
    pub users_not_added: Vec<UserId>,
}

impl CoreUser {
    /// Invite users to an existing chat.
    ///
    /// Users whose key material is not compatible with the group are left
    /// out of the invite and reported in
    /// [`InviteUsersResult::users_not_added`].
    ///
    /// Since this function causes the creation of an MLS commit, it can cause
    /// more than one effect on the group. As a result, the returned
    /// [`InviteUsersResult`] contains a vector of [`ChatMessage`]s that
    /// represents the changes to the group.
    pub async fn invite_users(
        &self,
        chat_id: ChatId,
        invited_users: &[UserId],
    ) -> anyhow::Result<InviteUsersResult> {
        let job = AddMembers::new(chat_id, invited_users.to_vec());
        let output = self.execute_job(job).await?;
        Ok(InviteUsersResult {
            messages: output.messages,
            users_not_added: output.users_not_added,
        })
    }
}
