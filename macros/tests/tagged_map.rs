// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::borrow::Cow;

use airmacros::{DeserializeTaggedMap, SerializeTaggedMap};
use ciborium::{from_reader, into_writer};

// Helpers

fn cbor_roundtrip<T>(value: &T) -> T
where
    T: serde::Serialize + for<'de> serde::Deserialize<'de>,
{
    let mut buf = Vec::new();
    into_writer(value, &mut buf).expect("serialize");
    from_reader(buf.as_slice()).expect("deserialize")
}

/// Returns the number of entries in the top-level CBOR map.
fn cbor_map_len(value: &impl serde::Serialize) -> usize {
    let mut buf = Vec::new();
    into_writer(value, &mut buf).expect("serialize");
    let v: ciborium::value::Value = from_reader(buf.as_slice()).expect("parse");
    match v {
        ciborium::value::Value::Map(m) => m.len(),
        other => panic!("expected CBOR map, got {other:?}"),
    }
}

// Basic roundtrip

#[derive(Debug, PartialEq, SerializeTaggedMap, DeserializeTaggedMap)]
struct Basic {
    #[tag(1)]
    id: u32,
    #[tag(2)]
    name: String,
    #[tag(3)]
    flag: bool,
    #[tag(4)]
    score: i64,
}

#[test]
fn basic_roundtrip() {
    let orig = Basic {
        id: 42,
        name: "hello".into(),
        flag: true,
        score: -7,
    };
    assert_eq!(cbor_roundtrip(&orig), orig);
}

// Skip-if-default

#[test]
fn skip_if_default_omits_zero_and_empty() {
    let full = Basic {
        id: 1,
        name: "x".into(),
        flag: true,
        score: 1,
    };
    assert_eq!(cbor_map_len(&full), 4);

    let partial = Basic {
        id: 0,           // default -> omitted
        name: "".into(), // default -> omitted
        flag: false,     // default -> omitted
        score: 1,
    };
    assert_eq!(cbor_map_len(&partial), 1);
}

#[test]
fn skip_if_default_roundtrips_partial() {
    let orig = Basic {
        id: 0,
        name: String::new(),
        flag: false,
        score: 99,
    };
    assert_eq!(cbor_roundtrip(&orig), orig);
}

// Option fields

#[derive(Debug, PartialEq, SerializeTaggedMap, DeserializeTaggedMap)]
struct WithOption {
    #[tag(1)]
    value: Option<u32>,
    #[tag(2)]
    label: Option<String>,
}

#[test]
fn option_some_roundtrip() {
    let orig = WithOption {
        value: Some(7),
        label: Some("hi".into()),
    };
    assert_eq!(cbor_roundtrip(&orig), orig);
}

#[test]
fn option_none_is_omitted() {
    let v = WithOption {
        value: None,
        label: None,
    };
    assert_eq!(cbor_map_len(&v), 0);
    assert_eq!(cbor_roundtrip(&v), v);
}

// Bytes fields

#[derive(Debug, PartialEq, SerializeTaggedMap, DeserializeTaggedMap)]
struct WithBytes {
    #[tag(1)]
    blob: Vec<u8>,
    #[tag(2)]
    fixed: [u8; 4],
    #[tag(3)]
    opt_blob: Option<Vec<u8>>,
}

#[test]
fn vec_u8_roundtrip() {
    let orig = WithBytes {
        blob: vec![1, 2, 3],
        fixed: [0xde, 0xad, 0xbe, 0xef],
        opt_blob: Some(vec![0xff]),
    };
    assert_eq!(cbor_roundtrip(&orig), orig);
}

#[test]
fn vec_u8_empty_is_omitted() {
    let v = WithBytes {
        blob: vec![],
        fixed: [0; 4],
        opt_blob: None,
    };
    assert_eq!(cbor_map_len(&v), 0);
    assert_eq!(cbor_roundtrip(&v), v);
}

// Cow<'_, [u8]>

#[derive(Debug, PartialEq, SerializeTaggedMap, DeserializeTaggedMap)]
struct WithCow<'a> {
    #[tag(1)]
    data: Cow<'a, [u8]>,
    #[tag(2)]
    opt_data: Option<Cow<'a, [u8]>>,
}

#[test]
fn cow_bytes_roundtrip() {
    let orig: WithCow<'_> = WithCow {
        data: Cow::Owned(vec![10, 20, 30]),
        opt_data: Some(Cow::Owned(vec![1])),
    };
    assert_eq!(cbor_roundtrip(&orig), orig);
}

#[test]
fn cow_bytes_empty_is_omitted() {
    let v: WithCow<'_> = WithCow {
        data: Cow::Borrowed(&[]),
        opt_data: None,
    };
    assert_eq!(cbor_map_len(&v), 0);
    assert_eq!(cbor_roundtrip(&v), v);
}

// Unknown keys ignored

/// We serialize a superset struct and deserialize into a subset struct to
/// simulate forward-compatibility (extra fields are unknown to the old reader).
#[derive(Debug, PartialEq, SerializeTaggedMap, DeserializeTaggedMap)]
struct Superset {
    #[tag(1)]
    known: u32,
    #[tag(99)]
    unknown: u32,
}

#[derive(Debug, PartialEq, SerializeTaggedMap, DeserializeTaggedMap)]
struct Subset {
    #[tag(1)]
    known: u32,
}

#[test]
fn unknown_keys_are_ignored() {
    let super_val = Superset {
        known: 5,
        unknown: 999,
    };
    let mut buf = Vec::new();
    into_writer(&super_val, &mut buf).expect("serialize");

    let sub_val: Subset = from_reader(buf.as_slice()).expect("deserialize");
    assert_eq!(sub_val, Subset { known: 5 });
}

// Missing keys get default

#[test]
fn missing_keys_default_to_zero() {
    // Serialize only the `unknown` field (tag 99), which Subset doesn't know.
    let super_val = Superset {
        known: 0, // default -> omitted
        unknown: 1,
    };
    let mut buf = Vec::new();
    into_writer(&super_val, &mut buf).expect("serialize");

    let sub_val: Subset = from_reader(buf.as_slice()).expect("deserialize");
    assert_eq!(sub_val, Subset { known: 0 });
}
