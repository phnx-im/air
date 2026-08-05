// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Cubit backing the Linked Devices screen.
//!
//! The list is a join: the self group's members decide which devices are
//! linked, the synchronized `LinkedDevicesSetting` supplies their names and
//! dates.

use std::sync::Arc;

use aircoreclient::{
    LinkedDevicesSetting, UserSetting,
    clients::CoreUser,
    db::notification::{DbEntityId, DbNotification},
};
pub use airprotos::client::self_group::LinkedDevicePlatform;
use chrono::{DateTime, TimeZone, Utc};
use flutter_rust_bridge::frb;
use tokio::sync::watch;
use tokio_stream::{Stream, StreamExt};
use tracing::error;
use uuid::Uuid;

use crate::{
    StreamSink,
    api::user_cubit::UserCubitBase,
    util::{Cubit, CubitCore, spawn_from_sync},
};

#[doc(hidden)]
#[frb(mirror(LinkedDevicePlatform))]
pub enum _LinkedDevicePlatform {
    Unknown,
    Android,
    Ios,
    Macos,
    Windows,
    Linux,
}

/// One row in the Linked Devices screen.
///
/// An empty `name` means the device has no metadata entry yet. The Dart side
/// substitutes a localized fallback, since localization lives in Flutter.
#[derive(Debug, Clone, PartialEq, Eq)]
#[frb(dart_metadata = ("freezed"))]
pub struct UiLinkedDevice {
    pub client_id: Uuid,
    pub name: String,
    pub platform: LinkedDevicePlatform,
    pub linked_at: Option<DateTime<Utc>>,
    pub is_this_device: bool,
}

#[derive(Debug, Default, Clone)]
#[frb(dart_metadata = ("freezed"))]
pub struct LinkedDevicesState {
    pub devices: Vec<UiLinkedDevice>,
}

#[frb(opaque)]
pub struct LinkedDevicesCubitBase {
    core: CubitCore<LinkedDevicesState>,
    core_user: CoreUser,
}

impl LinkedDevicesCubitBase {
    #[frb(sync)]
    pub fn new(user_cubit: &UserCubitBase) -> Self {
        let core_user = user_cubit.core_user().clone();

        let notifications = core_user.db_notifications();
        let core = CubitCore::new();

        spawn_from_sync(core.cancellation_token().clone().run_until_cancelled_owned(
            devices_listener(core_user.clone(), notifications, core.state_tx().clone()),
        ));

        Self { core, core_user }
    }

    pub fn close(&self) {
        self.core.close();
    }

    #[frb(getter, sync)]
    pub fn is_closed(&self) -> bool {
        self.core.is_closed()
    }

    #[frb(getter, sync)]
    pub fn state(&self) -> LinkedDevicesState {
        self.core.state()
    }

    pub async fn stream(&self, sink: StreamSink<LinkedDevicesState>) {
        self.core.stream(sink).await;
    }

    pub async fn rename_device(&self, client_id: Uuid, name: String) -> anyhow::Result<()> {
        self.core_user.rename_device(client_id, name).await?;
        load_and_emit(&self.core_user, self.core.state_tx()).await;
        Ok(())
    }

    pub async fn unlink_device(&self, client_id: Uuid) -> anyhow::Result<()> {
        self.core_user.unlink_device(client_id).await?;
        load_and_emit(&self.core_user, self.core.state_tx()).await;
        Ok(())
    }
}

/// Reloads the device list whenever the synchronized setting changes.
///
/// Loads once before the loop, so the screen is populated without waiting for a
/// notification.
async fn devices_listener(
    core_user: CoreUser,
    mut notifications: impl Stream<Item = Arc<DbNotification>> + Send + Unpin + 'static,
    state_tx: watch::Sender<LinkedDevicesState>,
) {
    load_and_emit(&core_user, &state_tx).await;

    let mut self_chat_id = None;

    while let Some(notification) = notifications.next().await {
        for entity_id in notification.ops.keys() {
            let updated = match entity_id {
                DbEntityId::UserSetting(key) if key == LinkedDevicesSetting::KEY => true,
                DbEntityId::Chat(chat_id) => {
                    // The self chat id never changes once set.
                    match self_chat_id {
                        Some(self_chat_id) => *chat_id == self_chat_id,
                        None => {
                            self_chat_id = core_user.self_chat_id().await.ok().flatten();
                            self_chat_id.as_ref().is_some_and(|id| id == chat_id)
                        }
                    }
                }
                _ => false,
            };
            if updated {
                load_and_emit(&core_user, &state_tx).await;
                break;
            }
        }
    }
}

async fn load_and_emit(core_user: &CoreUser, state_tx: &watch::Sender<LinkedDevicesState>) {
    match try_load(core_user).await {
        Ok(devices) => {
            state_tx.send_if_modified(|state| {
                let modified = state.devices != devices;
                state.devices = devices;
                modified
            });
        }
        Err(error) => error!(%error, "failed to load the linked devices"),
    }
}

async fn try_load(core_user: &CoreUser) -> anyhow::Result<Vec<UiLinkedDevice>> {
    let own_client_id = core_user.own_client_id().await?;
    let metadata = core_user.linked_devices().await?;

    let mut self_group_members = core_user.self_group_client_ids().await?;
    if self_group_members.is_empty() {
        // No self group yet: this device is the only one, and it is linked by
        // definition. Without this the screen would render nothing at all.
        self_group_members.push(own_client_id);
    }

    let mut devices: Vec<UiLinkedDevice> = self_group_members
        .iter()
        .map(|client_id| {
            let entry = metadata.iter().find(|entry| &entry.client_id == client_id);
            UiLinkedDevice {
                client_id: *client_id,
                name: entry.map(|entry| entry.name.clone()).unwrap_or_default(),
                platform: entry
                    .map(|entry| entry.platform)
                    .unwrap_or(LinkedDevicePlatform::Unknown),
                linked_at: entry
                    .and_then(|entry| Utc.timestamp_opt(entry.linked_at as i64, 0).single()),
                is_this_device: client_id == &own_client_id,
            }
        })
        .collect();

    // This device first, then oldest link first for a stable order.
    devices.sort_unstable_by_key(|device| (!device.is_this_device, device.linked_at));
    Ok(devices)
}
