// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use bytes::Buf;
use memmap2::{MmapMut, MmapOptions};
use parking_lot::Mutex;

use std::{fs::OpenOptions, io, ops::DerefMut, path::Path};

/// Append-only fixed-length ring buffer backed by a memory-mapped file.
///
/// ## Memory Layout
///
/// The first 8 bytes store a little-endian encoded `u64`, representing the position of the buffer's tail.
/// This is the position where data will be written on the next call to [`std::io::Write::write`].
/// The rest of the buffer contains its fixed-length storage. Unwritten bytes are initialized to `0`.
///
/// The tolal size of the buffer in memory is `len + 8`.
///
/// When reading, *all* data is always read, starting at the tail and proceeding circularly until
/// it reaches the tail again (exclusive). Notably, the buffer's length remains constant.
#[derive(Debug)]
pub struct FileRingBuffer {
    mmap: MmapMut,
}

const HEADER_LEN: usize = 8;

impl FileRingBuffer {
    pub fn open(file_path: impl AsRef<Path>, len: usize) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(file_path)?;

        file.set_len((HEADER_LEN + len).try_into().expect("usize overflow"))?;

        let mmap = unsafe { MmapOptions::new().map_mut(&file)? };
        let mut buf = Self { mmap };

        // ensure: tail < self.data().len()
        let tail = buf.read_tail();
        buf.write_tail(tail % buf.data().len());

        Ok(buf)
    }

    pub fn anon(len: usize) -> io::Result<Self> {
        let mmap = MmapOptions::new().len(HEADER_LEN + len).map_anon()?;
        Ok(Self { mmap })
    }

    /// Clears the buffer.
    ///
    /// Note that the length of the buffer remains unchanged, but all data is overwritten with zero
    /// bytes.
    pub fn clear(&mut self) {
        self.mmap.fill(0); // this also sets the tail to 0
    }

    /// Returns `true` if the buffer is empty, that is, [`Self::len()`] is 0.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the length of the buffer.
    ///
    /// The length of the buffer is constant and remains the same as it was during creation.
    pub fn len(&self) -> usize {
        self.data().len()
    }

    /// Returns a `Buf` implementation for reading the data.
    ///
    /// Full buffer is read, starting at the last non-overwritten position.
    pub fn buf(&self) -> impl Buf + '_ {
        RingBufferReader {
            buf: self,
            pos: self.read_tail(),
            flipped: false,
        }
    }

    /// Tail is encoded as 8-bytes header in u64 little-endian format
    fn read_tail(&self) -> usize {
        u64::from_le_bytes(self.mmap[..HEADER_LEN].try_into().expect("logic error"))
            .try_into()
            .expect("usize overflow")
    }

    /// Tail is encoded as 8-bytes header in u64 little-endian format
    fn write_tail(&mut self, tail: usize) {
        let tail: u64 = tail.try_into().expect("usize overflow");
        self.mmap[..HEADER_LEN].copy_from_slice(&tail.to_le_bytes());
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
            // This is equivalent to writing the new_data to the circular buffer and overwriting
            // the non-fitting prefix. Only the suffix of the new_data that fits in the buffer is written.
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

        let tail = (tail + new_data.len()) % data.len();
        self.write_tail(tail);

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
    flipped: bool,
}

impl Buf for RingBufferReader<'_> {
    fn remaining(&self) -> usize {
        let pos = self.pos;
        let tail = self.buf.read_tail();

        if pos < tail {
            tail - pos
        } else if self.flipped {
            0
        } else {
            debug_assert!(pos <= self.buf.data().len());
            self.buf.data().len() - pos
        }
    }

    fn chunk(&self) -> &[u8] {
        let pos = self.pos;
        let tail = self.buf.read_tail();
        if pos < tail {
            &self.buf.data()[pos..tail]
        } else if self.flipped {
            &[]
        } else {
            &self.buf.data()[pos..]
        }
    }

    fn advance(&mut self, cnt: usize) {
        let len = self.buf.data().len();
        let new_pos = self.pos + cnt;
        let (div, new_pos) = (new_pos / len, new_pos % len);
        self.flipped = self.flipped || div > 0;
        self.pos = new_pos;
    }
}

/// A thread-safe wrapper around [`FileRingBuffer`].
///
/// `Arc<FileRingBufferLock>` can be used as a writer for [`tracing_subscriber::fmt::Subscriber`].
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
        // The buffer is larger than the data which was written to it, so the remaining space is
        // filled with 0 byte.
        assert_eq!(lines.next().unwrap(), format!("\0\0\0{data}"));
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
        let mut model_buffer = vec![0u8; len];

        for data in data {
            ring_buffer.write_all(data.as_bytes())?;

            let offset = data.len().saturating_sub(len);
            let data = &data.as_bytes()[offset..];
            model_buffer.extend(data);
            model_buffer = model_buffer.split_off(model_buffer.len() - len);
        }

        let mut ring_buffer_data = Vec::new();
        ring_buffer
            .buf()
            .reader()
            .read_to_end(&mut ring_buffer_data)?;
        assert_eq!(ring_buffer_data, model_buffer);

        Ok(())
    }
}
