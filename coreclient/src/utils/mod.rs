// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

pub(crate) mod connection_ext;
pub(crate) mod data_migrations;
pub(crate) mod image;
pub(crate) mod persistence;

#[cfg(test)]
pub(crate) fn init_test_tracing() {
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
