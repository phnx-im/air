// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Background execution on mobile platforms

use serde::{Deserialize, Serialize};

use crate::api::notifications::NotificationContent;

#[cfg(target_os = "android")]
pub mod java_api;

#[cfg(target_os = "ios")]
pub mod swift_api;

#[allow(
    dead_code,
    reason = "used only on Android/iOS but is compiled for all targets"
)]
pub(crate) mod processing;
pub(crate) mod stack;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IncomingNotificationContent {
    title: String,
    body: String,
    data: String,
    path: String,
    log_file_path: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NotificationBatch {
    badge_count: usize,
    removals: Vec<String>,
    additions: Vec<NotificationContent>,
}
