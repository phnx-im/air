// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

#[cfg(any(target_os = "android", target_os = "ios"))]
pub(crate) mod dart;

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU8, Ordering},
    },
};

use anyhow::Context;
use tracing::{info, level_filters::LevelFilter};
use tracing_subscriber::{EnvFilter, registry};
use tracing_subscriber::{fmt, layer::SubscriberExt};
use tracing_subscriber::{
    fmt::{MakeWriter, writer::OptionalWriter},
    util::SubscriberInitExt,
};

use crate::util::{FileRingBuffer, FileRingBufferLock};

pub(crate) const LOG_FILE_RING_BUFFER_SIZE: usize = 4 * 1024 * 1024; // 4 MiB

static APP_LOG: OnceLock<LogFile> = OnceLock::new();
static BACKGROUND_LOG: OnceLock<LogFile> = OnceLock::new();

/// Kind of the log file the tracing subscriber writes to, or `NO_LOG`.
static CURRENT_LOG: AtomicU8 = AtomicU8::new(NO_LOG);
const NO_LOG: u8 = 0;

struct LogFile {
    path: PathBuf,
    buffer: Arc<FileRingBufferLock>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum LogKind {
    App = 1,
    Background = 2,
}

impl LogKind {
    fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::App),
            2 => Some(Self::Background),
            _ => None,
        }
    }

    fn slot(self) -> &'static OnceLock<LogFile> {
        match self {
            Self::App => &APP_LOG,
            Self::Background => &BACKGROUND_LOG,
        }
    }
}

/// Opens the log file of the given kind and makes it the current one.
///
/// The app log always becomes current. The background log only becomes current if no log file is
/// current yet.
pub(crate) fn init_logger(path: impl AsRef<Path>, kind: LogKind) -> Arc<FileRingBufferLock> {
    let log_file = kind.slot().get_or_init(|| {
        let path = path.as_ref().to_path_buf();
        let buffer = init_app_log(&path).expect("failed to init log file");
        LogFile { path, buffer }
    });

    let prev = match kind {
        LogKind::App => CURRENT_LOG.swap(kind as u8, Ordering::AcqRel),
        LogKind::Background => {
            let (Ok(prev) | Err(prev)) = CURRENT_LOG.compare_exchange(
                NO_LOG,
                kind as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
            prev
        }
    };

    match LogKind::from_u8(prev) {
        None => {
            do_init_logger();
            info!(log_file =% log_file.path.display(), "Rust logging initialized");
        }
        Some(LogKind::Background) if kind == LogKind::App => {
            if let Some(from) = BACKGROUND_LOG.get() {
                info!(
                    from =% from.path.display(),
                    to =% log_file.path.display(),
                    "Switched log file"
                );
            }
        }
        Some(_) => {}
    }

    log_file.buffer.clone()
}

fn current_log() -> Option<&'static LogFile> {
    LogKind::from_u8(CURRENT_LOG.load(Ordering::Acquire))?
        .slot()
        .get()
}

/// Writes to the current log file.
struct CurrentLog;

impl MakeWriter<'_> for CurrentLog {
    type Writer = OptionalWriter<&'static FileRingBufferLock>;

    fn make_writer(&self) -> Self::Writer {
        current_log().map(|log_file| &*log_file.buffer).into()
    }
}

fn do_init_logger() {
    let env_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();

    let registry = registry().with(env_filter);

    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        if let Err(error) = registry
            .with(dart::layer())
            .with(fmt::Layer::new().with_writer(CurrentLog))
            .try_init()
        {
            tracing::warn!(%error, "skip logger init; already initialized");
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    {
        use fmt::writer::MakeWriterExt;
        if let Err(error) = registry
            .with(fmt::Layer::new().map_writer(|w| w.and(CurrentLog)))
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

pub(crate) fn app_log_buffer() -> Option<Arc<FileRingBufferLock>> {
    APP_LOG.get().map(|log_file| log_file.buffer.clone())
}

pub(crate) fn background_log_buffer() -> Option<Arc<FileRingBufferLock>> {
    BACKGROUND_LOG.get().map(|log_file| log_file.buffer.clone())
}
