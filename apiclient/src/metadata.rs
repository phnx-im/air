// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::LazyLock;

use airprotos::common::{self, v1::Version};

// Collected by `build.rs`. Note that these are only refreshed when the build
// script reruns, see the comment there.
pub(super) static VERSION: LazyLock<Version> = LazyLock::new(|| {
    new_version(
        env!("CARGO_PKG_VERSION"),
        env!("AIR_GIT_DIRTY") == "true",
        env!("AIR_COMMIT_HASH"),
        env!("AIR_BUILD_NUMBER")
            .parse()
            .expect("invalid build number"),
    )
});

fn new_version(
    pkg_version: &str,
    git_dirty: bool,
    commit_hash: &str,
    build_number: u64,
) -> Version {
    let mut version = semver::Version::parse(pkg_version).unwrap();

    if git_dirty {
        version.pre = semver::Prerelease::new("dev").unwrap();
    }

    let commit_hash_hex = &commit_hash[0..8];
    debug_assert_eq!(commit_hash_hex.len() % 2, 0);
    let commit_hash = (0..commit_hash_hex.len())
        .step_by(2)
        .map(|idx| {
            let byte = &commit_hash_hex[idx..idx + 2];
            u8::from_str_radix(byte, 16).unwrap()
        })
        .collect();

    common::v1::Version {
        build_number,
        commit_hash,
        ..version.into()
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn version() {
        let version = super::new_version("1.2.3", false, "1234567890abcdef", 10);
        assert_eq!(
            version,
            airprotos::common::v1::Version {
                major: 1,
                minor: 2,
                patch: 3,
                pre: Default::default(),
                build_number: 10,
                commit_hash: vec![0x12, 0x34, 0x56, 0x78],
            },
        );

        let version = super::new_version("1.2.3", true, "1234567890abcdef", 10);
        assert_eq!(
            version,
            airprotos::common::v1::Version {
                major: 1,
                minor: 2,
                patch: 3,
                pre: "dev".to_owned(),
                build_number: 10,
                commit_hash: vec![0x12, 0x34, 0x56, 0x78],
            },
        );
    }
}
