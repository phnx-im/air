// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

pub mod convert;
pub mod v1;

#[cfg(test)]
mod test {

    use prost::Message;

    use super::*;

    #[test]
    fn client_metadata_size_bytes() {
        let metadata = v1::ClientMetadata {
            version: Some(v1::Version {
                major: 1,
                minor: 2,
                patch: 3,
                pre: Default::default(),
                build_number: 100,
                commit_hash: [0xa1, 0xb1, 0xc1, 0xd1].to_vec(),
            }),
        };
        let bytes = metadata.encode_to_vec();
        assert_eq!(bytes.len(), 16);
    }
}
