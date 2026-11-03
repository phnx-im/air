// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Messaging: sending, editing, deleting, reactions, receipts, attachments and
//! queue processing.

pub(crate) mod attachments;
mod collision_tags;
mod deletion;
mod edit;
mod processing;
mod reactions;
mod receipts;
mod send;
mod system_messages;

use aircoreclient::{MessageId, clients::CoreUser};
use airserver_test_harness::utils::core_user_ext::CoreUserExt;
use mimi_content::MimiContent;

/// The mimi content currently stored for the message.
async fn stored_content(user: &CoreUser, message_id: MessageId) -> MimiContent {
    user.expect_message(message_id)
        .await
        .message()
        .mimi_content()
        .unwrap()
        .clone()
}
