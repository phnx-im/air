// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::LazyLock;

use airprotos::common::{self, v1::ClientMetadata};

shadow_rs::shadow!(build);

pub(super) static METADATA: LazyLock<ClientMetadata> = LazyLock::new(|| {
    new_metadata(
        build::PKG_VERSION,
        !build::GIT_CLEAN,
        build::COMMIT_HASH,
        build::COMMITS_SINCE_TAG,
    )
});

fn new_metadata(
    pkg_version: &str,
    git_dirty: bool,
    commit_hash: &str,
    commits_since_tag: usize,
) -> ClientMetadata {
    let mut version = semver::Version::parse(pkg_version).unwrap();

    if git_dirty {
        version.pre = semver::Prerelease::new("dev").unwrap();
    }

    let build_number = u64::try_from(commits_since_tag).unwrap();

    let commit_hash_hex = &commit_hash[0..8];
    debug_assert_eq!(commit_hash_hex.len() % 2, 0);
    let commit_hash = (0..commit_hash_hex.len())
        .step_by(2)
        .map(|idx| {
            let byte = &commit_hash_hex[idx..idx + 2];
            u8::from_str_radix(byte, 16).unwrap()
        })
        .collect();

    let proto_version = common::v1::Version {
        build_number,
        commit_hash,
        ..version.into()
    };

    ClientMetadata {
        version: Some(proto_version),
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn metadata() {
        let metadata = super::new_metadata("1.2.3", false, "1234567890abcdef", 10);
        assert_eq!(
            metadata,
            airprotos::common::v1::ClientMetadata {
                version: Some(airprotos::common::v1::Version {
                    major: 1,
                    minor: 2,
                    patch: 3,
                    pre: Default::default(),
                    build_number: 10,
                    commit_hash: vec![0x12, 0x34, 0x56, 0x78],
                }),
            }
        );

        let metadata = super::new_metadata("1.2.3", true, "1234567890abcdef", 10);
        assert_eq!(
            metadata,
            airprotos::common::v1::ClientMetadata {
                version: Some(airprotos::common::v1::Version {
                    major: 1,
                    minor: 2,
                    patch: 3,
                    pre: "dev".to_owned(),
                    build_number: 10,
                    commit_hash: vec![0x12, 0x34, 0x56, 0x78],
                }),
            }
        );
    }
}
