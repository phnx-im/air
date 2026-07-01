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
/// Can be constructed directly, but usually it is decoded from protobuf message bytes of `T`.
/// During decoding of `T` it extracts the payload bytes identified by the `TAG` parameter.
///
/// When `T` implements `VerifiableRequest` the extracted payload bytes are verified against the
/// extracted signature.
///
/// The captured `payload_bytes` are exactly the bytes prost decodes into `request`'s payload field:
/// decoding rejects a message that carries the payload field more than once, so the verified bytes
/// and the payload the handler acts on can never diverge. `payload_bytes` stays `None` unless the
/// value was captured from the wire during protobuf decoding (or supplied via
/// [`SignedRequest::new`]); verification fails closed in that case rather than verifying an empty
/// payload.
#[derive(Default)]
pub struct SignedRequest<T, const TAG: u32 = 1> {
    pub(crate) request: T,
    payload_bytes: Option<Bytes>,
}

impl<T: fmt::Debug, const TAG: u32> fmt::Debug for SignedRequest<T, TAG> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SignedRequest")
            .field("request", &self.request)
            .field(
                "payload_bytes",
                &self.payload_bytes.as_ref().map(|b| b.len()),
            )
            .finish()
    }
}

impl<T, const TAG: u32> SignedRequest<T, TAG> {
    pub fn new(request: T, payload_bytes: Bytes) -> Self {
        Self {
            request,
            payload_bytes: Some(payload_bytes),
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
    fn merge(&mut self, mut buf: impl Buf) -> Result<(), DecodeError> {
        let raw = buf.copy_to_bytes(buf.remaining());
        if let Some(payload_bytes) = extract_field(&raw, TAG)? {
            // A repeated `merge` (e.g. concatenated buffers) carrying a second payload is the same
            // tampering vector `extract_field` rejects within a single buffer: reject it too.
            if self.payload_bytes.is_some() {
                return Err(duplicate_payload_error());
            }
            self.payload_bytes = Some(payload_bytes);
        }
        self.request.merge(raw)?;
        Ok(())
    }

    fn clear(&mut self) {
        self.request.clear();
        self.payload_bytes = None;
    }
}

#[expect(
    deprecated,
    reason = "we are forced to use DecodeError and there is no other way to construct it"
)]
fn duplicate_payload_error() -> DecodeError {
    DecodeError::new("duplicate signed payload field")
}

/// Scan a protobuf message buffer and return a zero-copy slice of the value bytes of the field
/// matching `tag`. Returns `None` if the field is absent.
///
/// The whole buffer is scanned so that a message carrying the field more than once is rejected
/// with a `DecodeError`. This is important to make sure that the payload bytes the signature
/// verifies against are identical to the payload prost decodes.
fn extract_field(raw: &Bytes, tag: u32) -> Result<Option<Bytes>, DecodeError> {
    let mut cur = raw.clone();
    let mut found = None;
    while cur.has_remaining() {
        let (field_tag, wire_type) = decode_key(&mut cur)?;
        // Let `skip_field` validate and advance past the value; it returns a proper `DecodeError`
        // on a malformed/overlong field.
        let value_start = raw.len() - cur.remaining();
        skip_field(wire_type, field_tag, &mut cur, DecodeContext::default())?;
        if field_tag != tag {
            continue;
        }
        if found.is_some() {
            return Err(duplicate_payload_error());
        }
        let value_end = raw.len() - cur.remaining();
        let span = raw.slice(value_start..value_end);
        found = Some(match wire_type {
            // Strip the length prefix; keep just the value content.
            WireType::LengthDelimited => {
                let mut prefix = span.clone();
                let len = decode_varint(&mut prefix)? as usize;
                span.slice(span.len().saturating_sub(len)..)
            }
            _ => span,
        });
    }
    Ok(found)
}

/// A request that contains a signature and can be verified using it
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
        // A `SignedRequest` whose payload bytes were never captured from the wire (absent payload
        // field, or constructed without decoding) must not verify against an empty payload.
        //
        // TODO: avoid cloning?
        self.payload_bytes
            .as_ref()
            .map(|b| b.to_vec())
            .ok_or_else(|| tls_codec::Error::EncodingError("missing signed payload".to_owned()))
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

#[cfg(test)]
mod tests {
    use prost::{
        Message,
        encoding::{WireType, encode_key, encode_varint},
    };

    use crate::common;

    use super::*;

    #[derive(Clone, PartialEq, Message)]
    struct TestPayload {
        #[prost(string, tag = "1")]
        name: String,
        #[prost(uint64, tag = "2")]
        value: u64,
    }

    /// Signed payload at the default tag 1
    #[derive(Clone, PartialEq, Message)]
    struct TestRequest {
        #[prost(message, optional, tag = "1")]
        payload: Option<TestPayload>,
    }

    /// Signed payload at tag 2
    #[derive(Clone, PartialEq, Message)]
    struct TestRequestTag2 {
        #[prost(message, optional, tag = "1")]
        other: Option<TestPayload>,
        #[prost(message, optional, tag = "2")]
        payload: Option<TestPayload>,
    }

    impl VerifiableRequest for TestRequest {
        fn signature(&self) -> Option<&common::v1::Signature> {
            None
        }

        fn label(&self) -> &str {
            "test"
        }
    }

    fn payload() -> TestPayload {
        TestPayload {
            name: "Ellie".to_owned(),
            value: 7,
        }
    }

    /// Manually frame a length-delimited field so a tag can be repeated on the wire.
    fn push_len_delimited_field(buf: &mut Vec<u8>, tag: u32, value: &[u8]) {
        encode_key(tag, WireType::LengthDelimited, buf);
        encode_varint(value.len() as u64, buf);
        buf.extend_from_slice(value);
    }

    #[test]
    fn extract_field_returns_value_content_without_length_prefix() {
        let payload_bytes = payload().encode_to_vec();
        let request_bytes = TestRequest {
            payload: Some(payload()),
        }
        .encode_to_vec();

        let extracted = extract_field(&Bytes::from(request_bytes), 1).unwrap();
        assert_eq!(extracted.as_deref(), Some(payload_bytes.as_slice()));
    }

    #[test]
    fn extract_field_returns_none_when_absent() {
        let request_bytes = TestRequest { payload: None }.encode_to_vec();
        let extracted = extract_field(&Bytes::from(request_bytes), 1).unwrap();
        assert_eq!(extracted, None);
    }

    #[test]
    fn extract_field_rejects_duplicate_tag() {
        let payload_bytes = payload().encode_to_vec();
        let mut buf = Vec::new();
        push_len_delimited_field(&mut buf, 1, &payload_bytes);
        push_len_delimited_field(&mut buf, 1, &payload_bytes);

        assert!(extract_field(&Bytes::from(buf), 1).is_err());
    }

    #[test]
    fn decode_captures_payload_bytes_matching_inner_payload() {
        let payload_bytes = payload().encode_to_vec();
        let request_bytes = TestRequest {
            payload: Some(payload()),
        }
        .encode_to_vec();

        let signed: SignedRequest<TestRequest> = Message::decode(request_bytes.as_slice()).unwrap();
        assert_eq!(
            signed.payload_bytes.as_deref(),
            Some(payload_bytes.as_slice())
        );
        assert_eq!(signed.inner().payload, Some(payload()));
        assert_eq!(
            signed.unsigned_payload().unwrap(),
            signed.inner().payload.as_ref().unwrap().encode_to_vec()
        );
    }

    #[test]
    fn decode_respects_custom_tag() {
        let payload_bytes = payload().encode_to_vec();
        let request_bytes = TestRequestTag2 {
            other: Some(TestPayload {
                name: "decoy".to_owned(),
                value: 99,
            }),
            payload: Some(payload()),
        }
        .encode_to_vec();

        let signed: SignedRequest<TestRequestTag2, 2> =
            Message::decode(request_bytes.as_slice()).unwrap();
        assert_eq!(
            signed.payload_bytes.as_deref(),
            Some(payload_bytes.as_slice())
        );
    }

    #[test]
    fn decode_rejects_duplicate_payload_field() {
        // A valid request plus an appended second payload field that prost would otherwise merge.
        let tampered = TestPayload {
            name: "Mallory".to_owned(),
            value: 1,
        }
        .encode_to_vec();
        let mut buf = Vec::new();
        push_len_delimited_field(&mut buf, 1, &payload().encode_to_vec());
        push_len_delimited_field(&mut buf, 1, &tampered);

        assert!(<SignedRequest<TestRequest>>::decode(buf.as_slice()).is_err());
    }

    #[test]
    fn unsigned_payload_fails_closed_when_not_captured() {
        // Built via `Default` (e.g. never decoded from the wire): must not verify empty bytes.
        let signed = <SignedRequest<TestRequest>>::default();
        assert!(signed.unsigned_payload().is_err());
    }

    #[test]
    fn decode_without_payload_field_fails_closed() {
        let request_bytes = TestRequest { payload: None }.encode_to_vec();
        let signed = <SignedRequest<TestRequest>>::decode(request_bytes.as_slice()).unwrap();
        assert!(signed.unsigned_payload().is_err());
    }

    #[test]
    fn new_captures_supplied_payload_bytes() {
        let signed =
            SignedRequest::<TestRequest>::new(TestRequest { payload: None }, vec![1, 2, 3].into());
        assert_eq!(signed.unsigned_payload().unwrap(), vec![1, 2, 3]);
    }
}
