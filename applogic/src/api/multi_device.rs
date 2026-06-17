// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Multi-device linking

use aircommon::identifiers::Fqdn;
use aircoreclient::clients::{CoreUser, multi_device::MultiDeviceProvisionStep};
use airprotos::relay_service::v1::LinkingSessionId;
use anyhow::Result;
use qrcode::QrCode;
use tracing::{debug, error};
use url::Url;

use crate::{StreamSink, api::user_cubit::UserCubitBase};

const LINKING_URL_SCHEME: &str = "air";
const LINKING_URL_PATH: &str = "multiDeviceLinkingCode";
const LINKING_URL_SESSION_ID: &str = "sessionId";

/// Builds the QR-code URL that a fresh device displays for an existing device to scan.
fn multi_device_linking_url(domain: &Fqdn, session_id: &LinkingSessionId) -> String {
    format!(
        "{LINKING_URL_SCHEME}://{domain}/{LINKING_URL_PATH}?{LINKING_URL_SESSION_ID}={session_id}"
    )
}

/// Extracts the linking code from a QR payload produced by [`multi_device_linking_url`], or returns
/// `None` if `url` is not a linking URL targeting `fqdn`.
pub(crate) fn linking_code_from_url(fqdn: &Fqdn, url: &str) -> Option<String> {
    let url = Url::parse(url).ok()?;
    if url.scheme() != LINKING_URL_SCHEME {
        debug!(%url, "wrong scheme, skipping URL");
        return None;
    }

    if !url.host().is_some_and(|host| fqdn.is_host(host.to_owned())) {
        debug!(%url, "wrong host, skipping URL");
        return None;
    }

    if url.path().trim_start_matches('/') != LINKING_URL_PATH {
        debug!(%url, "wrong path, skipping URL");
        return None;
    }

    let session_id = url
        .query_pairs()
        .find_map(|(key, value)| (key == LINKING_URL_SESSION_ID).then(|| value.into_owned()))?;
    if session_id.is_empty() {
        debug!(%url, "empty session ID, skipping URL");
        None
    } else {
        Some(session_id)
    }
}

/// An event emitted while a fresh device provisions itself against an existing account.
///
/// The new device opens a linking session with the relay, which hands back a numeric `code`. The
/// user types (or scans) that code on their existing device. Once the existing device connects,
/// the session completes with [`MultiDeviceProvisionEvent::Linked`].
pub enum MultiDeviceProvisionEvent {
    /// The relay assigned a linking code. Display it so the user can enter it on their existing
    /// device.
    Code {
        qrcode_svg: Option<String>,
        code: String,
    },
    /// The existing device has established the session and the linking process is underway.
    Linking,
    /// The existing device connected and linking completed successfully.
    Linked(String),
    /// The session ended without linking (e.g. it timed out or the connection dropped).
    Failed(String),
}

/// Runs a multi-device provisioning session on a fresh device.
///
/// Connects to the relay at `domain`, emits the linking [`MultiDeviceProvisionEvent::Code`] as soon
/// as the relay assigns it, and keeps the session alive until the existing device links or the
/// session fails.
pub async fn multi_device_provision_client(
    domain: String,
    sink: StreamSink<MultiDeviceProvisionEvent>,
) -> Result<()> {
    let domain: Fqdn = domain.parse()?;
    let (session_tx, mut session_rx) = tokio::sync::mpsc::channel::<MultiDeviceProvisionStep>(1);

    let forward_code = async {
        while let Some(msg) = session_rx.recv().await {
            match msg {
                MultiDeviceProvisionStep::SessionId(session_id) => {
                    let qrcode_svg = QrCode::new(multi_device_linking_url(&domain, &session_id))
                        .map(|code| {
                            use qrcode::render::svg;
                            code.render::<svg::Color>()
                                .min_dimensions(200, 200)
                                .dark_color(svg::Color("#000000"))
                                .light_color(svg::Color("#FFFFFF"))
                                .quiet_zone(false)
                                .build()
                        })
                        .ok();

                    if let Err(error) = sink.add(MultiDeviceProvisionEvent::Code {
                        code: session_id.to_string(),
                        qrcode_svg,
                    }) {
                        error!(%error, "failed to forward MultiDeviceProvisionEvent to the Dart side");
                    }
                }
                MultiDeviceProvisionStep::Linking => {
                    if let Err(error) = sink.add(MultiDeviceProvisionEvent::Linking) {
                        error!(%error, "failed to forward MultiDeviceProvisionEvent to the Dart side");
                    }
                }
            }
        }
    };

    let linking_session = async {
        match CoreUser::multi_device_provision_client(&domain, session_tx).await {
            Ok(answer) => {
                if let Err(error) = sink.add(MultiDeviceProvisionEvent::Linked(answer)) {
                    error!(%error, "failed to forward MultiDeviceProvisionEvent to the Dart side");
                }
            }
            Err(error) => {
                if let Err(error) = sink.add(MultiDeviceProvisionEvent::Failed(error.to_string())) {
                    error!(%error, "failed to forward MultiDeviceProvisionEvent to the Dart side");
                }
            }
        }
    };

    tokio::join!(forward_code, linking_session);

    Ok(())
}

/// Drives the acceptor (existing-device) side of multi-device linking.
///
/// `session_id` is the linking code that the existing device scanned from (or typed in from) the
/// fresh device. Connects to the relay, admits the new device into a fresh MLS group and completes
/// the handshake.
pub async fn multi_device_link_client(
    user_cubit: &UserCubitBase,
    session_id: String,
) -> Result<String> {
    let session_id = LinkingSessionId { value: session_id };
    user_cubit
        .core_user()
        .multi_device_link_client(session_id)
        .await
}
