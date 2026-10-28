// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Rate Limiter

use chrono::{DateTime, TimeDelta, Utc};
use sha2::{Digest, Sha256};

pub(crate) mod provider;

#[derive(Debug, Clone)]
pub(crate) struct RlConfig {
    pub(crate) max_requests: u64,
    pub(crate) time_window: TimeDelta,
}

/// A rate-limited operation. Operations sharing a variant share an allowance.
#[derive(Debug, Clone, Copy)]
pub(crate) enum RlOperation {
    /// Creating a group, which claims a reserved group id.
    ReserveGroupId,
    /// Opening a linking session on the relay.
    RelayProvision,
    /// Attaching to a linking session on the relay.
    RelayLink,
}

impl RlOperation {
    fn service_name(self) -> &'static [u8] {
        match self {
            RlOperation::ReserveGroupId => b"ds",
            RlOperation::RelayProvision | RlOperation::RelayLink => b"rs",
        }
    }

    fn rpc_name(self) -> &'static [u8] {
        match self {
            RlOperation::ReserveGroupId => b"reserve_group_id",
            RlOperation::RelayProvision => b"multi_device_provision_client",
            RlOperation::RelayLink => b"multi_device_link_client",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RlKey {
    key: [u8; 32],
}

impl RlKey {
    pub(crate) fn new(operation: RlOperation, custom: &[&[u8]]) -> Self {
        let key = {
            let mut hasher = Sha256::new();

            for part in [operation.service_name(), operation.rpc_name()]
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
    /// Atomically consumes one request from the allowance of `key` at `now` and
    /// reports whether it is allowed.
    async fn try_consume(&self, config: &RlConfig, key: &RlKey, now: DateTime<Utc>) -> bool;
}

pub(crate) struct RateLimiter<S: StorageProvider> {
    config: RlConfig,
    storage: S,
}

impl<S: StorageProvider> RateLimiter<S> {
    pub(crate) fn new(config: RlConfig, storage: S) -> Self {
        RateLimiter { config, storage }
    }

    pub(crate) async fn allowed(&self, key: RlKey, now: DateTime<Utc>) -> bool {
        self.storage.try_consume(&self.config, &key, now).await
    }
}
