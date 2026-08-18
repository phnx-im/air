// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Deterministic derivation of Privacy Pass nonces and blinding scalars.
//!
//! Every device of a user derives the same batch from the same token seed, so
//! all devices send byte-identical token requests and finalize byte-identical
//! tokens. The byte layout below is therefore a cross-device and
//! cross-version protocol invariant, not an implementation detail: the `v1` in
//! the labels is the version, and any change to the chain (primitive, framing,
//! widths, reduction, retry rule) has to bump it.
//!
//! ```text
//! batch_seed = Expand(PRK = token_seed, info = BatchInfo, L = 32)
//!
//! nonce_i = Expand(PRK = batch_seed, info = NonceInfo(i), L = 32)
//!
//! blind_i: for r = 0, 1, 2, ...:
//!     wide = Expand(PRK = batch_seed, info = BlindInfo(i, r), L = 64)
//!     s = Scalar::from_bytes_mod_order_wide(wide)
//!     if s != 0 { blind_i = s; break }
//! ```
//!
//! Each info is the TLS serialization of the struct of the same name below.
//! All fields are fixed-size, so the encoding is the plain concatenation of
//! the label bytes and the big-endian integers.
//!
//! HKDF-SHA256 is used in expand-only mode: seeds are uniformly random by
//! construction, so extract would add nothing. The `hkdf` crate is used
//! directly rather than the `KdfDerivable` machinery in
//! `common/src/crypto/kdf/`, so that the layout is pinned here and not by
//! trait-level info framing.

use hkdf::Hkdf;
use privacypass::{common::private::Scalar, private_tokens::Ristretto255};
use sha2::Sha256;
use tls_codec::{Serialize, TlsSerialize, TlsSize};

/// The blinding scalar type, curve25519-dalek's `Scalar` behind the alias
/// privacypass exports. Going through the alias keeps the curve crate out of
/// our dependencies and the two types provably unified.
pub(crate) type Blind = Scalar<Ristretto255>;

/// Length of a token seed, a batch seed and a nonce.
pub(crate) const SEED_LEN: usize = 32;

/// Length of the wide bytes reduced into a blinding scalar.
const WIDE_LEN: usize = 64;

const LABEL_LEN: usize = 15;

const BATCH_LABEL: [u8; LABEL_LEN] = *b"air-pp-v1 batch";
const NONCE_LABEL: [u8; LABEL_LEN] = *b"air-pp-v1 nonce";
const BLIND_LABEL: [u8; LABEL_LEN] = *b"air-pp-v1 blind";

/// Info of a batch seed derivation.
#[derive(TlsSize, TlsSerialize)]
struct BatchInfo {
    label: [u8; LABEL_LEN],
    operation_type: u32,
    key_fingerprint: [u8; 32],
    allowance_epoch: u32,
}

/// Info of a nonce derivation.
#[derive(TlsSize, TlsSerialize)]
struct NonceInfo {
    label: [u8; LABEL_LEN],
    index: u32,
}

/// Info of a blinding scalar derivation.
#[derive(TlsSize, TlsSerialize)]
struct BlindInfo {
    label: [u8; LABEL_LEN],
    index: u32,
    retry: u32,
}

/// Seed of one batch, derived from the token seed of a (operation type, key).
///
/// `operation_type` is the proto enum value, `key_fingerprint` the SHA-256 of
/// the serialized VOPRF public key.
pub(crate) fn batch_seed(
    token_seed: &[u8; SEED_LEN],
    operation_type: u32,
    key_fingerprint: &[u8; 32],
    allowance_epoch: u32,
) -> [u8; SEED_LEN] {
    let info = BatchInfo {
        label: BATCH_LABEL,
        operation_type,
        key_fingerprint: *key_fingerprint,
        allowance_epoch,
    };
    expand(token_seed, &info)
}

/// Nonce of the token at index `index` in the batch.
pub(crate) fn nonce(batch_seed: &[u8; SEED_LEN], index: u32) -> [u8; SEED_LEN] {
    let info = NonceInfo {
        label: NONCE_LABEL,
        index,
    };
    expand(batch_seed, &info)
}

/// Blinding scalar of the token at index `index` in the batch.
///
/// The retry counter is part of the info and starts at 0, so the common case
/// costs nothing extra and the astronomically unlikely retry stays identical
/// across devices.
pub(crate) fn blind(batch_seed: &[u8; SEED_LEN], index: u32) -> Blind {
    let mut retry: u32 = 0;
    loop {
        let info = BlindInfo {
            label: BLIND_LABEL,
            index,
            retry,
        };
        let wide: [u8; WIDE_LEN] = expand(batch_seed, &info);
        let scalar = Blind::from_bytes_mod_order_wide(&wide);
        if scalar != Blind::ZERO {
            return scalar;
        }
        retry += 1;
    }
}

/// HKDF-SHA256 expand with the seed used directly as the PRK.
fn expand<const L: usize>(prk: &[u8; SEED_LEN], info: &impl Serialize) -> [u8; L] {
    const { assert!(L <= 255 * 32, "HKDF-SHA256 expands to at most 255 blocks") }

    let info = info
        .tls_serialize_detached()
        .expect("serializing fixed-size fields into a vec cannot fail");
    let hkdf =
        Hkdf::<Sha256>::from_prk(prk).expect("a seed is 32 bytes, exactly one SHA-256 block");
    let mut out = [0u8; L];
    hkdf.expand(&info, &mut out)
        .expect("the output length is checked at compile time");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: [u8; SEED_LEN] = [7u8; SEED_LEN];
    const FINGERPRINT: [u8; 32] = [9u8; 32];

    /// Derivation is a pure function of its inputs.
    #[test]
    fn derivation_is_deterministic() {
        let first = batch_seed(&SEED, 1, &FINGERPRINT, 679);
        let second = batch_seed(&SEED, 1, &FINGERPRINT, 679);
        assert_eq!(first, second);

        assert_eq!(nonce(&first, 3), nonce(&second, 3));
        assert_eq!(blind(&first, 3), blind(&second, 3));
    }

    /// Every field of the batch info separates the derived batch seeds.
    #[test]
    fn batch_seed_separates_contexts() {
        let base = batch_seed(&SEED, 1, &FINGERPRINT, 679);

        assert_ne!(base, batch_seed(&[8u8; SEED_LEN], 1, &FINGERPRINT, 679));
        assert_ne!(base, batch_seed(&SEED, 2, &FINGERPRINT, 679));
        assert_ne!(base, batch_seed(&SEED, 1, &[10u8; 32], 679));
        assert_ne!(base, batch_seed(&SEED, 1, &FINGERPRINT, 680));
    }

    /// Indices separate nonces and blinds within a batch.
    #[test]
    fn index_separates_batch_members() {
        let batch = batch_seed(&SEED, 1, &FINGERPRINT, 679);

        assert_ne!(nonce(&batch, 0), nonce(&batch, 1));
        assert_ne!(blind(&batch, 0), blind(&batch, 1));
        // Nonce and blind of the same index come from different labels.
        assert_ne!(nonce(&batch, 0), blind(&batch, 0).to_bytes());
    }
}
