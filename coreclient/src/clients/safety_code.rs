// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{credentials::UserCredential, identifiers::UserId};
use anyhow::Context;
use sha2::Digest;
use tls_codec::{Serialize as _, TlsSerialize, TlsSize};

use crate::{clients::CoreUser, groups::client_auth_info::StorableUserCredential};

#[derive(Debug, Clone, Copy, PartialEq, Eq, TlsSize, TlsSerialize)]
#[repr(u8)]
enum SafetyCodeVersion {
    V1 = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SafetyCode(pub [u8; 32]);

#[derive(Debug, TlsSize, TlsSerialize)]
struct HashInput<'a> {
    version: SafetyCodeVersion,
    user_credential: &'a UserCredential,
    label: [u8; 15],
}

impl<'a> HashInput<'a> {
    fn new(version: SafetyCodeVersion, user_credential: &'a UserCredential) -> Self {
        Self {
            version,
            user_credential,
            label: *b"AIR SAFETY CODE",
        }
    }

    fn hash(&self) -> anyhow::Result<[u8; 32]> {
        let bytes = self.tls_serialize_detached()?;
        let hash = sha2::Sha256::digest(bytes);
        Ok(hash.into())
    }
}

impl SafetyCode {
    /// Computes the safety code of the given version by hashing over the
    /// version, the user credential and the static string "AIR SAFETY CODE".
    pub fn new(user_credential: &UserCredential) -> anyhow::Result<Self> {
        let hash_input = HashInput::new(SafetyCodeVersion::V1, user_credential);
        let hash = hash_input.hash()?;
        Ok(Self(hash))
    }

    /// Returns the safety code as a string of 6 chunks of 5 base-10 digits.
    pub fn to_chunks(&self) -> [u64; 6] {
        const MODULUS: u64 = 100_000;

        let mut out = [0u64; 6];

        // Operate on chunks of 5 bytes (40 bits) to produce 6 values in [0, 100_000)
        for (dst, chunk) in out.iter_mut().zip(self.0.chunks_exact(5)) {
            let mut value = 0u64;
            for &b in chunk {
                value = (value << 8) | b as u64;
            }
            *dst = value % MODULUS;
        }

        out
    }
}

impl CoreUser {
    pub async fn safety_code(&self, user_id: &UserId) -> anyhow::Result<SafetyCode> {
        let user_credential =
            StorableUserCredential::load_by_user_id(self.db().read().await?, user_id)
                .await?
                .context("Can't find user credential of given user")?;
        SafetyCode::new(&user_credential)
    }
}
