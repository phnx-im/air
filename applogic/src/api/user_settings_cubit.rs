// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;

use aircoreclient::{
    IsDeveloperSetting, ReadReceiptsSetting, UserSetting,
    clients::CoreUser,
    db::notification::{DbEntityId, DbNotification},
};
use anyhow::{anyhow, bail};
use flutter_rust_bridge::frb;
use tokio::sync::watch;
use tokio_stream::{Stream, StreamExt};
use tokio_util::sync::{CancellationToken, DropGuard};
use tracing::debug;

use crate::{
    StreamSink,
    api::{user::User, user_cubit::UserCubitBase},
    util::{Cubit, CubitCore, spawn_from_sync},
};

#[derive(Debug, Clone)]
#[frb(dart_metadata = ("freezed"))]
pub struct UserSettings {
    pub locale: Option<String>,
    pub interface_scale: Option<f64>,
    #[frb(default = 300.0)]
    pub sidebar_width: f64,
    #[frb(default = false)]
    pub send_on_enter: bool,
    #[frb(default = true)]
    pub read_receipts: bool,
    #[frb(default = false)]
    pub is_developer: bool,
    /// Index into the client `EmojiSkinTone` enum (0 = default/none).
    #[frb(default = 0)]
    pub default_emoji_skin_tone: u8,
}

impl Default for UserSettings {
    #[frb(ignore)]
    fn default() -> Self {
        Self {
            locale: None,
            interface_scale: None,
            sidebar_width: 300.0,
            send_on_enter: false,
            read_receipts: true,
            is_developer: false,
            default_emoji_skin_tone: 0,
        }
    }
}

#[frb(opaque)]
pub struct UserSettingsCubitBase {
    core: CubitCore<UserSettings>,
    /// Cancels the running db-notification listener when dropped or replaced.
    ///
    /// A new listener is spawned on every `load_state`, so the guard is
    /// replaced there. It is cleared on `reset` and `close`.
    listener: std::sync::Mutex<Option<DropGuard>>,
}

impl UserSettingsCubitBase {
    #[frb(sync)]
    pub fn new() -> Self {
        Self {
            core: CubitCore::new(),
            listener: std::sync::Mutex::new(None),
        }
    }

    // Cubit interface

    pub fn close(&self) {
        *self.listener.lock().expect("poisoned listener lock") = None;
        self.core.close();
    }

    #[frb(getter, sync)]
    pub fn is_closed(&self) -> bool {
        self.core.is_closed()
    }

    #[frb(getter, sync)]
    pub fn state(&self) -> UserSettings {
        self.core.state()
    }

    pub async fn stream(&self, sink: StreamSink<UserSettings>) {
        self.core.stream(sink).await;
    }

    // Cubit methods

    pub async fn reset(&self) {
        *self.listener.lock().expect("poisoned listener lock") = None;
        self.core
            .state_tx()
            .send_modify(|state| *state = Default::default());
    }

    pub async fn load_state(&self, user: &User) {
        let core_user = &user.user;

        // Subscribe before the initial reads. `db_notifications` observes every
        // notification sent after this call, and the underlying broadcast
        // channel buffers them until the listener task first polls the stream.
        // Subscribing first means an update applied between the reads below and
        // the task starting is delivered rather than lost.
        let notifications = core_user.db_notifications();

        let locale = core_user.user_setting().await;
        let interface_scale = core_user.user_setting().await;
        let sidebar_width = core_user.user_setting().await;
        let send_on_enter = core_user.user_setting().await;
        let read_receipts = core_user.user_setting().await;
        let is_developer = core_user.user_setting().await;
        let default_emoji_skin_tone = core_user.user_setting().await;
        self.core.state_tx().send_modify(|state| {
            state.locale = locale.map(|LocaleSetting(value)| value);
            state.interface_scale = interface_scale.map(|InterfaceScaleSetting(value)| value);
            if let Some(SidebarWidthSetting(value)) = sidebar_width {
                state.sidebar_width = value;
            }
            if let Some(SendOnEnterSetting(value)) = send_on_enter {
                state.send_on_enter = value;
            }
            if let Some(ReadReceiptsSetting(value)) = read_receipts {
                state.read_receipts = value;
            }
            if let Some(IsDeveloperSetting(value)) = is_developer {
                state.is_developer = value;
            }
            if let Some(DefaultEmojiSkinToneSetting(value)) = default_emoji_skin_tone {
                state.default_emoji_skin_tone = value;
            }
        });

        // Listen for synced settings that change out of band: a sibling
        // device's update or a rollback after a failed send. Both emit a
        // `DbEntityId::UserSetting` notification.
        let cancel = CancellationToken::new();
        spawn_from_sync(settings_listener(
            core_user.clone(),
            notifications,
            self.core.state_tx().clone(),
            cancel.clone(),
        ));
        // Replacing the guard cancels any listener from a previous load.
        *self.listener.lock().expect("poisoned listener lock") = Some(cancel.drop_guard());
    }

    pub async fn set_locale(&self, user: &User, value: String) -> anyhow::Result<()> {
        if self.core.state_tx().borrow().locale.as_deref() == Some(value.as_str()) {
            return Ok(());
        }
        user.user
            .set_user_setting(&LocaleSetting(value.clone()))
            .await?;
        self.core
            .state_tx()
            .send_modify(|state| state.locale = Some(value));
        Ok(())
    }

    pub async fn set_interface_scale(
        &self,
        user_cubit: &UserCubitBase,
        value: f64,
    ) -> anyhow::Result<()> {
        if self.core.state_tx().borrow().interface_scale == Some(value) {
            return Ok(());
        }
        user_cubit
            .core_user()
            .set_user_setting(&InterfaceScaleSetting(value))
            .await?;
        self.core
            .state_tx()
            .send_modify(|state| state.interface_scale = Some(value));
        Ok(())
    }

    pub async fn set_sidebar_width(
        &self,
        user_cubit: &UserCubitBase,
        value: f64,
    ) -> anyhow::Result<()> {
        if self.core.state_tx().borrow().sidebar_width == value {
            return Ok(());
        }
        user_cubit
            .core_user()
            .set_user_setting(&SidebarWidthSetting(value))
            .await?;
        self.core
            .state_tx()
            .send_modify(|state| state.sidebar_width = value);
        Ok(())
    }

    pub async fn set_send_on_enter(
        &self,
        user_cubit: &UserCubitBase,
        value: bool,
    ) -> anyhow::Result<()> {
        if self.core.state_tx().borrow().send_on_enter == value {
            return Ok(());
        }
        user_cubit
            .core_user()
            .set_user_setting(&SendOnEnterSetting(value))
            .await?;
        self.core
            .state_tx()
            .send_modify(|state| state.send_on_enter = value);
        Ok(())
    }

    pub async fn set_read_receipts(
        &self,
        user_cubit: &UserCubitBase,
        value: bool,
    ) -> anyhow::Result<()> {
        if self.core.state_tx().borrow().read_receipts == value {
            return Ok(());
        }
        user_cubit
            .core_user()
            .set_synced_user_setting(&ReadReceiptsSetting(value))
            .await?;
        self.core
            .state_tx()
            .send_modify(|state| state.read_receipts = value);
        Ok(())
    }

    pub async fn set_is_developer(
        &self,
        user_cubit: Option<UserCubitBase>,
        value: bool,
    ) -> anyhow::Result<()> {
        if self.core.state_tx().borrow().is_developer == value {
            return Ok(());
        }
        if let Some(user_cubit) = user_cubit {
            user_cubit
                .core_user()
                .set_user_setting(&IsDeveloperSetting(value))
                .await?;
        }
        self.core
            .state_tx()
            .send_modify(|state| state.is_developer = value);
        Ok(())
    }

    pub async fn set_default_emoji_skin_tone(
        &self,
        user_cubit: &UserCubitBase,
        value: u8,
    ) -> anyhow::Result<()> {
        if self.core.state_tx().borrow().default_emoji_skin_tone == value {
            return Ok(());
        }
        user_cubit
            .core_user()
            .set_user_setting(&DefaultEmojiSkinToneSetting(value))
            .await?;
        self.core
            .state_tx()
            .send_modify(|state| state.default_emoji_skin_tone = value);
        Ok(())
    }

    pub(crate) fn subscribe(&self) -> watch::Receiver<UserSettings> {
        self.core.state_tx().subscribe()
    }
}

/// Reloads synced settings into the cubit state when they change out of band.
///
/// Runs until cancelled. On each `DbEntityId::UserSetting` notification it
/// reloads the affected setting from the database. A missing row means a
/// rollback deleted it, so the state falls back to the default.
///
/// The notification stream is established by the caller before it takes the
/// initial reads, so a change applied between those reads and this task
/// starting is buffered by the stream and delivered here rather than lost.
async fn settings_listener(
    core_user: CoreUser,
    mut notifications: impl Stream<Item = Arc<DbNotification>> + Send + Unpin + 'static,
    state_tx: watch::Sender<UserSettings>,
    cancel: CancellationToken,
) {
    loop {
        let notification = tokio::select! {
            _ = cancel.cancelled() => return,
            notification = notifications.next() => match notification {
                Some(notification) => notification,
                None => return,
            },
        };

        for entity_id in notification.ops.keys() {
            let DbEntityId::UserSetting(key) = entity_id else {
                continue;
            };
            if key == ReadReceiptsSetting::KEY {
                // `None` means the row was deleted by a rollback, so fall back
                // to the default (matching the `frb(default)` of `true`).
                let read_receipts = core_user
                    .user_setting::<ReadReceiptsSetting>()
                    .await
                    .is_none_or(|ReadReceiptsSetting(value)| value);
                state_tx.send_if_modified(|state| {
                    let modified = state.read_receipts != read_receipts;
                    state.read_receipts = read_receipts;
                    modified
                });
            } else {
                debug!(%key, "ignoring notification for unhandled user setting");
            }
        }
    }
}

struct DefaultEmojiSkinToneSetting(u8);

impl UserSetting for DefaultEmojiSkinToneSetting {
    const KEY: &'static str = "default_emoji_skin_tone";

    fn encode(&self) -> anyhow::Result<Vec<u8>> {
        Ok(vec![self.0])
    }

    fn decode(bytes: Vec<u8>) -> anyhow::Result<Self> {
        match bytes.as_slice() {
            [byte] => Ok(Self(*byte)),
            _ => bail!("invalid default_emoji_skin_tone bytes"),
        }
    }
}

struct InterfaceScaleSetting(f64);

impl UserSetting for InterfaceScaleSetting {
    const KEY: &'static str = "interface_scale";

    fn encode(&self) -> anyhow::Result<Vec<u8>> {
        f64_encode(&self.0)
    }

    fn decode(bytes: Vec<u8>) -> anyhow::Result<Self> {
        f64_decode(bytes).map(Self)
    }
}

struct SidebarWidthSetting(f64);

impl UserSetting for SidebarWidthSetting {
    const KEY: &'static str = "sidebar_width";

    fn encode(&self) -> anyhow::Result<Vec<u8>> {
        f64_encode(&self.0)
    }

    fn decode(bytes: Vec<u8>) -> anyhow::Result<Self> {
        f64_decode(bytes).map(Self)
    }
}

fn f64_encode(f64: &f64) -> anyhow::Result<Vec<u8>> {
    Ok(f64.to_le_bytes().to_vec())
}

fn f64_decode(bytes: Vec<u8>) -> anyhow::Result<f64> {
    Ok(f64::from_le_bytes(
        bytes.try_into().map_err(|_| anyhow!("invalid f64 bytes"))?,
    ))
}

struct LocaleSetting(String);

impl UserSetting for LocaleSetting {
    const KEY: &'static str = "locale";

    fn encode(&self) -> anyhow::Result<Vec<u8>> {
        Ok(self.0.as_bytes().to_vec())
    }

    fn decode(bytes: Vec<u8>) -> anyhow::Result<Self> {
        let value =
            String::from_utf8(bytes).map_err(|error| anyhow!("invalid locale bytes: {error}"))?;
        Ok(Self(value))
    }
}

struct SendOnEnterSetting(bool);

impl UserSetting for SendOnEnterSetting {
    const KEY: &'static str = "send_on_enter";

    fn encode(&self) -> anyhow::Result<Vec<u8>> {
        Ok(vec![self.0 as u8])
    }

    fn decode(bytes: Vec<u8>) -> anyhow::Result<Self> {
        match bytes.as_slice() {
            [byte] => Ok(Self(*byte != 0)),
            _ => bail!("invalid send_on_enter bytes"),
        }
    }
}
