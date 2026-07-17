// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::borrow::Cow;

use airmacros::{
    DeserializeTaggedMap, DeserializeTaggedUnion, SerializeTaggedMap, SerializeTaggedUnion,
};
use minicbor_serde::{from_slice, to_vec};

// Helpers

fn cbor_roundtrip<T>(value: &T) -> T
where
    T: serde::Serialize + for<'de> serde::Deserialize<'de>,
{
    let buf = to_vec(value).expect("serialize");
    from_slice(&buf).expect("deserialize")
}

/// Returns the number of entries in the top-level CBOR map.
fn cbor_map_len(value: &impl serde::Serialize) -> u64 {
    let mut buf = Vec::new();
    let mut serializer = minicbor_serde::Serializer::new(&mut buf);
    value.serialize(&mut serializer).expect("serialize");

    let mut decoder = minicbor::Decoder::new(&buf);
    decoder
        .map()
        .expect("expected CBOR map")
        .unwrap_or_default()
}

/// Serializes `value` to a CBOR buffer.
fn cbor_bytes(value: &impl serde::Serialize) -> Vec<u8> {
    to_vec(value).expect("serialize")
}

// A small tagged-map struct used as a union payload.

#[derive(Debug, Clone, PartialEq, SerializeTaggedMap, DeserializeTaggedMap)]
struct Inner {
    #[tag(1)]
    id: u32,
    #[tag(2)]
    name: String,
}

// A multi-variant union with a struct, a Vec<u8> and a String payload, plus #[unknown].

// `Default` is required to use `Payload` as a field of a `SerializeTaggedMap` struct (test 8/9).
#[derive(Debug, Clone, PartialEq, Default, SerializeTaggedUnion, DeserializeTaggedUnion)]
enum Payload {
    #[tag(1)]
    Struct(Inner),
    #[tag(2)]
    Blob(Vec<u8>),
    #[tag(3)]
    Text(String),
    #[unknown]
    #[default]
    Unknown,
}

// 1. Roundtrip of each payload kind.

#[test]
fn roundtrip_struct_payload() {
    let orig = Payload::Struct(Inner {
        id: 7,
        name: "hello".into(),
    });
    assert_eq!(cbor_roundtrip(&orig), orig);
}

#[test]
fn roundtrip_blob_payload() {
    let orig = Payload::Blob(vec![1, 2, 3, 4]);
    assert_eq!(cbor_roundtrip(&orig), orig);
}

#[test]
fn roundtrip_text_payload() {
    let orig = Payload::Text("world".into());
    assert_eq!(cbor_roundtrip(&orig), orig);
}

// 2. Wire shape: single-entry map with the integer tag as key.

#[test]
fn wire_shape_is_single_entry_map() {
    let value = Payload::Text("x".into());
    assert_eq!(cbor_map_len(&value), 1);

    let buf = cbor_bytes(&value);
    let mut decoder = minicbor::Decoder::new(&buf);
    assert_eq!(decoder.map().expect("map"), Some(1));
    assert_eq!(decoder.u32().expect("key"), 3); // Text has #[tag(3)]
}

// 3. Vec<u8> payload is encoded as a CBOR byte string.

#[test]
fn blob_payload_is_byte_string() {
    let value = Payload::Blob(vec![0xde, 0xad, 0xbe, 0xef]);
    let buf = cbor_bytes(&value);

    let mut decoder = minicbor::Decoder::new(&buf);
    assert_eq!(decoder.map().expect("map"), Some(1));
    assert_eq!(decoder.u32().expect("key"), 2); // Blob has #[tag(2)]
    assert_eq!(decoder.bytes().expect("bytes"), &[0xde, 0xad, 0xbe, 0xef]);
}

// 4. Unknown-tag decode maps to the #[unknown] variant when declared.

/// A "newer" enum with an extra variant unknown to `Payload`.
#[derive(Debug, Clone, PartialEq, SerializeTaggedUnion, DeserializeTaggedUnion)]
enum PayloadV2 {
    #[tag(1)]
    Struct(Inner),
    #[tag(2)]
    Blob(Vec<u8>),
    #[tag(3)]
    Text(String),
    #[tag(4)]
    Number(u64),
    #[unknown]
    Unknown,
}

#[test]
fn unknown_tag_decodes_to_unknown_variant() {
    let newer = PayloadV2::Number(123);
    let buf = cbor_bytes(&newer);
    let older: Payload = from_slice(&buf).expect("deserialize");
    assert_eq!(older, Payload::Unknown);
}

// 5. Unknown-tag decode errors when no #[unknown] variant is declared.

#[derive(Debug, Clone, PartialEq, SerializeTaggedUnion, DeserializeTaggedUnion)]
enum Closed {
    #[tag(1)]
    Text(String),
}

#[test]
fn unknown_tag_errors_without_unknown_variant() {
    let newer = PayloadV2::Number(123);
    let buf = cbor_bytes(&newer);
    let result: Result<Closed, _> = from_slice(&buf);
    assert!(result.is_err());
}

// 6. Serializing the #[unknown] variant returns an error (not a panic).

#[test]
fn serializing_unknown_variant_errors() {
    let value = Payload::Unknown;
    let result = to_vec(&value);
    assert!(result.is_err());
}

// 7. Empty-map and two-entry map decode errors.

#[test]
fn empty_map_decode_errors() {
    let mut buf = Vec::new();
    let mut encoder = minicbor::Encoder::new(&mut buf);
    encoder.map(0).expect("map header");

    let result: Result<Payload, _> = from_slice(&buf);
    assert!(result.is_err());
}

#[test]
fn two_entry_map_decode_errors() {
    let mut buf = Vec::new();
    let mut encoder = minicbor::Encoder::new(&mut buf);
    encoder
        .map(2)
        .expect("map header")
        .u32(3)
        .expect("key 1")
        .str("a")
        .expect("value 1")
        .u32(1)
        .expect("key 2")
        .u32(9)
        .expect("value 2");

    let result: Result<Payload, _> = from_slice(&buf);
    assert!(result.is_err());
}

// 8. Nesting: a tagged-union used as a field inside a SerializeTaggedMap struct.

#[derive(Debug, Clone, PartialEq, SerializeTaggedMap, DeserializeTaggedMap)]
struct Envelope {
    #[tag(1)]
    seq: u32,
    #[tag(2)]
    payload: Payload,
}

#[test]
fn union_inside_struct_roundtrips() {
    let orig = Envelope {
        seq: 5,
        payload: Payload::Text("nested".into()),
    };
    assert_eq!(cbor_roundtrip(&orig), orig);

    let orig_blob = Envelope {
        seq: 6,
        payload: Payload::Blob(vec![9, 8, 7]),
    };
    assert_eq!(cbor_roundtrip(&orig_blob), orig_blob);
}

// 9. A Vec<SomeUnion> roundtrips, including an element that decodes to the unknown variant.

#[derive(Debug, Clone, PartialEq, SerializeTaggedMap, DeserializeTaggedMap)]
struct SelfGroupMessages {
    #[tag(1)]
    messages: Vec<Payload>,
}

#[derive(Debug, Clone, PartialEq, SerializeTaggedMap, DeserializeTaggedMap)]
struct SelfGroupMessagesV2 {
    #[tag(1)]
    messages: Vec<PayloadV2>,
}

#[test]
fn vec_of_unions_roundtrips() {
    let orig = SelfGroupMessages {
        messages: vec![
            Payload::Text("one".into()),
            Payload::Blob(vec![1, 2]),
            Payload::Struct(Inner {
                id: 3,
                name: "three".into(),
            }),
        ],
    };
    assert_eq!(cbor_roundtrip(&orig), orig);
}

#[test]
fn vec_of_unions_with_unknown_element() {
    // Encode a "newer" vector containing a variant the older enum does not know.
    let newer = SelfGroupMessagesV2 {
        messages: vec![
            PayloadV2::Text("keep".into()),
            PayloadV2::Number(42),
            PayloadV2::Blob(vec![5, 6]),
        ],
    };
    let buf = to_vec(&newer).expect("serialize");
    let older: SelfGroupMessages = from_slice(&buf).expect("deserialize");
    assert_eq!(
        older,
        SelfGroupMessages {
            messages: vec![
                Payload::Text("keep".into()),
                Payload::Unknown,
                Payload::Blob(vec![5, 6]),
            ],
        }
    );
}

// Generics/lifetimes: a union with a Cow<'_, [u8]> payload.

#[derive(Debug, Clone, PartialEq, SerializeTaggedUnion, DeserializeTaggedUnion)]
enum WithCow<'a> {
    #[tag(1)]
    Data(Cow<'a, [u8]>),
    #[tag(2)]
    Fixed([u8; 4]),
    #[unknown]
    Unknown,
}

#[test]
fn cow_and_array_payloads_roundtrip() {
    let data: WithCow<'_> = WithCow::Data(Cow::Owned(vec![10, 20, 30]));
    assert_eq!(cbor_roundtrip(&data), data);

    // Cow payload is encoded as a byte string.
    let buf = cbor_bytes(&data);
    let mut decoder = minicbor::Decoder::new(&buf);
    assert_eq!(decoder.map().expect("map"), Some(1));
    assert_eq!(decoder.u32().expect("key"), 1);
    assert_eq!(decoder.bytes().expect("bytes"), &[10, 20, 30]);

    let fixed: WithCow<'_> = WithCow::Fixed([1, 2, 3, 4]);
    assert_eq!(cbor_roundtrip(&fixed), fixed);
}

#[derive(Debug, Clone, PartialEq, SerializeTaggedUnion, DeserializeTaggedUnion)]
enum GenericPayload<T> {
    #[tag(1)]
    Value(T),
    #[unknown]
    Unknown,
}

#[test]
fn generic_payload_roundtrips() {
    let value = GenericPayload::Value(42u64);
    assert_eq!(cbor_roundtrip(&value), value);
}
