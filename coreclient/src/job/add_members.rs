// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashSet;

use aircommon::identifiers::UserId;

use crate::{
    ChatId, ChatMessage,
    job::{
        Job, JobContext, JobError,
        chat_operation::{ChatOperationError, execute_pending_operation, load_active_group},
        pending_chat_operation::PendingChatOperation,
    },
};

/// Adds users to an existing chat.
pub(crate) struct AddMembers {
    chat_id: ChatId,
    users: Vec<UserId>,
}

pub(crate) struct AddMembersOutput {
    /// Messages that represent the changes to the group.
    pub(crate) messages: Vec<ChatMessage>,
    /// Users that were left out of the commit because their key material is
    /// not compatible with the group.
    pub(crate) users_not_added: Vec<UserId>,
}

impl AddMembers {
    pub(crate) fn new(chat_id: ChatId, users: Vec<UserId>) -> Self {
        Self { chat_id, users }
    }

    /// Drops users that are already members.
    async fn refine(&mut self, context: &mut JobContext<'_, '_>) -> anyhow::Result<()> {
        let group = load_active_group(&mut context.db, self.chat_id).await?;
        let members: HashSet<_> = group.members().collect();
        self.users.retain(|user_id| !members.contains(user_id));
        Ok(())
    }
}

impl Job for AddMembers {
    type Output = AddMembersOutput;

    type DomainError = ChatOperationError;

    async fn execute_logic(
        mut self,
        context: &mut JobContext<'_, '_>,
    ) -> Result<AddMembersOutput, JobError<Self::DomainError>> {
        // The group state may have changed since the users were picked, either
        // due to a PendingChatOperation executed as a dependency, or one or
        // more commits arriving from the QS.
        self.refine(context).await?;

        let Self { chat_id, users } = self;
        if users.is_empty() {
            return Ok(AddMembersOutput {
                messages: Vec::new(),
                users_not_added: Vec::new(),
            });
        }

        let JobContext {
            api_clients,
            db,
            key_store,
            ..
        } = context;
        let (job, users_not_added) = Box::pin(PendingChatOperation::create_add(
            db.write().await?,
            api_clients,
            &key_store.signing_key,
            chat_id,
            users,
        ))
        .await?;

        // No commit is staged when every user turned out to be incompatible.
        let messages = match job {
            Some(job) => job.execute(context).await?,
            None => Vec::new(),
        };

        Ok(AddMembersOutput {
            messages,
            users_not_added,
        })
    }

    async fn execute_dependencies(
        &mut self,
        context: &mut JobContext<'_, '_>,
    ) -> Result<(), JobError<Self::DomainError>> {
        execute_pending_operation(self.chat_id, context).await
    }
}
