// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    fs::{File, OpenOptions},
    io,
    path::Path,
    sync::Arc,
};

use tokio::task::spawn_blocking;

#[derive(Debug)]
pub(crate) struct FileLock {
    file: Arc<File>,
}

impl FileLock {
    pub(crate) fn new(path: impl AsRef<Path>) -> io::Result<Self> {
        let file = OpenOptions::new()
            .write(true)
            .read(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        Ok(Self {
            file: Arc::new(file),
        })
    }

    pub(crate) fn from_file(file: File) -> io::Result<Self> {
        Ok(Self {
            file: Arc::new(file),
        })
    }

    /// Note: `&mut self` makes sure that the file cannot be locked twice which is unspecified
    /// behavior and platform dependent.
    pub(crate) async fn lock(&mut self) -> io::Result<FileLockGuard<'_>> {
        let file = self.file.clone();
        spawn_blocking(move || file.lock())
            .await
            .map_err(|_| io::Error::other("background task failed"))??;
        Ok(FileLockGuard {
            file_lock: Some(self),
        })
    }
}

#[derive(Debug)]
#[must_use]
pub struct FileLockGuard<'a> {
    file_lock: Option<&'a mut FileLock>,
}

impl Drop for FileLockGuard<'_> {
    fn drop(&mut self) {
        if let Some(lock_file) = self.file_lock.take() {
            let _ = lock_file.file.unlock();
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;
    use tokio::time::{Duration, timeout};

    use super::*;

    #[tokio::test]
    async fn lock_and_unlock() -> anyhow::Result<()> {
        let dir = tempdir().unwrap();
        let path = dir.path().join("lockfile");

        let mut file_lock = FileLock::new(&path)?;
        let guard = file_lock.lock().await.expect("failed to lock file");

        let mut file_lock2 = FileLock::new(&path)?;
        timeout(Duration::from_millis(100), file_lock2.lock())
            .await
            .expect_err("another file lock should timeout");

        drop(guard);

        let _guard = file_lock2.lock().await?;

        Ok(())
    }

    #[tokio::test]
    async fn blocks_until_unlocked() -> anyhow::Result<()> {
        let dir = tempdir().unwrap();
        let path = dir.path().join("lockfile");

        let mut file_lock = FileLock::new(&path)?;
        let guard = file_lock.lock().await.expect("failed to lock file");

        let handle = tokio::spawn(async move {
            let mut file_lock = FileLock::new(&path)?;
            file_lock.lock().await?;
            Ok::<(), anyhow::Error>(())
        });

        drop(guard);
        handle.await??;

        Ok(())
    }
}
