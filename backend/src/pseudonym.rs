// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Log-safe pseudonyms for QS client ids and DS group ids.

use std::fmt;

use aircommon::identifiers::QsClientId;
use uuid::Uuid;

use crate::bucket_key::BucketKey;

const CLIENT_LABEL: &[u8] = b"air qs client pseudonym";
const GROUP_LABEL: &[u8] = b"air ds group pseudonym";

/// Maps ids to pseudonyms that are stable for the lifetime of the process and
/// unlinkable to the id without the key.
///
/// The key lives only in memory, so pseudonyms change on restart and differ
/// between replicas.
#[derive(Debug, Clone)]
pub(crate) struct Pseudonymizer(BucketKey);

impl Pseudonymizer {
    pub(crate) fn random() -> Self {
        Self(BucketKey::random())
    }

    pub(crate) fn client(&self, client_id: &QsClientId) -> Pseudonym {
        self.pseudonym(CLIENT_LABEL, client_id.as_uuid())
    }

    pub(crate) fn group(&self, group_id: &Uuid) -> Pseudonym {
        self.pseudonym(GROUP_LABEL, group_id)
    }

    fn pseudonym(&self, label: &[u8], id: &Uuid) -> Pseudonym {
        let mac = self.0.bucket(label, id.as_bytes());
        let (prefix, _) = mac.split_first_chunk().expect("MAC is longer than 8 bytes");
        Pseudonym(*prefix)
    }
}

/// A truncated keyed hash of an id, displayed as hex.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Pseudonym([u8; 8]);

impl fmt::Display for Pseudonym {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&hex::encode(self.0))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn stable_per_key() {
        let client_id = QsClientId::from(Uuid::new_v4());
        let a = Pseudonymizer::random();
        let b = Pseudonymizer::random();

        assert!(a.client(&client_id) == a.client(&client_id));
        assert!(a.client(&client_id) != b.client(&client_id));
    }

    #[test]
    fn labels_separate_pseudonyms() {
        let id = Uuid::new_v4();
        let pseudonymizer = Pseudonymizer::random();

        assert!(pseudonymizer.client(&QsClientId::from(id)) != pseudonymizer.group(&id));
    }
}
