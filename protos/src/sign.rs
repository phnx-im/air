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
macro_rules! impl_signed_payload {
    ($request:ty, $payload:ty, $key_type:ty, $label:expr) => {
        impl ::aircommon::crypto::signatures::signable::SignedStruct<$payload, $key_type>
            for $request
        {
            fn from_payload(
                payload: $payload,
                signature: ::aircommon::crypto::signatures::signable::Signature<$key_type>,
            ) -> Self {
                Self {
                    payload: Some(payload),
                    signature: Some(signature.into()),
                    ..Default::default()
                }
            }
        }

        impl ::aircommon::crypto::signatures::signable::Signable for $payload {
            type SignedOutput = $request;

            fn unsigned_payload(&self) -> Result<Vec<u8>, ::tls_codec::Error> {
                use ::prost::Message;
                Ok(self.encode_to_vec())
            }

            fn label(&self) -> &str {
                $label
            }
        }

        impl ::aircommon::crypto::signatures::signable::VerifiedStruct<$request> for $payload {
            type SealingType = $crate::sign::Seal;

            fn from_verifiable(verifiable: $request, _seal: Self::SealingType) -> Self {
                verifiable.payload.unwrap()
            }
        }

        impl ::aircommon::crypto::signatures::signable::Verifiable for $request {
            fn unsigned_payload(&self) -> Result<Vec<u8>, ::tls_codec::Error> {
                use ::prost::Message;
                Ok(self
                    .payload
                    .as_ref()
                    .ok_or($crate::sign::MissingPayloadError)?
                    .encode_to_vec())
            }

            fn signature(&self) -> impl AsRef<[u8]> {
                self.signature
                    .as_ref()
                    .map(|s| s.value.as_slice())
                    .unwrap_or_default()
            }

            fn label(&self) -> &str {
                $label
            }
        }
    };
}

pub(crate) use impl_signed_payload;
