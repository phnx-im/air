// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{io, path::Path};

#[derive(Debug)]
pub(crate) struct GlobalLock {
    #[cfg(not(target_os = "android"))]
    file: super::file_lock::FileLock,
    #[cfg(target_os = "android")]
    lock: tokio::sync::Mutex<()>,
}

impl GlobalLock {
    pub(crate) fn new(_path: impl AsRef<Path>) -> io::Result<Self> {
        #[cfg(not(target_os = "android"))]
        {
            Ok(Self {
                file: super::file_lock::FileLock::new(_path)?,
            })
        }
        #[cfg(target_os = "android")]
        {
            Ok(Self {
                lock: tokio::sync::Mutex::new(()),
            })
        }
    }

    /// Note: `&mut self` makes sure that the file cannot be locked twice which is unspecified
    /// behavior and platform dependent.
    pub(crate) async fn lock<'a>(&'a mut self) -> io::Result<impl Drop + 'a> {
        #[cfg(not(target_os = "android"))]
        {
            self.file.lock().await
        }
        #[cfg(target_os = "android")]
        {
            Ok(self.lock.lock().await)
        }
    }

    #[cfg(any(test, feature = "test_utils"))]
    pub(crate) fn from_path(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::new(path)
    }
}
