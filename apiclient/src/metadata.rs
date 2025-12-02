// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::LazyLock;

use airprotos::common::v1::{ClientMetadata, Platform, ReleaseChannel};

shadow_rs::shadow!(build);

pub(super) static METADATA: LazyLock<ClientMetadata> = LazyLock::new(|| {
    let mut version = semver::Version::parse(build::PKG_VERSION).unwrap();
    if build::GIT_CLEAN {
        version.build = semver::BuildMetadata::new(&format!(
            "{}.{}",
            build::COMMITS_SINCE_TAG,
            build::COMMIT_HASH.get(..8).unwrap()
        ))
        .unwrap();
    } else {
        version.pre = semver::Prerelease::new("dev").unwrap();
        version.build = semver::BuildMetadata::new(&format!(
            "{}.{}",
            build::COMMITS_SINCE_TAG,
            build::COMMIT_HASH.get(..8).unwrap()
        ))
        .unwrap();
    }

    let channel = if build::GIT_CLEAN && build::BRANCH == "main" {
        ReleaseChannel::Stable
    } else {
        ReleaseChannel::Dev
    };

    ClientMetadata {
        version: version.to_string(),
        platform: PLATFORM.into(),
        channel: channel.into(),
    }
});

#[cfg(target_os = "android")]
const PLATFORM: Platform = Platform::Android;

#[cfg(target_os = "ios")]
const PLATFORM: Platform = Platform::Ios;

#[cfg(target_os = "macos")]
const PLATFORM: Platform = Platform::Macos;

#[cfg(target_os = "linux")]
const PLATFORM: Platform = Platform::Linux;

#[cfg(target_os = "windows")]
const PLATFORM: Platform = Platform::Windows;
