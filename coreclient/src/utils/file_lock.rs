// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

#![cfg_attr(
    target_os = "android",
    expect(unused, reason = "file locking is not supported on Android")
)]

use std::{
    io,
    path::{Path, PathBuf},
};

use sqlx::{
    ConnectOptions, SqliteConnection,
    sqlite::{SqliteConnectOptions, SqliteJournalMode},
};

#[derive(Debug)]
pub(crate) struct FileLock {
    path: PathBuf,
    connection: Option<SqliteConnection>,
}

impl FileLock {
    pub(crate) fn new(path: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self {
            path: path.as_ref().to_path_buf(),
            connection: None,
        })
    }

    async fn open_and_lock(path: &Path) -> io::Result<SqliteConnection> {
        // Use a dedicated SQLite file with classic locking (no WAL) and an exclusive
        // transaction to emulate a global mutex.
        let mut connection: SqliteConnection = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Delete)
            .connect()
            .await
            .map_err(io::Error::other)?;

        // Ensure the file has a SQLite header on disk.
        sqlx::query("PRAGMA user_version = 1")
            .execute(&mut connection)
            .await
            .map_err(io::Error::other)?;

        // Keep the connection in exclusive locking mode and start an exclusive transaction.
        sqlx::query("PRAGMA locking_mode = EXCLUSIVE")
            .execute(&mut connection)
            .await
            .map_err(io::Error::other)?;

        sqlx::query("BEGIN EXCLUSIVE")
            .execute(&mut connection)
            .await
            .map_err(io::Error::other)?;

        Ok(connection)
    }

    /// Note: `&mut self` makes sure that the file cannot be locked twice which is unspecified
    /// behavior and platform dependent.
    pub(crate) async fn lock(&mut self) -> io::Result<FileLockGuard<'_>> {
        if self.connection.is_some() {
            return Err(io::Error::other("lock already held"));
        }

        let connection = Self::open_and_lock(self.path.as_path()).await?;
        self.connection = Some(connection);

        Ok(FileLockGuard { file_lock: self })
    }
}

#[derive(Debug)]
#[must_use]
pub struct FileLockGuard<'a> {
    file_lock: &'a mut FileLock,
}

impl Drop for FileLockGuard<'_> {
    fn drop(&mut self) {
        // Dropping the connection releases the exclusive transaction and file locks.
        let _ = self.file_lock.connection.take();
    }
}

#[cfg(test)]
mod tests {
    use std::{future::Future, pin::Pin};

    use tempfile::tempdir;
    use tokio::time::{Duration, timeout};

    use super::*;

    trait TestLock: Send + Sized + 'static {
        type Guard<'a>: Send + 'a;

        fn new(path: &Path) -> io::Result<Self>;
        fn lock<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = io::Result<Self::Guard<'a>>> + Send + 'a>>;
    }

    impl TestLock for FileLock {
        type Guard<'a> = FileLockGuard<'a>;

        fn new(path: &Path) -> io::Result<Self> {
            FileLock::new(path)
        }

        fn lock<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = io::Result<Self::Guard<'a>>> + Send + 'a>> {
            Box::pin(FileLock::lock(self))
        }
    }

    /// A deliberately broken lock that never blocks.
    struct FakeLock;

    struct FakeLockGuard;

    impl TestLock for FakeLock {
        type Guard<'a> = FakeLockGuard;

        fn new(_path: &Path) -> io::Result<Self> {
            Ok(FakeLock)
        }

        fn lock<'a>(
            &'a mut self,
        ) -> Pin<Box<dyn Future<Output = io::Result<Self::Guard<'a>>> + Send + 'a>> {
            Box::pin(async { Ok(FakeLockGuard) })
        }
    }

    /// Asserts that the second lock attempt blocks until the first is released.
    async fn assert_blocks_until_release<L: TestLock>() -> anyhow::Result<()> {
        let dir = tempdir()?;
        let path = dir.path().join("lockfile");

        let mut first = L::new(&path)?;
        let guard = first.lock().await?;

        let (tx, mut rx) = tokio::sync::watch::channel(false);

        let handle = tokio::spawn({
            let path = path.clone();
            async move {
                let mut second = L::new(&path)?;
                let _g = second.lock().await?;
                let _ = tx.send(true);
                Ok::<(), anyhow::Error>(())
            }
        });

        // While first guard is held, second should not acquire
        assert!(
            tokio::time::timeout(Duration::from_millis(50), rx.changed())
                .await
                .is_err(),
            "second lock acquired too early"
        );

        drop(guard); // release

        // Now it should acquire promptly
        tokio::time::timeout(Duration::from_millis(200), rx.changed()).await??;
        handle.await??;
        Ok(())
    }

    /// BShows that locking and unlocking works.
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

    /// Shows the lock actually blocks.
    #[tokio::test]
    async fn blocks_until_unlocked() -> anyhow::Result<()> {
        assert_blocks_until_release::<FileLock>().await
    }

    /// Proves the helper catches a broken lock implementation.
    #[tokio::test]
    #[should_panic(expected = "second lock acquired too early")]
    async fn blocks_until_unlocked_fails_for_broken_lock() {
        let _ = assert_blocks_until_release::<FakeLock>().await;
    }
}
