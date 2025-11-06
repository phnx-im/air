// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircoreclient::store::{Store, UserSetting};
use anyhow::{anyhow, bail};
use flutter_rust_bridge::frb;
use tokio::sync::watch;

use crate::{
    StreamSink,
    api::{user::User, user_cubit::UserCubitBase},
    util::{Cubit, CubitCore},
};

#[derive(Debug, Clone)]
#[frb(dart_metadata = ("freezed"))]
pub struct UserSettings {
    pub interface_scale: Option<f64>,
    #[frb(default = 300.0)]
    pub sidebar_width: f64,
    #[frb(default = false)]
    pub send_on_enter: bool,
    #[frb(default = true)]
    pub read_receipts: bool,
}

impl Default for UserSettings {
    #[frb(ignore)]
    fn default() -> Self {
        Self {
            interface_scale: None,
            sidebar_width: 300.0,
            send_on_enter: false,
            read_receipts: true,
        }
    }
}

#[frb(opaque)]
pub struct UserSettingsCubitBase {
    core: CubitCore<UserSettings>,
}

impl UserSettingsCubitBase {
    #[frb(sync)]
    pub fn new() -> Self {
        Self {
            core: CubitCore::new(),
        }
    }

    // Cubit interface

    pub fn close(&mut self) {
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

    pub async fn stream(&mut self, sink: StreamSink<UserSettings>) {
        self.core.stream(sink).await;
    }

    // Cubit methods

    pub async fn reset(&self) {
        self.core
            .state_tx()
            .send_modify(|state| *state = Default::default());
    }

    pub async fn load_state(&self, user: &User) {
        let store = &user.user;
        let interface_scale = store.user_setting().await;
        let sidebar_width = store.user_setting().await;
        let send_on_enter = store.user_setting().await;
        let read_receipts = store.user_setting().await;
        self.core.state_tx().send_modify(|state| {
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
        });
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
            .set_user_setting(&ReadReceiptsSetting(value))
            .await?;
        self.core
            .state_tx()
            .send_modify(|state| state.read_receipts = value);
        Ok(())
    }

    pub(crate) fn subscribe(&self) -> watch::Receiver<UserSettings> {
        self.core.state_tx().subscribe()
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

struct ReadReceiptsSetting(bool);

impl UserSetting for ReadReceiptsSetting {
    const KEY: &'static str = "read_receipts";

    fn encode(&self) -> anyhow::Result<Vec<u8>> {
        Ok(vec![self.0 as u8])
    }

    fn decode(bytes: Vec<u8>) -> anyhow::Result<Self> {
        match bytes.as_slice() {
            [byte] => Ok(Self(*byte != 0)),
            _ => bail!("invalid read_receipts bytes"),
        }
    }
}
