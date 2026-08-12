// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{cmp::Ordering, sync::Arc};

use airprotos::common::v1::{
    ClientMetadata, StatusDetails, StatusDetailsCode, VersionUnsupportedDetail,
    status_details::Detail,
};
use chrono::{DateTime, Utc};
use prost::Message;
use semver::Version;
use serde::Deserialize;
use tonic::{Code, Status};
use tracing::{error, warn};

/// Expiration entry: all client versions below `before` expire at
/// `expires_at`.
#[derive(Debug, Clone, Deserialize)]
pub struct VersionExpiration {
    pub before: Version,
    pub expires_at: DateTime<Utc>,
}

/// Determines which client versions are expired or are about to expire.
///
/// A client version matches every expiration entry whose `before` version is
/// greater than the client version; the earliest `expires_at` among the
/// matching entries wins. Versions are compared by `(major, minor, patch)`
/// only: prerelease and build metadata are ignored, so that e.g. `0.20.0-dev`
/// builds count as `0.20.0`.
///
/// An empty policy never expires any version.
#[derive(Debug, Clone, Default)]
pub struct VersionPolicy {
    expirations: Arc<[VersionExpiration]>,
}

/// Status of a client version under a [`VersionPolicy`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VersionStatus {
    /// The version has no expiration.
    Ok,
    /// The version expires at the given future time.
    ExpiresAt(DateTime<Utc>),
    /// The version has expired.
    Expired,
}

impl VersionPolicy {
    pub fn new(expirations: Vec<VersionExpiration>) -> Self {
        Self {
            expirations: expirations.into(),
        }
    }

    fn is_empty(&self) -> bool {
        self.expirations.is_empty()
    }

    pub(crate) fn evaluate(&self, version: &Version, now: DateTime<Utc>) -> VersionStatus {
        let expires_at = self
            .expirations
            .iter()
            .filter(|expiration| release_cmp(version, &expiration.before) == Ordering::Less)
            .map(|expiration| expiration.expires_at)
            .min();
        match expires_at {
            None => VersionStatus::Ok,
            Some(expires_at) if expires_at <= now => VersionStatus::Expired,
            Some(expires_at) => VersionStatus::ExpiresAt(expires_at),
        }
    }

    /// The minimum supported version at `now`: the largest `before` among the
    /// already expired entries. `None` if nothing has expired yet.
    fn min_supported(&self, now: DateTime<Utc>) -> Option<&Version> {
        self.expirations
            .iter()
            .filter(|expiration| expiration.expires_at <= now)
            .map(|expiration| &expiration.before)
            .max()
    }
}

/// Compares versions by `(major, minor, patch)`, ignoring prerelease and
/// build metadata.
fn release_cmp(a: &Version, b: &Version) -> Ordering {
    (a.major, a.minor, a.patch).cmp(&(b.major, b.minor, b.patch))
}

/// A client version that passed [`verify_client_version`].
#[derive(Debug)]
pub(crate) struct VerifiedClientVersion {
    pub(crate) version: Option<Version>,
    /// Set if the version expires in the future.
    pub(crate) expires_at: Option<DateTime<Utc>>,
}

/// Verifies that the client version has not expired under the given policy.
///
/// On success, returns the client version (if any was sent) together with its
/// future expiration time (if any).
///
/// If the version has expired, this function returns a [`Status`] with
/// [`Code::FailedPrecondition`] and [`StatusDetailsCode::VersionUnsupported`].
/// The same applies when the client sends no (or an invalid) version while
/// some version has already expired: such a client cannot prove that it is
/// recent enough.
pub(crate) fn verify_client_version(
    policy: &VersionPolicy,
    client_metadata: Option<&ClientMetadata>,
) -> Result<VerifiedClientVersion, Status> {
    verify_client_version_at(policy, client_metadata, Utc::now())
}

fn verify_client_version_at(
    policy: &VersionPolicy,
    client_metadata: Option<&ClientMetadata>,
    now: DateTime<Utc>,
) -> Result<VerifiedClientVersion, Status> {
    if policy.is_empty() {
        // parse client version, but don't fail
        let version = client_metadata.and_then(|metadata| {
            let version = metadata.version.clone()?;
            version.try_into().ok()
        });
        return Ok(VerifiedClientVersion {
            version,
            expires_at: None,
        });
    }

    let min_supported = policy.min_supported(now);

    let version = match client_metadata.and_then(|metadata| metadata.version.clone()) {
        Some(version) => match Version::try_from(version) {
            Ok(version) => Some(version),
            Err(error) => {
                error!(%error, "invalid client version");
                None
            }
        },
        None => None,
    };

    let Some(version) = version else {
        // A client without a (valid) version cannot prove that it is recent
        // enough; block it once some version has expired.
        return if let Some(min_supported) = min_supported {
            warn!("missing or invalid client version");
            Err(failed_version_precondition(
                "missing or invalid client version",
                None,
                min_supported,
            ))
        } else {
            Ok(VerifiedClientVersion {
                version: None,
                expires_at: None,
            })
        };
    };

    match policy.evaluate(&version, now) {
        VersionStatus::Ok => Ok(VerifiedClientVersion {
            version: Some(version),
            expires_at: None,
        }),
        VersionStatus::ExpiresAt(expires_at) => Ok(VerifiedClientVersion {
            version: Some(version),
            expires_at: Some(expires_at),
        }),
        VersionStatus::Expired => {
            let min_supported = min_supported.expect("an expired version implies an expired entry");
            warn!(
                %version,
                %min_supported, "client version has expired"
            );
            Err(failed_version_precondition(
                "client version has expired",
                Some(&version),
                min_supported,
            ))
        }
    }
}

fn failed_version_precondition(
    message: impl Into<String>,
    client_version: Option<&Version>,
    min_supported: &Version,
) -> Status {
    Status::with_details(
        Code::FailedPrecondition,
        message,
        StatusDetails {
            code: StatusDetailsCode::VersionUnsupported.into(),
            detail: Some(Detail::VersionUnsupported(VersionUnsupportedDetail {
                client_version: client_version.map(|v| v.to_string()),
                client_version_requirement: format!(">={min_supported}"),
            })),
        }
        .encode_to_vec()
        .into(),
    )
}

#[cfg(test)]
mod tests {
    use airprotos::common::v1::Version as VersionProto;
    use chrono::Duration;

    use super::*;

    fn mock_client_metadata(major: u64, minor: u64, patch: u64) -> ClientMetadata {
        let version_struct = VersionProto {
            major,
            minor,
            patch,
            pre: Default::default(),
            build_number: 0,
            commit_hash: Default::default(),
        };
        ClientMetadata {
            version: Some(version_struct),
        }
    }

    fn policy(entries: &[(&str, DateTime<Utc>)]) -> VersionPolicy {
        VersionPolicy::new(
            entries
                .iter()
                .map(|(before, expires_at)| VersionExpiration {
                    before: Version::parse(before).unwrap(),
                    expires_at: *expires_at,
                })
                .collect(),
        )
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-07-15T00:00:00Z")
            .unwrap()
            .to_utc()
    }

    fn check_version_unsupported_status(status: &Status) -> bool {
        if status.code() != Code::FailedPrecondition {
            return false;
        }
        StatusDetails::from_status(status)
            .map(|details| details.code() == StatusDetailsCode::VersionUnsupported)
            .unwrap_or(false)
    }

    #[test]
    fn test_empty_policy() {
        let policy = VersionPolicy::default();
        let metadata = mock_client_metadata(1, 2, 3);
        let result = verify_client_version_at(&policy, Some(&metadata), now());
        assert!(result.is_ok(), "Should succeed when the policy is empty");
        let verified = result.unwrap();
        assert_eq!(verified.version, Some(Version::new(1, 2, 3)));
        assert_eq!(verified.expires_at, None);

        let result = verify_client_version_at(&policy, None, now());
        assert!(
            result.is_ok(),
            "Should succeed without metadata when the policy is empty"
        );
    }

    #[test]
    fn test_unexpired_version() {
        let policy = policy(&[("0.19.0", now() - Duration::days(1))]);
        let metadata = mock_client_metadata(0, 19, 0);
        let result = verify_client_version_at(&policy, Some(&metadata), now());
        let verified = result.expect("0.19.0 is not below 0.19.0");
        assert_eq!(verified.expires_at, None);
    }

    #[test]
    fn test_expired_version() {
        let policy = policy(&[("0.19.0", now() - Duration::days(1))]);
        let metadata = mock_client_metadata(0, 18, 2);
        let result = verify_client_version_at(&policy, Some(&metadata), now());

        let status = result.expect_err("0.18.2 expired a day ago");
        assert!(
            check_version_unsupported_status(&status),
            "Status details must indicate VersionUnsupported"
        );
        assert_eq!(status.code(), Code::FailedPrecondition);
        assert!(status.message().contains("expired"));
    }

    #[test]
    fn test_soon_expiring_version() {
        let expires_at = now() + Duration::days(10);
        let policy = policy(&[("0.20.0", expires_at)]);
        let metadata = mock_client_metadata(0, 19, 0);
        let result = verify_client_version_at(&policy, Some(&metadata), now());
        let verified = result.expect("0.19.0 expires only in 10 days");
        assert_eq!(verified.expires_at, Some(expires_at));
    }

    #[test]
    fn test_earliest_expiry_wins() {
        let earlier = now() + Duration::days(5);
        let later = now() + Duration::days(10);
        let policy = policy(&[("0.20.0", later), ("0.19.0", earlier)]);

        // 0.18.0 matches both entries -> earliest expiry
        let metadata = mock_client_metadata(0, 18, 0);
        let verified = verify_client_version_at(&policy, Some(&metadata), now()).unwrap();
        assert_eq!(verified.expires_at, Some(earlier));

        // 0.19.0 matches only the 0.20.0 entry
        let metadata = mock_client_metadata(0, 19, 0);
        let verified = verify_client_version_at(&policy, Some(&metadata), now()).unwrap();
        assert_eq!(verified.expires_at, Some(later));
    }

    #[test]
    fn test_prerelease_is_ignored() {
        // 0.20.0-dev must count as 0.20.0, i.e. not be below 0.20.0
        let policy = policy(&[("0.20.0", now() - Duration::days(1))]);
        let metadata = ClientMetadata {
            version: Some(VersionProto {
                major: 0,
                minor: 20,
                patch: 0,
                pre: "dev".to_owned(),
                build_number: 69,
                commit_hash: vec![0xf3, 0x22, 0x68, 0x79],
            }),
        };
        let result = verify_client_version_at(&policy, Some(&metadata), now());
        assert!(result.is_ok(), "prerelease of 0.20.0 must not be expired");
    }

    #[test]
    fn test_missing_client_metadata() {
        // nothing expired yet: missing metadata is allowed
        let lenient_policy = policy(&[("0.20.0", now() + Duration::days(10))]);
        let result = verify_client_version_at(&lenient_policy, None, now());
        assert!(
            result.is_ok(),
            "Should succeed while nothing has expired yet"
        );

        // once something expired, missing metadata is blocked
        let expired_policy = policy(&[("0.20.0", now() - Duration::days(1))]);
        let result = verify_client_version_at(&expired_policy, None, now());
        let status = result.expect_err("Should fail when client metadata is missing");
        assert!(
            check_version_unsupported_status(&status),
            "Status details must indicate VersionUnsupported"
        );
        assert_eq!(status.code(), Code::FailedPrecondition);
        assert!(status.message().contains("missing or invalid"));
    }

    #[test]
    fn test_missing_client_version_field() {
        let policy = policy(&[("0.20.0", now() - Duration::days(1))]);
        let metadata = ClientMetadata {
            version: None, // The Protobuf optional field is missing
        };
        let result = verify_client_version_at(&policy, Some(&metadata), now());
        let status = result.expect_err("Should fail when client version field is missing");
        assert_eq!(status.code(), Code::FailedPrecondition);
        assert!(status.message().contains("missing or invalid"));
    }

    #[test]
    fn test_min_supported_in_details() {
        let policy = policy(&[
            ("0.19.0", now() - Duration::days(30)),
            ("0.20.0", now() - Duration::days(1)),
        ]);
        let metadata = mock_client_metadata(0, 18, 0);
        let status = verify_client_version_at(&policy, Some(&metadata), now()).unwrap_err();
        let details = StatusDetails::from_status(&status).unwrap();
        let Some(Detail::VersionUnsupported(detail)) = details.detail else {
            panic!("expected VersionUnsupported detail");
        };
        assert_eq!(detail.client_version.as_deref(), Some("0.18.0"));
        assert_eq!(detail.client_version_requirement, ">=0.20.0");
    }
}
