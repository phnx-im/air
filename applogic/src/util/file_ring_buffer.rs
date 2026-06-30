// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use bytes::{Buf, BufMut};
use memmap2::{MmapMut, MmapOptions};
use parking_lot::Mutex;

use std::{fs::OpenOptions, io, ops::DerefMut, path::Path};

/// Append-only fixed-length ring buffer backed by a memory-mapped file.
///
/// ## Memory Layout
///
/// ```
/// [ HEADER: 16 bytes                    ] [ DATA ]
/// [ MAGIC: 8 bytes ] [ WRITTEN: 8 bytes ] [ ...  ]
/// ```
///
/// * MAGIC is a magic number that identifies the file as a ring buffer, 8
///   bytes.
/// * WRITTEN is the total number of bytes ever written to the buffer, including
///   bytes that were later overwritten. u64 stored in little-endian format, 8
///   bytes.
///
/// If the MAGIC constant does not match, the file is overwritten with a fresh
/// ring buffer (filled with 0). This is what discards files written in an
/// older/incompatible format.
///
/// The total size of the buffer in memory is `16 + len`.
///
/// The write position is `WRITTEN % len`. While `WRITTEN <= len` the buffer has
/// not wrapped and only `[0, WRITTEN)` holds valid data; once `WRITTEN > len`
/// the whole `len`-byte buffer is valid. Reading returns exactly this valid
/// window, oldest byte first, never the unwritten remainder, so the unwritten
/// (zero-filled) region can never leak into the data.
#[derive(Debug)]
pub struct FileRingBuffer {
    mmap: MmapMut,
}

const HEADER_LEN: usize = 16;
const MAGIC: u64 = 0x4149_524C_4F47_0002; // AIRLOG v2
const WRITTEN_OFFSET: usize = 8;

impl FileRingBuffer {
    pub fn open(file_path: impl AsRef<Path>, len: usize) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(file_path)?;

        file.set_len((HEADER_LEN + len).try_into().expect("usize overflow"))?;

        let mut mmap = unsafe { MmapOptions::new().map_mut(&file)? };

        if (&mmap[..HEADER_LEN]).get_u64_le() != MAGIC {
            // old format, fresh or garbage file => discard and reinit
            mmap.fill(0);
            let mut buf = &mut *mmap;
            buf.put_u64_le(MAGIC);
            buf.put_u64_le(0); // written = 0
        }

        Ok(Self { mmap })
    }

    pub fn anon(len: usize) -> io::Result<Self> {
        let mmap = MmapOptions::new().len(HEADER_LEN + len).map_anon()?;
        Ok(Self { mmap })
    }

    /// Clears the buffer.
    ///
    /// Note that the length of the buffer remains unchanged, but all data is
    /// overwritten with zero bytes.
    pub fn clear(&mut self) {
        self.mmap[WRITTEN_OFFSET..].fill(0); // this also sets written to 0
    }

    /// Returns `true` if the buffer is empty, that is, [`Self::len()`] is 0.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the length of the buffer.
    ///
    /// The length of the buffer is constant and remains the same as it was
    /// during creation.
    pub fn len(&self) -> usize {
        self.data().len()
    }

    /// Returns a [`Buf`] over the valid data, oldest byte first.
    ///
    /// This is `[0, written)` while the buffer has not wrapped, otherwise the
    /// whole buffer starting at the current write position. The unwritten
    /// remainder is never included.
    pub fn buf(&self) -> impl Buf + '_ {
        let len = self.data().len();
        let written = self.read_written();
        let pos = if written > len { written % len } else { 0 };
        let remaining = written.min(len);
        RingBufferReader {
            buf: self,
            pos,
            remaining,
        }
    }

    /// Returns the current write position (`written % len`), where the next
    /// byte will be written.
    fn read_tail(&self) -> usize {
        let len = self.data().len();
        if len == 0 {
            return 0;
        }
        self.read_written() % len
    }

    /// Written is encoded in the 16-bytes header after MAGIC in u64
    /// little-endian format
    fn read_written(&self) -> usize {
        let mut buf = &self.mmap[WRITTEN_OFFSET..WRITTEN_OFFSET + 8];
        buf.get_u64_le().try_into().expect("usize overflow")
    }

    /// Written is encoded in the 16-bytes header after MAGIC in u64
    /// little-endian format
    fn write_written(&mut self, written: usize) {
        let written: u64 = written.try_into().expect("usize overflow");
        self.mmap[WRITTEN_OFFSET..WRITTEN_OFFSET + 8].copy_from_slice(&written.to_le_bytes());
    }

    fn data(&self) -> &[u8] {
        &self.mmap[HEADER_LEN..]
    }

    fn data_mut(&mut self) -> &mut [u8] {
        &mut self.mmap[HEADER_LEN..]
    }

    fn write_data(&mut self, mut new_data: &[u8]) -> io::Result<()> {
        if self.data().is_empty() {
            // special case: the buffer is empty => avoid division by zero
            return Ok(());
        }

        if self.len() <= new_data.len() {
            // This is equivalent to writing the new_data to the circular buffer and
            // overwriting the non-fitting prefix. Only the suffix of the
            // new_data that fits in the buffer is written.
            let offset = new_data.len() - self.len();
            new_data = &new_data[offset..];
        }

        let tail = self.read_tail();
        let data = self.data_mut();

        debug_assert!(tail < data.len());
        let left_len = new_data.len().min(data.len() - tail);
        debug_assert!(left_len <= new_data.len());
        let right_len = new_data.len() - left_len;
        debug_assert_eq!(left_len + right_len, new_data.len());

        let (left_data, right_data) = new_data.split_at(left_len);
        data[tail..tail + left_len].copy_from_slice(left_data);
        data[..right_len].copy_from_slice(right_data);

        let written = self.read_written() + new_data.len();
        self.write_written(written);

        Ok(())
    }
}

impl io::Write for FileRingBuffer {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.write_data(data)?;
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.mmap.flush()
    }
}

#[derive(Debug)]
struct RingBufferReader<'a> {
    buf: &'a FileRingBuffer,
    pos: usize,
    remaining: usize,
}

impl Buf for RingBufferReader<'_> {
    fn remaining(&self) -> usize {
        self.remaining
    }

    fn chunk(&self) -> &[u8] {
        if self.remaining == 0 {
            return &[];
        }
        let data = self.buf.data();
        let end = (self.pos + self.remaining).min(data.len());
        &data[self.pos..end]
    }

    fn advance(&mut self, cnt: usize) {
        self.pos = (self.pos + cnt) % self.buf.data().len();
        self.remaining -= cnt;
    }
}

/// A thread-safe wrapper around [`FileRingBuffer`].
///
/// `Arc<FileRingBufferLock>` can be used as a writer for
/// [`tracing_subscriber::fmt::Subscriber`].
#[derive(Debug)]
pub struct FileRingBufferLock {
    inner: Mutex<FileRingBuffer>,
}

impl FileRingBufferLock {
    pub fn new(buffer: FileRingBuffer) -> Self {
        Self {
            inner: Mutex::new(buffer),
        }
    }

    pub fn lock(&self) -> impl DerefMut<Target = FileRingBuffer> + '_ {
        self.inner.lock()
    }

    pub fn into_inner(self) -> FileRingBuffer {
        self.inner.into_inner()
    }
}

impl io::Write for &FileRingBufferLock {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.lock().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.lock().flush()
    }
}

#[cfg(test)]
mod tests {
    use quickcheck_macros::quickcheck;

    use super::*;

    use std::io::{BufRead, Read, Write};

    #[test]
    fn non_circular_read_write() -> io::Result<()> {
        let data = "Hello, World!";
        // slightly larger buffer than data
        let mut ring_buffer = FileRingBuffer::anon(data.len() + 3)?;

        write!(ring_buffer, "{data}").unwrap();

        let mut lines = ring_buffer.buf().reader().lines().map_while(Result::ok);
        // Only the bytes actually written are part of the valid window; the
        // unwritten remainder of the buffer is not read back.
        assert_eq!(lines.next().unwrap(), data);
        assert_eq!(lines.next(), None);

        Ok(())
    }

    #[test]
    fn read_write_utf8() -> io::Result<()> {
        let bear = "🐻";
        let hedgehog = "🦔";
        let data = format!("{bear}{hedgehog}");
        assert_eq!(data.len(), 8);

        // buffer which is not multiple of data.len()
        let mut ring_buffer = FileRingBuffer::anon(8 + 6)?;

        write!(ring_buffer, "{data}{data}")?;

        let mut buf = Vec::new();
        ring_buffer.buf().reader().read_to_end(&mut buf)?;

        assert_eq!(
            String::from_utf8_lossy(&buf),
            format!("��{hedgehog}{bear}{hedgehog}")
        );

        Ok(())
    }

    #[test]
    fn circular_read_write() -> io::Result<()> {
        let mut ring_buffer = FileRingBuffer::anon(40)?;

        writeln!(ring_buffer, "Hello, world!").unwrap();
        writeln!(ring_buffer, "This is a test.").unwrap();
        writeln!(ring_buffer, "Another line.").unwrap();

        let mut lines = ring_buffer.buf().reader().lines().map_while(Result::ok);
        assert_eq!(lines.next().unwrap(), "o, world!");
        assert_eq!(lines.next().unwrap(), "This is a test.");
        assert_eq!(lines.next().unwrap(), "Another line.");
        assert_eq!(lines.next(), None);
        drop(lines);

        Ok(())
    }

    #[quickcheck]
    fn model_test(capacity: u8, data: Vec<String>) -> io::Result<()> {
        let len = capacity as usize;

        let mut ring_buffer = FileRingBuffer::anon(len)?;
        // The model holds only the valid window (no zero padding): the most
        // recent `len` bytes that were actually written.
        let mut model_buffer: Vec<u8> = Vec::new();

        for data in data {
            ring_buffer.write_all(data.as_bytes())?;

            let offset = data.len().saturating_sub(len);
            model_buffer.extend_from_slice(&data.as_bytes()[offset..]);
            if model_buffer.len() > len {
                model_buffer.drain(..model_buffer.len() - len);
            }
        }

        let mut ring_buffer_data = Vec::new();
        ring_buffer
            .buf()
            .reader()
            .read_to_end(&mut ring_buffer_data)?;
        assert_eq!(ring_buffer_data, model_buffer);

        Ok(())
    }

    fn temp_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "air_ring_buffer_test_{}_{}.log",
            std::process::id(),
            tag
        ))
    }

    /// Reproduces the original defect. A stale `written` (e.g. a torn header
    /// that lags the data actually present in the buffer) used to make the
    /// reader walk past the frontier, reading leftover data, then the
    /// unwritten zero region, then wrapping around, which surfaced as a
    /// multi-megabyte line with NUL bytes spliced into the middle. The
    /// `written`-bounded reader must only return the valid prefix.
    #[test]
    fn stale_written_does_not_read_gap_or_stale_data() -> io::Result<()> {
        let mut ring_buffer = FileRingBuffer::anon(64)?;

        // Valid data occupies [0, 40); more (now-stale) data is at [40, 56);
        // [56, 64) was never written and is zero-filled.
        ring_buffer.write_all(&[b'A'; 40])?;
        ring_buffer.write_all(&[b'B'; 16])?;
        assert_eq!(ring_buffer.read_written(), 56);

        // Simulate a torn/stale header that lags the real data extent.
        ring_buffer.write_written(40);

        let mut out = Vec::new();
        ring_buffer.buf().reader().read_to_end(&mut out)?;

        // Only the valid prefix is returned: no zero gap (the symptom of the
        // bug) and no stale 'B' bytes from beyond the recorded frontier.
        assert_eq!(out, vec![b'A'; 40]);
        assert!(!out.contains(&0), "read must not contain NUL bytes");
        assert!(!out.contains(&b'B'), "read must not contain stale data");

        Ok(())
    }

    #[test]
    fn wrapped_read_returns_last_len_bytes() -> io::Result<()> {
        let mut ring_buffer = FileRingBuffer::anon(8)?;
        ring_buffer.write_all(b"01234")?; // written = 5
        ring_buffer.write_all(b"56789")?; // written = 10, wraps
        assert_eq!(ring_buffer.read_written(), 10);

        let mut out = Vec::new();
        ring_buffer.buf().reader().read_to_end(&mut out)?;
        // The valid window is the last `len` (= 8) bytes of "0123456789".
        assert_eq!(out, b"23456789");

        Ok(())
    }

    #[test]
    fn clear_empties_buffer() -> io::Result<()> {
        let mut ring_buffer = FileRingBuffer::anon(64)?;
        ring_buffer.write_all(b"some data\n")?;
        ring_buffer.clear();

        assert_eq!(ring_buffer.read_written(), 0);
        let mut out = Vec::new();
        ring_buffer.buf().reader().read_to_end(&mut out)?;
        assert!(out.is_empty());

        Ok(())
    }

    /// An existing file without our MAGIC (old format / garbage) must be fully
    /// discarded on open, so stale old-format bytes never leak into reads.
    #[test]
    fn open_discards_old_format_file() -> io::Result<()> {
        let path = temp_path("old_format");
        let _ = std::fs::remove_file(&path);

        // Old format: an 8-byte tail header (a small value, never == MAGIC)
        // followed by old log data.
        {
            let mut f = std::fs::File::create(&path)?;
            f.write_all(&123u64.to_le_bytes())?;
            f.write_all(b"OLD LOG LINE\n")?;
            f.flush()?;
        }

        let ring_buffer = FileRingBuffer::open(&path, 64)?;

        assert_eq!(ring_buffer.read_written(), 0);
        let mut out = Vec::new();
        ring_buffer.buf().reader().read_to_end(&mut out)?;
        assert!(out.is_empty(), "old-format data must be wiped on open");

        let _ = std::fs::remove_file(&path);
        Ok(())
    }

    /// A file written with the current format must be recognized via MAGIC on
    /// reopen and its data kept (no reinit).
    #[test]
    fn open_persists_and_reopens_without_wiping() -> io::Result<()> {
        let path = temp_path("persist");
        let _ = std::fs::remove_file(&path);

        {
            let mut ring_buffer = FileRingBuffer::open(&path, 64)?;
            writeln!(ring_buffer, "persisted line")?;
            ring_buffer.flush()?;
        }

        let ring_buffer = FileRingBuffer::open(&path, 64)?;
        let mut lines = ring_buffer.buf().reader().lines().map_while(Result::ok);
        assert_eq!(lines.next().unwrap(), "persisted line");
        assert_eq!(lines.next(), None);
        drop(lines);

        let _ = std::fs::remove_file(&path);
        Ok(())
    }
}
