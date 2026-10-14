// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Rate Limiter

use chrono::TimeDelta;
use sha2::{Digest, Sha256};

pub(crate) mod provider;

#[derive(Debug, Clone)]
pub(crate) struct RlConfig {
    pub(crate) max_requests: u64,
    pub(crate) time_window: TimeDelta,
}

#[derive(Debug, Clone)]
pub(crate) struct RlKey {
    key: [u8; 32],
}

impl RlKey {
    pub(crate) fn new(service_name: &[u8], rpc_name: &[u8], custom: &[&[u8]]) -> Self {
        let key = {
            let mut hasher = Sha256::new();

            for part in [service_name, rpc_name]
                .into_iter()
                .chain(custom.iter().copied())
            {
                hasher.update((part.len() as u32).to_be_bytes());
                hasher.update(part);
            }

            hasher.finalize().into()
        };

        RlKey { key }
    }

    pub(crate) fn serialize(&self) -> &[u8] {
        &self.key
    }
}

pub(crate) trait StorageProvider {
    /// Atomically consumes one request from the allowance of `key` and reports
    /// whether it is allowed.
    async fn try_consume(&self, config: &RlConfig, key: &RlKey) -> bool;
}

pub(crate) struct RateLimiter<S: StorageProvider> {
    config: RlConfig,
    storage: S,
}

impl<S: StorageProvider> RateLimiter<S> {
    pub(crate) fn new(config: RlConfig, storage: S) -> Self {
        RateLimiter { config, storage }
    }

    pub(crate) async fn allowed(&self, key: RlKey) -> bool {
        self.storage.try_consume(&self.config, &key).await
    }
}
