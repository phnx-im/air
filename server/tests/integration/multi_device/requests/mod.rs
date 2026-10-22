// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Connection requests across the devices of a user.

mod expiry;
mod forwarding;
mod onboarding;
mod retract_decline;

use aircoreclient::{ChatId, SystemMessage, clients::CoreUser};
use airserver_test_harness::utils::core_user_ext::CoreUserExt;
use chrono::{DateTime, Utc};

/// Whether the chat holds a system message matching `predicate`.
async fn has_system_message(
    device: &CoreUser,
    chat_id: ChatId,
    predicate: impl Fn(&SystemMessage) -> bool,
) -> bool {
    device.system_messages(chat_id).await.iter().any(predicate)
}

async fn request_expires_at(device: &CoreUser, chat_id: ChatId) -> Option<DateTime<Utc>> {
    let chat = device.expect_chat(chat_id).await;
    device.contact_request_expires_at(&chat).await.unwrap()
}
