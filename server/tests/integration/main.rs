// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Integration tests of clients against a test backend, grouped by feature
//! area.

mod account;
mod api;
mod contacts;
mod groups;
mod messaging;
mod multi_device;

#[cfg(test)]
#[allow(dead_code)]
fn init_test_logging() {
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
