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
use tonic::{Code, Status};
use tracing::error;

use crate::settings::VersionExpiration;

/// Cheaply-clonable version policy
///
/// [`Default]` implemenentation constructs an empty policy.
#[derive(Debug, Clone, Default)]
pub struct VersionPolicy {
    // Invariant: sorted by (`older_than`, `expires_on`)
    expirations: Arc<[VersionExpiration]>,
}

enum VersionStatus {
    /// Version has no expiration
    Ok,
    /// Version will expire at the given future date.
    ExpiresAt(DateTime<Utc>),
    /// Version is expired
    Expired,
}

impl VersionExpiration {
    fn expired_at(&self, now: DateTime<Utc>) -> VersionStatus {
        if self.expires_on < now {
            VersionStatus::Expired
        } else {
            VersionStatus::ExpiresAt(self.expires_on)
        }
    }
}

/// A version that passed [`VersionPolicy::verify_client_version`].
#[derive(Debug)]
pub(crate) struct VerifiedClientVersion {
    pub(crate) version: Option<Version>,
    /// Set if the version expires in the future
    // TODO: Will be communicated back to client over QS listen stream
    #[cfg_attr(not(test), expect(dead_code))]
    pub(crate) expires_at: Option<DateTime<Utc>>,
}

impl VersionPolicy {
    pub fn new(mut expirations: Vec<VersionExpiration>) -> Self {
        expirations.sort_unstable();
        Self {
            expirations: expirations.into(),
        }
    }

    fn evaluate(&self, version: &Version, now: DateTime<Utc>) -> VersionStatus {
        // Find the first expiration whose older_than is strictly greater than the version.
        let pos = self
            .expirations
            .partition_point(|e| release_cmp(&e.older_than, version) != Ordering::Greater);
        match self.expirations.get(pos) {
            Some(expiration) => expiration.expired_at(now),
            None => VersionStatus::Ok,
        }
    }

    /// Minimum supported version at `now`, the largest `older_than` version among already expired
    /// versions.
    ///
    /// `None if nothing has expired yet.
    pub fn min_supported(&self, now: DateTime<Utc>) -> Option<&Version> {
        self.expirations
            .iter()
            .rfind(|expiration| matches!(expiration.expired_at(now), VersionStatus::Expired))
            .map(|expiration| &expiration.older_than)
    }

    /// Verifies that the client version matches the given version requirement.
    ///
    /// If the version requirement is not set, this function returns `Ok(None)`, otherwise, on success,
    /// it returns the client version.
    ///
    /// If version requirement does not match, this function returns a [`Status`] with
    /// [`Code::FailedPrecondition`] and [`StatusDetailsCode::VersionUnsupported`].
    pub(crate) fn verify_client_version(
        &self,
        client_metadata: Option<&ClientMetadata>,
        now: DateTime<Utc>,
    ) -> Result<VerifiedClientVersion, Status> {
        // parse client version, but don't fail
        if self.expirations.is_empty() {
            let version = client_metadata.and_then(|metadata| {
                let version = metadata.version.clone()?;
                version.try_into().ok()
            });
            return Ok(VerifiedClientVersion {
                version,
                expires_at: None,
            });
        }

        let min_supported = self.min_supported(now);

        let version = client_metadata.and_then(|metadata| {
            Version::try_from(metadata.version.clone()?)
                .inspect_err(|error| {
                    error!(%error, "invalid client version");
                })
                .ok()
        });
        let Some(version) = version else {
            // A client wihtout a valid version cannot prove it is recent enough. Reject it once
            // some version has expired.
            if let Some(min_supported) = min_supported {
                return Err(failed_version_precondition(
                    "missing required client version",
                    None,
                    min_supported,
                ));
            } else {
                return Ok(VerifiedClientVersion {
                    version: None,
                    expires_at: None,
                });
            }
        };

        match self.evaluate(&version, now) {
            VersionStatus::Ok => Ok(VerifiedClientVersion {
                version: Some(version),
                expires_at: None,
            }),
            VersionStatus::ExpiresAt(at) => Ok(VerifiedClientVersion {
                version: Some(version),
                expires_at: Some(at),
            }),
            VersionStatus::Expired => {
                let min_supported = min_supported
                    .expect("logic error: expired version implies a min_supported version");
                Err(failed_version_precondition(
                    "client version is expired",
                    Some(&version),
                    min_supported,
                ))
            }
        }
    }
}

/// Compares two versions (major, minor, patch) ignoring pre-release and build metadata.
fn release_cmp(a: &Version, b: &Version) -> std::cmp::Ordering {
    (a.major, a.minor, a.patch).cmp(&(b.major, b.minor, b.patch))
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

    use super::*;

    fn mock_client_metadata(major: u64, minor: u64, patch: u64) -> ClientMetadata {
        ClientMetadata {
            version: Some(VersionProto {
                major,
                minor,
                patch,
                pre: Default::default(),
                build_number: 0,
                commit_hash: Default::default(),
            }),
        }
    }

    fn now() -> DateTime<Utc> {
        "2026-07-15T12:00:00Z".parse().unwrap()
    }

    fn date(s: &str) -> DateTime<Utc> {
        format!("{s}T00:00:00Z").parse().unwrap()
    }

    fn policy(entries: &[(&str, &str)]) -> VersionPolicy {
        VersionPolicy::new(
            entries
                .iter()
                .map(|(older_than, expires_on)| VersionExpiration {
                    older_than: Version::parse(older_than).unwrap(),
                    expires_on: date(expires_on),
                })
                .collect(),
        )
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
    fn empty_policy_allows_all() {
        let policy = VersionPolicy::new(Vec::new());

        let metadata = mock_client_metadata(1, 2, 3);
        let verified = policy
            .verify_client_version(Some(&metadata), now())
            .unwrap();
        assert_eq!(verified.version, Some(Version::new(1, 2, 3)));
        assert_eq!(verified.expires_at, None);

        let verified = policy.verify_client_version(None, now()).unwrap();
        assert_eq!(verified.version, None);
        assert_eq!(verified.expires_at, None);
    }

    #[test]
    fn version_equal_to_older_than_is_supported() {
        // older_than is exclusive
        let policy = policy(&[("0.19.0", "2026-07-14")]);
        let metadata = mock_client_metadata(0, 19, 0);
        let verified = policy
            .verify_client_version(Some(&metadata), now())
            .expect("0.19.0 is not older than 0.19.0");
        assert_eq!(verified.expires_at, None);
    }

    #[test]
    fn expired_version_is_blocked() {
        let policy = policy(&[("0.19.0", "2026-07-14")]);
        let metadata = mock_client_metadata(0, 18, 2);
        let status = policy
            .verify_client_version(Some(&metadata), now())
            .expect_err("0.18.2 expired a day ago");
        assert!(
            check_version_unsupported_status(&status),
            "Status details must indicate VersionUnsupported"
        );
        assert!(status.message().contains("expired"));
    }

    #[test]
    fn expiry_time_boundary() {
        // a version expires strictly after expires_on
        let policy = policy(&[("0.19.0", "2026-07-15")]);
        let metadata = mock_client_metadata(0, 18, 0);

        let at_expiry = date("2026-07-15");
        let verified = policy
            .verify_client_version(Some(&metadata), at_expiry)
            .expect("not yet expired at the expiry time itself");
        assert_eq!(verified.expires_at, Some(at_expiry));

        let after_expiry = at_expiry + chrono::Duration::seconds(1);
        let result = policy.verify_client_version(Some(&metadata), after_expiry);
        assert!(result.is_err());
    }

    #[test]
    fn soon_expiring_version_reports_date() {
        let policy = policy(&[("0.20.0", "2026-07-25")]);
        let metadata = mock_client_metadata(0, 19, 0);
        let verified = policy
            .verify_client_version(Some(&metadata), now())
            .unwrap();
        assert_eq!(verified.expires_at, Some(date("2026-07-25")));
    }

    #[test]
    fn nearest_entry_governs() {
        let policy = policy(&[("0.19.0", "2026-07-20"), ("0.20.0", "2026-07-25")]);

        let metadata = mock_client_metadata(0, 18, 0);
        let verified = policy
            .verify_client_version(Some(&metadata), now())
            .unwrap();
        assert_eq!(verified.expires_at, Some(date("2026-07-20")));

        let metadata = mock_client_metadata(0, 19, 5);
        let verified = policy
            .verify_client_version(Some(&metadata), now())
            .unwrap();
        assert_eq!(verified.expires_at, Some(date("2026-07-25")));

        // equal to the first entry's older_than, so governed by the second
        let metadata = mock_client_metadata(0, 19, 0);
        let verified = policy
            .verify_client_version(Some(&metadata), now())
            .unwrap();
        assert_eq!(verified.expires_at, Some(date("2026-07-25")));
    }

    #[test]
    fn prerelease_counts_as_its_release() {
        let policy = policy(&[("0.20.0", "2026-07-14")]);
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
        let result = policy.verify_client_version(Some(&metadata), now());
        assert!(result.is_ok(), "0.20.0-dev must count as 0.20.0");
    }

    #[test]
    fn missing_version_is_blocked_only_after_first_expiry() {
        let no_version = ClientMetadata { version: None };

        // nothing expired yet
        let lenient = policy(&[("0.20.0", "2026-07-25")]);
        assert!(lenient.verify_client_version(None, now()).is_ok());
        assert!(
            lenient
                .verify_client_version(Some(&no_version), now())
                .is_ok()
        );

        // something expired
        let strict = policy(&[("0.20.0", "2026-07-14")]);
        let status = strict
            .verify_client_version(None, now())
            .expect_err("missing metadata cannot prove a recent version");
        assert!(check_version_unsupported_status(&status));
        assert!(status.message().contains("missing"));

        let status = strict
            .verify_client_version(Some(&no_version), now())
            .expect_err("missing version cannot prove a recent version");
        assert!(check_version_unsupported_status(&status));
    }

    #[test]
    fn error_details_report_min_supported() {
        let policy = policy(&[("0.19.0", "2026-06-15"), ("0.20.0", "2026-07-14")]);
        let metadata = mock_client_metadata(0, 18, 0);
        let status = policy
            .verify_client_version(Some(&metadata), now())
            .unwrap_err();
        let details = StatusDetails::from_status(&status).unwrap();
        let Some(Detail::VersionUnsupported(detail)) = details.detail else {
            panic!("expected VersionUnsupported detail");
        };
        assert_eq!(detail.client_version.as_deref(), Some("0.18.0"));
        assert_eq!(detail.client_version_requirement, ">=0.20.0");
    }
}
