// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::HashMap, path::Path};

use tracing::{error, info};
use zbus::{
    zvariant::Value,
    {Connection, Proxy},
};

use crate::notifications::NotificationContent;
use crate::notifications::NotificationId;

const PORTAL_DEST: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";

#[derive(Debug, Clone)]
pub(super) enum LinuxNotifier {
    /// org.freedesktop.portal.Notification, used whenever a portal is available
    Portal { proxy: Proxy<'static>, version: u32 },
    /// org.freedesktop.Notifications.Notify, fallback without a usable portal
    Direct { proxy: Proxy<'static> },
}

impl LinuxNotifier {
    /// Detects the notification backend, `None` without a session bus.
    ///
    /// Behavior for different environments:
    /// - Flatpak: portal (no registration)
    /// - deb/rpm: register the app ID, then portal, else direct
    /// - no portal: direct
    pub(super) async fn new() -> Option<Self> {
        let connection = Connection::session()
            .await
            .inspect_err(|error| error!(%error, "failed to connect to D-Bus"))
            .ok()?;
        let in_flatpak = Path::new("/.flatpak-info").exists();
        let use_portal = in_flatpak
            || Self::register(&connection)
                .await
                .inspect_err(|error| info!(%error, "portal registry unavailable"))
                .is_ok();
        if use_portal && let Some(portal) = Self::portal(&connection).await {
            Some(portal)
        } else {
            Self::direct(&connection).await
        }
    }

    async fn register(connection: &Connection) -> Result<(), zbus::Error> {
        let proxy = Proxy::new(
            connection,
            PORTAL_DEST,
            PORTAL_PATH,
            "org.freedesktop.host.portal.Registry",
        )
        .await?;
        proxy
            .call_method("Register", &("ms.air.Air", HashMap::<&str, Value>::new()))
            .await?;
        Ok(())
    }

    async fn portal(connection: &Connection) -> Option<Self> {
        let proxy = Proxy::new(
            connection,
            PORTAL_DEST,
            PORTAL_PATH,
            "org.freedesktop.portal.Notification",
        )
        .await
        .ok()?;
        let version = proxy
            .get_property::<u32>("version")
            .await
            .inspect_err(|error| info!(%error, "portal notifications unavailable"))
            .ok()?;
        info!(version, "using portal notifications");
        Some(Self::Portal { proxy, version })
    }

    async fn direct(connection: &Connection) -> Option<Self> {
        let proxy = Proxy::new(
            connection,
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
        )
        .await
        .ok()?;
        info!("using direct notifications");
        Some(Self::Direct { proxy })
    }

    pub(super) async fn show(&self, notification: NotificationContent) -> anyhow::Result<()> {
        match self {
            Self::Portal { proxy, version } => show_portal(proxy, *version, notification).await,
            Self::Direct { proxy } => show_direct(proxy, notification).await,
        }
    }
}

async fn show_portal(
    proxy: &Proxy<'static>,
    portal_version: u32,
    NotificationContent {
        title,
        body,
        identifier,
        ..
    }: NotificationContent,
) -> anyhow::Result<()> {
    let mut notification = HashMap::<&str, Value>::new();
    notification.insert("title", title.into());
    notification.insert("body", body.into());
    if portal_version >= 2 {
        notification.insert("category", "im.received".into());
    }

    let NotificationId(id) = identifier;
    proxy
        .call_method("AddNotification", &(id.to_string(), notification))
        .await?;

    Ok(())
}

// Version 4.x of `notify-rust` does not set the `sender-pid` hint, which is required for GNOME
// 46+ compatibility. Doing it manually also lets us enable notifications grouping per chat.
async fn show_direct(
    proxy: &Proxy<'static>,
    NotificationContent {
        chat_id,
        title,
        body,
        ..
    }: NotificationContent,
) -> anyhow::Result<()> {
    let mut hints: HashMap<&str, Value> = HashMap::new();
    // for GNOME 46+ compatibility
    hints.insert("sender-pid", std::process::id().into());
    hints.insert("x-gnome-stack-group", format!("air-chat-{chat_id}").into());
    hints.insert("desktop-entry", "ms.air.Air".into());

    proxy
        .call_method(
            "Notify",
            &(
                "Air",              // app_name
                0u32,               // replaces_id
                "ms.air.Air",       // icon
                title,              // summary
                body,               // body
                Vec::<&str>::new(), // actions
                hints,
                -1i32, // timeout (-1 = default)
            ),
        )
        .await?;

    Ok(())
}
