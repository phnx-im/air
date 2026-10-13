// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Log-safe pseudonyms for QS client ids.

use std::fmt;

use aircommon::identifiers::QsClientId;

use crate::bucket_key::BucketKey;

const LABEL: &[u8] = b"air qs client pseudonym";

/// Maps client ids to pseudonyms that are stable for the lifetime of the
/// process and unlinkable to the id without the key.
///
/// The key lives only in memory, so pseudonyms change on restart and differ
/// between replicas.
#[derive(Debug, Clone)]
pub(crate) struct ClientPseudonymizer(BucketKey);

impl ClientPseudonymizer {
    pub(crate) fn random() -> Self {
        Self(BucketKey::random())
    }

    pub(crate) fn pseudonym(&self, client_id: &QsClientId) -> ClientPseudonym {
        let mac = self.0.bucket(LABEL, client_id.as_uuid().as_bytes());
        let (prefix, _) = mac.split_first_chunk().expect("MAC is longer than 8 bytes");
        ClientPseudonym(*prefix)
    }
}

/// A truncated keyed hash of a client id, displayed as hex.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClientPseudonym([u8; 8]);

impl fmt::Display for ClientPseudonym {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&hex::encode(self.0))
    }
}

#[cfg(test)]
mod test {
    use uuid::Uuid;

    use super::*;

    #[test]
    fn stable_per_key() {
        let client_id = QsClientId::from(Uuid::new_v4());
        let a = ClientPseudonymizer::random();
        let b = ClientPseudonymizer::random();

        assert!(a.pseudonym(&client_id) == a.pseudonym(&client_id));
        assert!(a.pseudonym(&client_id) != b.pseudonym(&client_id));
    }
}
