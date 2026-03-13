// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Facilities for sending logs to the Dart side

use std::{
    io::{self, BufRead, Write},
    path::Path,
    sync::{Arc, LazyLock},
};

use anyhow::Context;
use anyhow::bail;
use bytes::Buf;
use chrono::{DateTime, Utc};
use flate2::{Compression, write::GzEncoder};
use flutter_rust_bridge::frb;
use regex::Regex;

use crate::{
    StreamSink,
    logging::{LOG_FILE_RING_BUFFER, LOG_FILE_RING_BUFFER_SIZE, init_logger},
    util::{FileRingBuffer, FileRingBufferLock},
};

/// Initializes the Rust logging system
///
/// The logs are sent to Flutter on Android and iOS, and are written to standard error output on
/// Linux/macOS/Windows. The logs are also written to a file specified by the provided `file_path`.
/// The file has a fixed size and is used as a ring buffer.
///
/// The returned [`LogWriter`] can be used to write logs to the file from the Flutter side.
#[frb(sync)]
pub fn init_rust_logging(log_file: String) -> LogWriter {
    let buffer = init_logger(log_file);
    LogWriter { buffer }
}

/// Reads the application logs from the file currently used for writing logs (if any).
pub fn read_app_logs() -> anyhow::Result<String> {
    let buffer = LOG_FILE_RING_BUFFER
        .get()
        .context("No application buffer found")?;
    read_logs_from_buffer(&buffer.lock())
}

/// Clears all application logs.
///
/// The file is truncated to zero length, but is is kept open.
pub fn clear_app_logs() -> anyhow::Result<()> {
    let Some(buffer) = LOG_FILE_RING_BUFFER.get() else {
        bail!("No application buffer found");
    };
    buffer.lock().clear();
    Ok(())
}

/// Reads the background logs from the file: `<cache_dir>/background.log`.
pub fn read_background_logs(cache_dir: String) -> anyhow::Result<String> {
    let buffer = open_background_logs_file(cache_dir)?;
    read_logs_from_buffer(&buffer)
}

/// Clears the background logs at `<cache_dir>/background.log`.
///
/// The file is truncated to zero length, but is not deleted.
pub fn clear_background_logs(cache_dir: String) -> anyhow::Result<()> {
    open_background_logs_file(cache_dir)?.clear();
    Ok(())
}

/// Creates a Zlib compressed tar archive of the logs
pub fn tar_logs(cache_dir: String) -> anyhow::Result<Vec<u8>> {
    tar_logs_impl(
        LOG_FILE_RING_BUFFER
            .get()
            .context("No application buffer found")?,
        || open_background_logs_file(cache_dir),
    )
}

fn tar_logs_impl(
    app_buffer: &Arc<FileRingBufferLock>,
    background_buffer: impl FnOnce() -> anyhow::Result<FileRingBuffer>,
) -> anyhow::Result<Vec<u8>> {
    let mut data = Vec::with_capacity(2 * LOG_FILE_RING_BUFFER_SIZE);
    let enc = GzEncoder::new(&mut data, Compression::default());
    let mut tar = tar::Builder::new(enc);

    let mut buffer = Vec::with_capacity(LOG_FILE_RING_BUFFER_SIZE);

    let mut append_data = |path: &str, reader: &mut dyn io::BufRead| {
        buffer.clear();

        reader.read_to_end(&mut buffer)?;
        // remove invalid UTF-8 sequences: we could have some because of circular buffer
        let content = String::from_utf8_lossy(&buffer);
        // remove leading and trailing null bytes (in case the buffer is not full)
        let content = content.trim_matches('\0');

        let mut header = tar::Header::new_gnu();
        header.set_size(content.len().try_into().expect("usize overflow"));
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, path, content.as_bytes())
    };

    append_data("logs/app.log", &mut app_buffer.lock().buf().reader())?;
    append_data(
        "logs/background.log",
        &mut background_buffer()?.buf().reader(),
    )?;

    tar.finish()?;
    drop(tar);

    Ok(data)
}

fn open_background_logs_file(cache_dir: String) -> anyhow::Result<FileRingBuffer> {
    let log_file_path = Path::new(&cache_dir).join("background.log");
    Ok(FileRingBuffer::open(
        log_file_path,
        LOG_FILE_RING_BUFFER_SIZE,
    )?)
}

// Note: this function is not memory-allocations optimized.
fn read_logs_from_buffer(buffer: &FileRingBuffer) -> anyhow::Result<String> {
    static ANSI_ESCAPE_SEQUENCE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\x1B\[[0-9;]*[mK]").unwrap());
    let mut lines = buffer
        .buf()
        .reader()
        .split(b'\n')
        .filter_map(|line| {
            let line = line.ok()?;
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_matches('\0');
            let line = ANSI_ESCAPE_SEQUENCE_RE.replace_all(line, "").into_owned();
            Some(line)
        })
        .collect::<Vec<_>>();
    lines.reverse();
    Ok(lines.join("\n"))
}

/// Writes the given log entry to the file initialized by [`init_rust_logging`].
#[frb(opaque)]
pub struct LogWriter {
    buffer: Arc<FileRingBufferLock>,
}

impl LogWriter {
    /// Writes the given log entry with a newline.
    #[frb]
    pub fn write_line(&self, message: &str) -> io::Result<()> {
        let mut buffer = self.buffer.lock();
        writeln!(buffer, "{message}")?;
        Ok(())
    }
}

/// A log entry sent to the Dart side
pub struct LogEntry {
    /// The timestamp of the log entry
    pub time: DateTime<Utc>,
    /// The log level
    pub level: LogEntryLevel,
    /// The target of the log entry (module path)
    pub target: String,
    /// The log message
    ///
    /// Structured data is attached to the end of the message as formatted key-value pairs.
    pub msg: String,
}

/// The log level
pub enum LogEntryLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl From<tracing::Level> for LogEntryLevel {
    fn from(level: tracing::Level) -> Self {
        match level {
            tracing::Level::TRACE => LogEntryLevel::Trace,
            tracing::Level::DEBUG => LogEntryLevel::Debug,
            tracing::Level::INFO => LogEntryLevel::Info,
            tracing::Level::WARN => LogEntryLevel::Warn,
            tracing::Level::ERROR => LogEntryLevel::Error,
        }
    }
}

/// Assigns the given sink as the log sink on the Rust side.
///
/// If there was already a different sink assigned, it is replaced.
///
/// Call this function to forward logs from the Rust side to the Dart side. This is useful to show
/// logs in the Flutter logging output.
///
/// Only done on Android and iOS. On other platforms, logs are printed to standard error output.
pub fn create_log_stream(_s: StreamSink<LogEntry>) {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    crate::logging::dart::set_stream_sink(_s)
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use flate2::read::GzDecoder;

    use super::*;

    #[test]
    fn tar() -> anyhow::Result<()> {
        let mut app_buffer = FileRingBuffer::anon(500)?;
        let mut background_buffer = FileRingBuffer::anon(500)?;

        writeln!(app_buffer, "app logs")?;
        writeln!(app_buffer, "Hello, world!")?;

        writeln!(background_buffer, "background logs")?;
        writeln!(background_buffer, "Hello, world!")?;

        let tar_data = tar_logs_impl(&Arc::new(FileRingBufferLock::new(app_buffer)), || {
            Ok(background_buffer)
        })?;

        let decoder = GzDecoder::new(&*tar_data);
        let mut tar = tar::Archive::new(decoder);
        for entry in tar.entries()? {
            let mut entry = entry?;
            let path = entry.path()?;

            let mut content = String::new();
            if path == Path::new("logs/app.log") {
                entry.read_to_string(&mut content)?;
                assert_eq!(content, "app logs\nHello, world!\n");
            } else if path == Path::new("logs/background.log") {
                entry.read_to_string(&mut content)?;
                assert_eq!(content, "background logs\nHello, world!\n");
            } else {
                panic!("Unexpected file in tar: {}", path.display());
            }
        }

        Ok(())
    }
}
