// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::LazyLock;

use airprotos::common::{self, v1::ClientMetadata};

shadow_rs::shadow!(build);

pub(super) static METADATA: LazyLock<ClientMetadata> = LazyLock::new(|| {
    let mut version = semver::Version::parse(build::PKG_VERSION).unwrap();

    if !build::GIT_CLEAN {
        version.pre = semver::Prerelease::new("dev").unwrap();
    }

    let build_number = u64::try_from(build::COMMITS_SINCE_TAG).unwrap();
    let commit_hash = build::COMMIT_HASH.as_bytes()[0..8].to_vec();

    let proto_version = common::v1::Version {
        build_number,
        commit_hash,
        ..version.into()
    };

    ClientMetadata {
        version: Some(proto_version),
    }
});
