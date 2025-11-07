// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Multi-platform client application logic

#![warn(clippy::large_futures)]

pub(crate) use frb_generated::*;

pub mod api;
pub mod background_execution;

#[allow(clippy::uninlined_format_args)]
pub(crate) mod frb_generated;
pub(crate) mod logging;
pub(crate) mod mark_as_read;
pub(crate) mod message_content;
pub(crate) mod messages;
pub(crate) mod notifications;
pub(crate) mod util;

#[cfg(test)]
fn init_test_tracing() {
    use tracing::Level;
    use tracing_subscriber::EnvFilter;

    let _ = tracing_subscriber::fmt::fmt()
        .with_test_writer()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(Level::INFO.into())
                .from_env_lossy(),
        )
        .try_init();
}
