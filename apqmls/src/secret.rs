// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Taken verbatim from
//! <https://github.com/openmls/openmls/blob/f0b97fc1df949a193630d40baa6ea41f0161742c/openmls/src/ciphersuite/secret.rs>

use openmls::prelude::{Ciphersuite, CryptoError, OpenMlsCrypto};
use tls_codec::{SecretVLBytes, Serialize, TlsSerialize, TlsSize};

#[derive(TlsSerialize, TlsSize)]
pub(crate) struct Secret {
    value: SecretVLBytes,
}

impl From<Vec<u8>> for Secret {
    fn from(value: Vec<u8>) -> Self {
        Self {
            value: value.into(),
        }
    }
}

impl Secret {
    pub(crate) fn as_slice(&self) -> &[u8] {
        self.value.as_slice()
    }

    /// HKDF expand where `self` is `prk`.
    pub(crate) fn hkdf_expand(
        &self,
        crypto: &impl OpenMlsCrypto,
        ciphersuite: Ciphersuite,
        info: &[u8],
        okm_len: usize,
    ) -> Result<Self, CryptoError> {
        let key = crypto
            .hkdf_expand(
                ciphersuite.hash_algorithm(),
                self.value.as_slice(),
                info,
                okm_len,
            )
            .map_err(|_| CryptoError::CryptoLibraryError)?;
        if key.as_slice().is_empty() {
            return Err(CryptoError::InvalidLength);
        }
        Ok(Self { value: key })
    }

    /// Expand a `Secret` to a new `Secret` of length `length` including a
    /// `label` and a `context`.
    pub(crate) fn kdf_expand_label(
        &self,
        crypto: &impl OpenMlsCrypto,
        ciphersuite: Ciphersuite,
        label: &str,
        context: &[u8],
        length: usize,
    ) -> Result<Secret, CryptoError> {
        let full_label = format!("MLS 1.0 {label}");
        let info = KdfLabel {
            length: length
                .try_into()
                .map_err(|_| CryptoError::KdfLabelTooLarge)?,
            label: full_label.as_bytes(),
            context,
        }
        .tls_serialize_detached()
        .map_err(|_| CryptoError::KdfSerializationError)?;
        self.hkdf_expand(crypto, ciphersuite, &info, length)
    }

    /// Derive a new `Secret` from the this one by expanding it with the given
    /// `label` and an empty `context`.
    pub(crate) fn derive_secret(
        &self,
        crypto: &impl OpenMlsCrypto,
        ciphersuite: Ciphersuite,
        label: &str,
    ) -> Result<Secret, CryptoError> {
        self.kdf_expand_label(crypto, ciphersuite, label, &[], ciphersuite.hash_length())
    }
}

/// `KdfLabel` is later serialized and used in the `label` field of
/// `kdf_expand_label`.
///
/// ```c
/// // draft-ietf-mls-protocol-16
/// struct {
///     uint16 length = Length;
///     opaque label<V> = "MLS 1.0 " + Label;
///     opaque context<V> = Context;
/// } KDFLabel;
/// ```
#[derive(TlsSerialize, TlsSize)]
struct KdfLabel<'a> {
    length: u16,
    label: &'a [u8],
    context: &'a [u8],
}
