// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;

use aircommon::crypto::signatures::signable::Verifiable;
use prost::{
    DecodeError,
    bytes::{Buf, Bytes},
    encoding::{DecodeContext, WireType, decode_key, decode_varint, skip_field},
};
use prost::{Message, bytes::BufMut};

/// A wrapper around a request that has a signed payload
///
/// Cannot be constructed directly. It is decoded from protobuf message bytes of `T`. During
/// decoding of `T` it extracts the payload and signature bytes identified by the corresponding
/// `TAG` and `SIGNATURE_TAG` constants.
///
/// When `T` implements `VerifiableRequest` the extracted payload bytes are verified against the
/// extracted signature.
#[derive(Default)]
pub struct SignedRequest<T, const TAG: u32 = 1> {
    pub(crate) request: T,
    payload_bytes: Vec<u8>,
}

impl<T: fmt::Debug, const TAG: u32> fmt::Debug for SignedRequest<T, TAG> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SignedRequest")
            .field("request", &self.request)
            .field("payload_bytes", &self.payload_bytes.len())
            .finish()
    }
}

impl<T, const TAG: u32> SignedRequest<T, TAG> {
    pub fn new(request: T, payload_bytes: Vec<u8>) -> Self {
        Self {
            request,
            payload_bytes,
        }
    }

    pub fn inner(&self) -> &T {
        &self.request
    }

    pub fn into_inner(self) -> T {
        self.request
    }
}

impl<T, const TAG: u32> Message for SignedRequest<T, TAG>
where
    T: Message,
{
    fn encode_raw(&self, buf: &mut impl BufMut) {
        self.request.encode_raw(buf)
    }

    fn encoded_len(&self) -> usize {
        self.request.encoded_len()
    }

    fn merge_field(
        &mut self,
        tag: u32,
        wire_type: WireType,
        buf: &mut impl prost::bytes::Buf,
        ctx: DecodeContext,
    ) -> Result<(), prost::DecodeError> {
        self.request.merge_field(tag, wire_type, buf, ctx)
    }

    // `Message::decode` funnels through `merge`, so this is the single place we snapshot the
    // message buffer and extract the requested field before handing the bytes to `inner`.
    fn merge(&mut self, mut buf: impl Buf) -> Result<(), DecodeError>
    where
        Self: Sized,
    {
        let raw = buf.copy_to_bytes(buf.remaining());
        self.payload_bytes = extract_field(&raw, TAG)?.into();
        self.request.merge(raw)?;
        Ok(())
    }

    fn clear(&mut self) {
        self.request.clear();
        self.payload_bytes.clear();
    }
}

/// Scan a protobuf message buffer and return a zero-copy slice of the value bytes of the first
/// field matching `tag`. Returns an empty `Bytes` if the field is absent.
fn extract_field(raw: &Bytes, tag: u32) -> Result<Bytes, DecodeError> {
    let mut cur = raw.clone();
    while cur.has_remaining() {
        let (field_tag, wire_type) = decode_key(&mut cur)?;
        // Let `skip_field` validate and advance past the value; it returns a proper `DecodeError`
        // on a malformed/overlong field.
        let value_start = raw.len() - cur.remaining();
        skip_field(wire_type, field_tag, &mut cur, DecodeContext::default())?;
        if field_tag != tag {
            continue;
        }
        let value_end = raw.len() - cur.remaining();
        let span = raw.slice(value_start..value_end);
        return Ok(match wire_type {
            // Strip the length prefix; keep just the value content.
            WireType::LengthDelimited => {
                let mut prefix = span.clone();
                let len = decode_varint(&mut prefix)? as usize;
                span.slice(span.len() - len..)
            }
            _ => span,
        });
    }
    Ok(Bytes::new())
}

/// A request that contains a signature a can be verified using it
pub trait VerifiableRequest: fmt::Debug {
    fn signature(&self) -> Option<&crate::common::v1::Signature>;

    fn label(&self) -> &str;
}

// Any `VerifiableRequest` request wrapped by `SignedRequest` can be verified
impl<T, const TAG: u32> Verifiable for SignedRequest<T, TAG>
where
    T: VerifiableRequest,
{
    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        // TODO: avoid cloning
        Ok(self.payload_bytes.to_vec())
    }

    fn signature(&self) -> impl AsRef<[u8]> {
        self.request
            .signature()
            .map(|s| s.value.as_slice())
            .unwrap_or_default()
    }

    fn label(&self) -> &str {
        self.request.label()
    }
}

#[derive(Default)]
pub struct Seal;

/// Bundles a payload type with a request type via signing and verification.
///
/// Request is constructed by signing the payload. Payload is extracted from the `SignedRequest<T>`
/// which wraps the request via signature verification of the payload. The raw bytes of the payload
/// and the signature are extracted from from raw bytes of the request during protobuf decoding of
/// `SignedRequest<T>`.
///
/// * `request` is the type containing the signed payload and the signature.
/// * `payload` is the type which is signed.
/// * `key_type` the key used for signing and verification.
/// * `label` is the label of the payload prepended when signing.
/// * `seal` is the seal type used to seal the implementation.
macro_rules! impl_signed_payload {
    // Default: signature lives in the top-level `signature` field.
    {
        request = $request:ty,
        payload = $payload:ty,
        key_type = $key_type:ty,
        label = $label:expr,
        seal = $seal:ty $(,)?
    } => {
        $crate::signed::impl_signed_payload! {
            request = $request,
            payload = $payload,
            key_type = $key_type,
            label = $label,
            signature = |request: &$request| request.signature.as_ref(),
            seal = $seal,
        }
    };

    // Explicit signature accessor.
    {
        request = $request:ty,
        payload = $payload:ty,
        key_type = $key_type:ty,
        label = $label:expr,
        signature = $signature:expr,
        seal = $seal:ty $(,)?
    } => {
        #[allow(clippy::needless_update)]
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

        impl<const TAG: u32>
            ::aircommon::crypto::signatures::signable::VerifiedStruct<
                $crate::signed::SignedRequest<$request, TAG>,
            > for $payload
        {
            type SealingType = $seal;

            fn from_verifiable(
                verifiable: crate::signed::SignedRequest<$request, TAG>,
                _seal: Self::SealingType
            ) -> Self {
                verifiable.request.payload.unwrap_or_default()
            }
        }

        impl $crate::signed::VerifiableRequest for $request {
            fn signature(&self) -> Option<&$crate::common::v1::Signature> {
                let accessor: fn(&$request) -> Option<&$crate::common::v1::Signature> = $signature;
                accessor(self)
            }

            fn label(&self) -> &str {
                $label
            }
        }
    };
}

pub(crate) use impl_signed_payload;
