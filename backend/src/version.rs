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
        .ok_or_else(|| Status::failed_precondition("missing client version"))?;
    let client_version: semver::Version = client_version.try_into().map_err(|error| {
        error!(%error, "invalid client version");
        Status::failed_precondition("invalid client version")
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
