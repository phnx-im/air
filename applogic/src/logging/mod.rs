// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

#[cfg(any(target_os = "android", target_os = "ios"))]
pub(crate) mod dart;

use std::{
    path::Path,
    sync::{Arc, OnceLock},
};

use anyhow::Context;
use tracing::{info, level_filters::LevelFilter};
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, registry};
use tracing_subscriber::{fmt, layer::SubscriberExt};

use crate::util::{FileRingBuffer, FileRingBufferLock};

pub(crate) const LOG_FILE_RING_BUFFER_SIZE: usize = 500 * 1024; // 500 KiB

pub(crate) static LOG_FILE_RING_BUFFER: OnceLock<Arc<FileRingBufferLock>> = OnceLock::new();

pub fn init_logger(log_file: impl AsRef<Path>) -> Arc<FileRingBufferLock> {
    let buffer_path = log_file.as_ref();
    let buffer = LOG_FILE_RING_BUFFER
        .get_or_init(|| init_app_log(buffer_path).expect("failed to init log file"));

    do_init_logger(buffer.clone());
    info!(log_file =% buffer_path.display(), "Rust logging initialized");

    buffer.clone()
}

fn do_init_logger(log_file: Arc<FileRingBufferLock>) {
    let env_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();

    let registry = registry().with(env_filter);

    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        if let Err(error) = registry
            .with(dart::layer())
            .with(fmt::Layer::new().with_writer(log_file))
            .try_init()
        {
            tracing::warn!(%error, "skip logger init; already initialized");
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    {
        use fmt::writer::MakeWriterExt;
        if let Err(error) = registry
            .with(fmt::Layer::new().map_writer(|w| w.and(log_file)))
            .try_init()
        {
            tracing::warn!(%error, "skip logger init; already initialized");
        }
    }

    #[cfg(not(any(
        target_os = "android",
        target_os = "ios",
        target_os = "linux",
        target_os = "macos",
        target_os = "windows"
    )))]
    {
        unimplemented!("logging is not supported on this platform");
    }
}

fn init_app_log(file_path: impl AsRef<Path>) -> anyhow::Result<Arc<FileRingBufferLock>> {
    let file_path = file_path.as_ref();
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let buffer = FileRingBuffer::open(file_path, LOG_FILE_RING_BUFFER_SIZE)
        .with_context(|| format!("failed to open log file at {}", file_path.display()))?;

    Ok(Arc::new(FileRingBufferLock::new(buffer)))
}
