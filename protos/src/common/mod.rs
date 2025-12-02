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
            version: "0.1.0+100".to_owned(),
            platform: v1::Platform::Android.into(),
            channel: v1::ReleaseChannel::Stable.into(),
        };
        let bytes = metadata.encode_to_vec();
        assert_eq!(bytes.len(), 15);
    }
}
