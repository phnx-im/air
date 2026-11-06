// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Per-user quotas on the bytes provisioned for uploads.

use aircommon::identifiers::UserId;
use airprotos::delivery_service::v1::StorageObjectType;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    rate_limiter::{
        RateLimiter, RlConfig, RlDecision, RlKey, RlOperation, rolling::RlRollingPostgresStorage,
    },
    settings::StorageSettings,
};

use super::{Ds, attachments::ProvisionObjectError};

/// Whose quota an upload is charged to.
#[derive(Debug, Clone)]
pub(super) enum UploadPayer {
    User(UserId),
    /// A self-group. Its clients sign with leaf keys that name no user.
    SelfGroup(Uuid),
}

impl UploadPayer {
    fn key(&self, operation: RlOperation) -> RlKey {
        match self {
            UploadPayer::User(user_id) => {
                RlKey::new(operation, &[b"user_uuid", user_id.uuid().as_bytes()])
            }
            UploadPayer::SelfGroup(group_uuid) => {
                RlKey::new(operation, &[b"self_group_uuid", group_uuid.as_bytes()])
            }
        }
    }
}

impl Ds {
    /// Charges `content_length` bytes to the quota of `payer` for objects of
    /// `object_type`. Everything fits when no quota is configured.
    ///
    /// # Errors
    ///
    /// Fails with [`ProvisionObjectError::QuotaExceeded`] when the quota has
    /// no room for the upload.
    pub(super) async fn charge_upload(
        &self,
        settings: &StorageSettings,
        payer: &UploadPayer,
        object_type: StorageObjectType,
        content_length: u64,
        now: DateTime<Utc>,
    ) -> Result<(), ProvisionObjectError> {
        let (operation, quota) = match object_type {
            StorageObjectType::DebugLogs => (
                RlOperation::ProvisionDebugLogs,
                settings.debug_logs_upload_quota,
            ),
            StorageObjectType::Unspecified
            | StorageObjectType::Attachment
            | StorageObjectType::GroupProfile
            | StorageObjectType::UserProfile => {
                (RlOperation::ProvisionUpload, settings.upload_quota)
            }
        };
        let Some(quota) = quota else {
            return Ok(());
        };

        let config = RlConfig {
            limit: quota.limit_bytes,
            time_window: quota.window,
        };
        let storage = RlRollingPostgresStorage::new(self.db_pool.clone());
        let decision = RateLimiter::new(config, storage)
            .allowed(payer.key(operation), content_length, now)
            .await;
        match decision {
            RlDecision::Allowed => Ok(()),
            RlDecision::Denied { retry_at } => Err(ProvisionObjectError::QuotaExceeded {
                limit_bytes: quota.limit_bytes,
                retry_after_secs: retry_at.map_or(0, |retry_at| seconds_until(now, retry_at)),
            }),
        }
    }
}

/// Whole seconds from `now` until `at`, rounded up and at least 1.
fn seconds_until(now: DateTime<Utc>, at: DateTime<Utc>) -> u64 {
    let millis = u64::try_from((at - now).num_milliseconds()).unwrap_or(0);
    millis.div_ceil(1000).max(1)
}

#[cfg(test)]
mod test {
    use chrono::TimeDelta;

    use super::*;

    #[test]
    fn retry_seconds_round_up() {
        let now = Utc::now();
        assert_eq!(seconds_until(now, now + TimeDelta::milliseconds(1)), 1);
        assert_eq!(seconds_until(now, now + TimeDelta::milliseconds(1001)), 2);
        assert_eq!(seconds_until(now, now + TimeDelta::seconds(60)), 60);
        assert_eq!(seconds_until(now, now - TimeDelta::seconds(5)), 1);
    }

    #[test]
    fn payers_and_quotas_have_distinct_keys() {
        let uuid = Uuid::new_v4();
        let user = UploadPayer::User(UserId::new(uuid, "example.com".parse().unwrap()));
        let self_group = UploadPayer::SelfGroup(uuid);

        let keys = [
            user.key(RlOperation::ProvisionUpload),
            user.key(RlOperation::ProvisionDebugLogs),
            self_group.key(RlOperation::ProvisionUpload),
            self_group.key(RlOperation::ProvisionDebugLogs),
        ];
        for (i, a) in keys.iter().enumerate() {
            for b in &keys[i + 1..] {
                assert_ne!(a.serialize(), b.serialize());
            }
        }
    }
}
