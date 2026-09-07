// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex, PoisonError, Weak},
};

use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

/// The in-process mutexes, one per lockfile path.
///
/// The file lock only excludes across processes, and Android has no file lock
/// at all. Two [`CoreUser`]s loaded from the same database in one process (on
/// Android the push processing worker next to the main app) each open their
/// own [`GlobalLock`], so without this registry their outbound services would
/// encrypt against the same MLS state at the same time.
///
/// [`CoreUser`]: crate::clients::CoreUser
static REGISTRY: LazyLock<Mutex<HashMap<PathBuf, Weak<AsyncMutex<()>>>>> =
    LazyLock::new(Default::default);

fn in_process_lock(path: &Path) -> Arc<AsyncMutex<()>> {
    let key = registry_key(path);
    let mut registry = REGISTRY.lock().unwrap_or_else(PoisonError::into_inner);
    registry.retain(|_, lock| lock.strong_count() > 0);
    if let Some(lock) = registry.get(&key).and_then(Weak::upgrade) {
        return lock;
    }
    let lock = Arc::new(AsyncMutex::new(()));
    registry.insert(key, Arc::downgrade(&lock));
    lock
}

/// Resolves the path so that two spellings of the same lockfile share a registry
/// entry.
///
/// Android hands out the app's files directory through a symlinked path on some
/// devices. The lockfile itself is created on first lock, so only the directory
/// can be resolved.
fn registry_key(path: &Path) -> PathBuf {
    let Some(parent) = path.parent() else {
        return path.to_path_buf();
    };
    let Some(file_name) = path.file_name() else {
        return path.to_path_buf();
    };
    let Ok(parent) = parent.canonicalize() else {
        return path.to_path_buf();
    };
    parent.join(file_name)
}

#[derive(Debug)]
pub(crate) struct GlobalLock {
    in_process: Arc<AsyncMutex<()>>,
    #[cfg(not(target_os = "android"))]
    file: super::file_lock::FileLock,
}

impl GlobalLock {
    pub(crate) fn new(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        Ok(Self {
            in_process: in_process_lock(path),
            #[cfg(not(target_os = "android"))]
            file: super::file_lock::FileLock::new(path)?,
        })
    }

    /// Note: `&mut self` makes sure that the file cannot be locked twice which is unspecified
    /// behavior and platform dependent.
    pub(crate) async fn lock(&mut self) -> io::Result<GlobalLockGuard<'_>> {
        // The in-process mutex is taken first on every platform, so the lock
        // order never differs between two holders.
        let in_process = self.in_process.clone().lock_owned().await;
        Ok(GlobalLockGuard {
            #[cfg(not(target_os = "android"))]
            _file: self.file.lock().await?,
            _in_process: in_process,
            #[cfg(target_os = "android")]
            _borrow: std::marker::PhantomData,
        })
    }

    #[cfg(any(test, feature = "test_utils"))]
    pub(crate) fn from_path(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::new(path)
    }
}

/// Holds the global lock until it is dropped.
///
/// Fields are dropped in declaration order, so the file lock is released before
/// the in-process mutex.
#[derive(Debug)]
#[must_use]
pub(crate) struct GlobalLockGuard<'a> {
    #[cfg(not(target_os = "android"))]
    _file: super::file_lock::FileLockGuard<'a>,
    _in_process: OwnedMutexGuard<()>,
    #[cfg(target_os = "android")]
    _borrow: std::marker::PhantomData<&'a mut ()>,
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tempfile::tempdir;
    use tokio::time::timeout;

    use super::*;

    #[tokio::test]
    async fn same_path_excludes() -> anyhow::Result<()> {
        let dir = tempdir()?;
        let path = dir.path().join("lockfile");

        let mut first = GlobalLock::new(&path)?;
        let mut second = GlobalLock::new(&path)?;

        let guard = first.lock().await?;
        timeout(Duration::from_millis(200), second.lock())
            .await
            .expect_err("a second lock on the same path must block");

        drop(guard);
        let _second = timeout(Duration::from_secs(5), second.lock())
            .await
            .expect("the lock must be free again")?;

        Ok(())
    }

    #[tokio::test]
    async fn different_paths_are_independent() -> anyhow::Result<()> {
        let dir = tempdir()?;
        let mut first = GlobalLock::new(dir.path().join("first"))?;
        let mut second = GlobalLock::new(dir.path().join("second"))?;

        let _first = first.lock().await?;
        let _second = timeout(Duration::from_secs(5), second.lock())
            .await
            .expect("locks on different paths must not exclude each other")?;

        Ok(())
    }

    /// The file lock hides the registry on this host, but the Android build has
    /// only the registry, so the sharing is asserted directly. Equivalent
    /// spellings of a path must land on the same entry.
    #[test]
    fn one_in_process_lock_per_lockfile() -> anyhow::Result<()> {
        let dir = tempdir()?;
        std::fs::create_dir(dir.path().join("sub"))?;

        let direct = in_process_lock(&dir.path().join("lockfile"));
        let indirect = in_process_lock(&dir.path().join("sub").join("..").join("lockfile"));
        let other = in_process_lock(&dir.path().join("other"));

        assert!(Arc::ptr_eq(&direct, &indirect));
        assert!(!Arc::ptr_eq(&direct, &other));

        Ok(())
    }
}
