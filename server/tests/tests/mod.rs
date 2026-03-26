// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

mod attachment;
mod connection;
mod group;
mod jobs;
mod message;
mod process;
mod server;
mod user;

#[cfg(test)]
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
