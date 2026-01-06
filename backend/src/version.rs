// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airprotos::common::v1::{ClientMetadata, StatusDetails, StatusDetailsCode};
use prost::Message;
use semver::VersionReq;
use tonic::{Code, Status};
use tracing::{error, warn};

/// Verifies that the client version matches the given version requirement.
///
/// If the version requirement is not set, this function returns `Ok(())`.
///
/// If version requirement does not match, this function returns a [`Status`] with
/// [`Code::FailedPrecondition`] and [`StatusDetailsCode::VersionUnsupported`].
pub(crate) fn verify_client_version(
    client_version_req: Option<&VersionReq>,
    client_metadata: Option<&ClientMetadata>,
) -> Result<(), Status> {
    let Some(client_version_req) = client_version_req else {
        return Ok(());
    };

    let Some(client_metadata) = client_metadata else {
        warn!("missing client metadata");
        return Err(failed_version_precondition(
            "missing required client version",
        ));
    };
    let client_version = client_metadata
        .version
        .clone()
        .ok_or_else(|| failed_version_precondition("missing client version"))?;
    let client_version: semver::Version = client_version.try_into().map_err(|error| {
        error!(%error, "invalid client version");
        failed_version_precondition("invalid client version")
    })?;

    if client_version_req.matches(&client_version) {
        Ok(())
    } else {
        warn!(
            %client_version,
            %client_version_req, "client version does not match required version"
        );
        Err(failed_version_precondition(
            "client version does not match required version",
        ))
    }
}

fn failed_version_precondition(message: impl Into<String>) -> Status {
    Status::with_details(
        Code::FailedPrecondition,
        message,
        StatusDetails {
            code: StatusDetailsCode::VersionUnsupported.into(),
        }
        .encode_to_vec()
        .into(),
    )
}

#[cfg(test)]
mod tests {
    use airprotos::common::v1::Version;

    use super::*;

    fn mock_client_metadata(major: u64, minor: u64, patch: u64) -> ClientMetadata {
        let version_struct = Version {
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

    fn check_version_unsupported_status(status: &Status) -> bool {
        if status.code() != Code::FailedPrecondition {
            return false;
        }
        StatusDetails::from_status(status)
            .map(|details| details.code() == StatusDetailsCode::VersionUnsupported)
            .unwrap_or(false)
    }

    #[test]
    fn test_no_version_requirement() {
        let req = None;
        let metadata = mock_client_metadata(1, 2, 3);
        let result = verify_client_version(req, Some(&metadata));
        assert!(result.is_ok(), "Should succeed when no requirement is set");
    }

    #[test]
    fn test_version_match() {
        let req = Some(&VersionReq::parse(">=1.0.0, <2.0.0").unwrap());
        let metadata = mock_client_metadata(1, 5, 0);
        let result = verify_client_version(req, Some(&metadata));
        assert!(
            result.is_ok(),
            "Should succeed when version matches requirement"
        );
    }

    #[test]
    fn test_prerelease_version_match() {
        let req = Some(&VersionReq::parse(">=1.4.0, <2.0.0, 1.5.0-dev").unwrap());
        let metadata = ClientMetadata {
            version: Some(Version {
                major: 1,
                minor: 5,
                patch: 0,
                pre: "dev".to_owned(),
                build_number: 69,
                commit_hash: vec![0xf3, 0x22, 0x68, 0x79],
            }),
        };
        let result = verify_client_version(req, Some(&metadata));
        assert!(
            result.is_ok(),
            "Should succeed when version matches requirement"
        );
    }

    #[test]
    fn test_version_mismatch() {
        let req = Some(&VersionReq::parse("=1.x.x").unwrap());
        let metadata = mock_client_metadata(2, 0, 0);
        let result = verify_client_version(req, Some(&metadata));

        assert!(
            result.is_err(),
            "Should fail when version mismatches requirement"
        );
        let status = result.unwrap_err();
        assert!(
            check_version_unsupported_status(&status),
            "Status details must indicate VersionUnsupported"
        );
        assert_eq!(status.code(), Code::FailedPrecondition);
        assert!(status.message().contains("does not match required version"));
    }

    #[test]
    fn test_missing_client_metadata() {
        let req = Some(&VersionReq::parse(">=1.0.0").unwrap());
        let metadata = None;
        let result = verify_client_version(req, metadata);

        assert!(
            result.is_err(),
            "Should fail when client metadata is missing"
        );
        let status = result.unwrap_err();
        assert!(
            check_version_unsupported_status(&status),
            "Status details must indicate VersionUnsupported"
        );
        assert_eq!(status.code(), Code::FailedPrecondition);
        assert!(status.message().contains("missing required client version"));
    }

    #[test]
    fn test_missing_client_version_field() {
        let req = Some(&VersionReq::parse(">=1.0.0").unwrap());
        let metadata = ClientMetadata {
            version: None, // The Protobuf optional field is missing
        };
        let result = verify_client_version(req, Some(&metadata));

        assert!(
            result.is_err(),
            "Should fail when client version field is missing"
        );
        let status = result.unwrap_err();
        // The implementation uses Status::failed_precondition which doesn't include the custom details
        assert_eq!(status.code(), Code::FailedPrecondition);
        assert!(status.message().contains("missing client version"));
    }
}
