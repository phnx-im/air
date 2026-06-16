// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

#[derive(Default)]
pub struct Seal;

pub(crate) struct MissingPayloadError;

impl From<MissingPayloadError> for tls_codec::Error {
    fn from(_: MissingPayloadError) -> Self {
        tls_codec::Error::EncodingError("missing payload".to_owned())
    }
}

/// Bundles a payload type with a request type via signing and verification.
///
/// Request is constructed by signing the payload. Payload is extracted from the request via
/// signature verification.
///
/// * `request` is the type containing the signed payload and the signature.
/// * `payload` is the type which is signed.
/// * `key_type` the key used for signing and verification.
/// * `label` is the label of the payload prepended when signing.

