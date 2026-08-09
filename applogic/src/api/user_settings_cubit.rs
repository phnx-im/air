// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;

use aircoreclient::{
    DeveloperModeSetting, ExperimentalFeaturesSetting, ReadReceiptsSetting, UserSetting,
    clients::CoreUser,
    db::notification::{DbEntityId, DbNotification},
};
use anyhow::{anyhow, bail};
use flutter_rust_bridge::frb;
use tokio::sync::watch;
use tokio_stream::{Stream, StreamExt};
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::{
    StreamSink,
    api::user::User,
    util::{Cubit, CubitCore, spawn_from_sync},
};

#[derive(Debug, Clone)]
#[frb(dart_metadata = ("freezed"))]
pub struct UserSettings {
    pub locale: Option<String>,
    pub interface_scale: Option<f64>,
    #[frb(default = 240.0)]
    pub sidebar_width: f64,
    #[frb(default = false)]
    pub send_on_enter: bool,
    #[frb(default = true)]
    pub read_receipts: bool,
    /// Whether the developer surface is unlocked on this device.
    #[frb(default = false)]
    pub developer_mode: bool,
    /// Whether experimental features are on. Only in effect while
    /// `developer_mode` is.
    #[frb(default = false)]
    pub experimental_features: bool,
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
            sidebar_width: 240.0,
            send_on_enter: false,
            read_receipts: true,
            developer_mode: false,
            experimental_features: false,
            default_emoji_skin_tone: 0,
        }
    }
}

/// Loads the persisted user settings. Missing rows keep the defaults.
pub async fn load_user_settings(user: &User) -> UserSettings {
    let core_user = &user.user;

    let locale = core_user.user_setting().await;
    let interface_scale = core_user.user_setting().await;
    let sidebar_width = core_user.user_setting().await;
    let send_on_enter = core_user.user_setting().await;
    let read_receipts = core_user.user_setting().await;
    let developer_mode = core_user.user_setting().await;
    let experimental_features = core_user.user_setting().await;
    let default_emoji_skin_tone = core_user.user_setting().await;

    let defaults = UserSettings::default();
    UserSettings {
        locale: locale.map(|LocaleSetting(value)| value),
        interface_scale: interface_scale.map(|InterfaceScaleSetting(value)| value),
        sidebar_width: sidebar_width
            .map_or(defaults.sidebar_width, |SidebarWidthSetting(value)| value),
        send_on_enter: send_on_enter
            .map_or(defaults.send_on_enter, |SendOnEnterSetting(value)| value),
        read_receipts: read_receipts
            .map_or(defaults.read_receipts, |ReadReceiptsSetting(value)| value),
        developer_mode: developer_mode
            .map_or(defaults.developer_mode, |DeveloperModeSetting(value)| value),
        experimental_features: experimental_features.map_or(
            defaults.experimental_features,
            |ExperimentalFeaturesSetting(value)| value,
        ),
        default_emoji_skin_tone: default_emoji_skin_tone.map_or(
            defaults.default_emoji_skin_tone,
            |DefaultEmojiSkinToneSetting(value)| value,
        ),
    }
}

#[frb(opaque)]
pub struct UserSettingsCubitBase {
    core: CubitCore<UserSettings>,
    core_user: CoreUser,
}

impl UserSettingsCubitBase {
    /// Creates a cubit for `user` starting from the `initial` settings.
    ///
    /// Callers load `initial` with [`load_user_settings`]. Synced settings are
    /// re-read in the background, see [`settings_listener`].
    #[frb(sync)]
    pub fn new(user: &User, initial: UserSettings) -> Self {
        let core_user = user.user.clone();

        // `db_notifications` observes every notification sent after this call,
        // and the underlying broadcast channel buffers them until the listener
        // task first polls the stream. A change applied while the task does its
        // initial re-read is therefore delivered rather than lost.
        let notifications = core_user.db_notifications();

        let core = CubitCore::with_initial_state(initial);

        spawn_from_sync(settings_listener(
            core_user.clone(),
            notifications,
            core.state_tx().clone(),
            core.cancellation_token().clone(),
        ));

        Self { core, core_user }
    }

    // Cubit interface

    pub fn close(&self) {
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

    pub async fn set_locale(&self, value: String) -> anyhow::Result<()> {
        if self.core.state_tx().borrow().locale.as_deref() == Some(value.as_str()) {
            return Ok(());
        }
        self.core_user
            .set_user_setting(&LocaleSetting(value.clone()))
            .await?;
        self.core
            .state_tx()
            .send_modify(|state| state.locale = Some(value));
        Ok(())
    }

    pub async fn set_interface_scale(&self, value: f64) -> anyhow::Result<()> {
        if self.core.state_tx().borrow().interface_scale == Some(value) {
            return Ok(());
        }
        self.core_user
            .set_user_setting(&InterfaceScaleSetting(value))
            .await?;
        self.core
            .state_tx()
            .send_modify(|state| state.interface_scale = Some(value));
        Ok(())
    }

    pub async fn set_sidebar_width(&self, value: f64) -> anyhow::Result<()> {
        if self.core.state_tx().borrow().sidebar_width == value {
            return Ok(());
        }
        self.core_user
            .set_user_setting(&SidebarWidthSetting(value))
            .await?;
        self.core
            .state_tx()
            .send_modify(|state| state.sidebar_width = value);
        Ok(())
    }

    pub async fn set_send_on_enter(&self, value: bool) -> anyhow::Result<()> {
        if self.core.state_tx().borrow().send_on_enter == value {
            return Ok(());
        }
        self.core_user
            .set_user_setting(&SendOnEnterSetting(value))
            .await?;
        self.core
            .state_tx()
            .send_modify(|state| state.send_on_enter = value);
        Ok(())
    }

    pub async fn set_read_receipts(&self, value: bool) -> anyhow::Result<()> {
        if self.core.state_tx().borrow().read_receipts == value {
            return Ok(());
        }
        self.core_user
            .set_synced_user_setting(&ReadReceiptsSetting(value))
            .await?;
        self.core
            .state_tx()
            .send_modify(|state| state.read_receipts = value);
        Ok(())
    }

    pub async fn set_developer_mode(&self, value: bool) -> anyhow::Result<()> {
        if self.core.state_tx().borrow().developer_mode == value {
            return Ok(());
        }
        self.core_user
            .set_user_setting(&DeveloperModeSetting(value))
            .await?;
        self.core
            .state_tx()
            .send_modify(|state| state.developer_mode = value);
        Ok(())
    }

    pub async fn set_experimental_features(&self, value: bool) -> anyhow::Result<()> {
        if self.core.state_tx().borrow().experimental_features == value {
            return Ok(());
        }
        self.core_user
            .set_user_setting(&ExperimentalFeaturesSetting(value))
            .await?;
        self.core
            .state_tx()
            .send_modify(|state| state.experimental_features = value);
        Ok(())
    }

    pub async fn set_default_emoji_skin_tone(&self, value: u8) -> anyhow::Result<()> {
        if self.core.state_tx().borrow().default_emoji_skin_tone == value {
            return Ok(());
        }
        self.core_user
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
/// Such a change comes from a sibling device's update or from a rollback after
/// a failed send. Both emit a `DbEntityId::UserSetting` notification.
///
/// The constructor takes its snapshot before subscribing to notifications, so a
/// change applied in that window is not reported here. Re-reading the synced
/// settings once before the loop closes that gap. Only synced settings can
/// change out of band, so only they need the re-read.
///
/// Runs until cancelled.
async fn settings_listener(
    core_user: CoreUser,
    mut notifications: impl Stream<Item = Arc<DbNotification>> + Send + Unpin + 'static,
    state_tx: watch::Sender<UserSettings>,
    cancel: CancellationToken,
) {
    reload_read_receipts(&core_user, &state_tx).await;

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
                reload_read_receipts(&core_user, &state_tx).await;
            } else {
                debug!(%key, "ignoring notification for unhandled user setting");
            }
        }
    }
}

/// Reads the read receipts setting from the database and emits it if it changed.
///
/// A missing row means a rollback deleted it, so the state falls back to the
/// default (matching the `frb(default)` of `true`).
async fn reload_read_receipts(core_user: &CoreUser, state_tx: &watch::Sender<UserSettings>) {
    let read_receipts = core_user
        .user_setting::<ReadReceiptsSetting>()
        .await
        .is_none_or(|ReadReceiptsSetting(value)| value);
    state_tx.send_if_modified(|state| {
        let modified = state.read_receipts != read_receipts;
        state.read_receipts = read_receipts;
        modified
    });
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
