//! Canonical v4.1 encoder (#210): replacer semantics mirroring the TypeScript
//! reference, plus round-trip proofs over the new encoder/decoder pair
//! ([`encode_v4`] + [`decode_value_v4`]).

use std::cell::RefCell;

use proptest::prelude::*;
use reddb_io_toon::{
    decode_value_v4, encode_v4, encode_v4_with_replacer, DecodeStreamOptions, EncodeV4Options,
    PathSegment, Value,
};
use serde_json::json;

fn decode(wire: &str) -> serde_json::Value {
    decode_value_v4(wire, &DecodeStreamOptions::default())
        .expect("v4 decode of self-produced wire")
        .to_json_value()
}

fn encode(input: serde_json::Value, replacer: &reddb_io_toon::EncodeReplacer) -> String {
    encode_v4_with_replacer(
        &Value::from_json_value(input),
        EncodeV4Options::default(),
        replacer,
    )
    .expect("v4 encode")
}

// ---------------------------------------------------------------------------
// Replacer — mirrors packages/toon/test/encoder.test.mjs
// ---------------------------------------------------------------------------

#[test]
fn replacer_filters_object_properties_and_compacts_array_elements() {
    let replacer = |key: &str, value: &Value, _path: &[PathSegment]| -> Option<Value> {
        if key == "password" {
            return None;
        }
        if value.to_json_value().get("enabled") == Some(&json!(false)) {
            return None;
        }
        Some(value.clone())
    };

    let wire = encode(
        json!({
            "users": [
                { "name": "Ada", "password": "one", "enabled": true },
                { "name": "Bob", "password": "two", "enabled": false },
            ]
        }),
        &replacer,
    );

    assert_eq!(
        decode(&wire),
        json!({ "users": [{ "name": "Ada", "enabled": true }] })
    );
}

#[test]
fn replacer_transforms_primitives_and_replacement_containers_recursively() {
    let replacer = |key: &str, value: &Value, path: &[PathSegment]| -> Option<Value> {
        if path.len() == 1 && key == "user" {
            let mut object = value.to_json_value();
            object
                .as_object_mut()
                .expect("user is an object")
                .insert("role".to_owned(), json!("admin"));
            return Some(Value::from_json_value(object));
        }
        if let Value::String(text) = value {
            return Some(Value::String(text.to_uppercase()));
        }
        Some(value.clone())
    };

    let wire = encode(json!({ "user": { "name": "Ada" } }), &replacer);

    assert_eq!(
        decode(&wire),
        json!({ "user": { "name": "ADA", "role": "ADMIN" } })
    );
}

#[test]
fn replacer_receives_root_object_and_array_paths_with_json_style_keys() {
    let calls: RefCell<Vec<(String, Vec<PathSegment>)>> = RefCell::new(Vec::new());
    {
        let replacer = |key: &str, value: &Value, path: &[PathSegment]| -> Option<Value> {
            calls.borrow_mut().push((key.to_owned(), path.to_vec()));
            Some(value.clone())
        };
        encode(json!({ "rows": [{ "value": 1 }] }), &replacer);
    }

    assert_eq!(
        calls.into_inner(),
        vec![
            (String::new(), vec![]),
            ("rows".to_owned(), vec![PathSegment::Key("rows".to_owned())]),
            (
                "0".to_owned(),
                vec![
                    PathSegment::Key("rows".to_owned()),
                    PathSegment::Index(0)
                ]
            ),
            (
                "value".to_owned(),
                vec![
                    PathSegment::Key("rows".to_owned()),
                    PathSegment::Index(0),
                    PathSegment::Key("value".to_owned()),
                ]
            ),
        ]
    );
}

#[test]
fn undefined_from_the_root_replacer_preserves_the_root_value() {
    let replacer = |_key: &str, value: &Value, path: &[PathSegment]| -> Option<Value> {
        if path.is_empty() {
            None
        } else {
            Some(value.clone())
        }
    };

    let wire = encode(json!({ "name": "Ada" }), &replacer);

    assert_eq!(decode(&wire), json!({ "name": "Ada" }));
}

#[test]
fn a_root_replacement_value_replaces_the_document() {
    // The root is the one position where a returned value is honoured rather
    // than omitted, so replacing it swaps the whole document.
    let replacer = |_key: &str, value: &Value, path: &[PathSegment]| -> Option<Value> {
        if path.is_empty() {
            return Some(Value::from_json_value(json!({ "replaced": true })));
        }
        Some(value.clone())
    };

    let wire = encode(json!([1, 2, 3]), &replacer);

    assert_eq!(decode(&wire), json!({ "replaced": true }));
}

// ---------------------------------------------------------------------------
// Round-trip — encode_v4 + decode_value_v4
// ---------------------------------------------------------------------------

fn round_trips(input: serde_json::Value, options: EncodeV4Options) {
    let value = Value::from_json_value(input);
    let wire = encode_v4(&value, options).expect("v4 encode");
    let decoded = decode_value_v4(&wire, &DecodeStreamOptions::default())
        .unwrap_or_else(|err| panic!("v4 decode failed for {wire:?}: {err}"))
        .to_json_value();
    assert!(
        json_model_eq(&decoded, &value.to_json_value()),
        "round-trip changed the value\n  wire:    {wire:?}\n  decoded: {decoded}",
    );
}

#[test]
fn canonical_forms_round_trip_through_the_v4_decoder() {
    let comma = EncodeV4Options::default();
    let cases = [
        json!({ "servers": { "alpha": { "host": "a", "port": 8080 }, "beta": { "host": "b", "port": 9090 } } }),
        json!({ "alice": { "age": 30, "city": "Berlin" }, "bob": { "age": 25, "city": "Oslo" } }),
        json!({ "regions": { "eu": { "name": "Europe", "geo": { "lat": 50, "lon": 10 } }, "us": { "name": "America", "geo": { "lat": 40, "lon": -100 } } } }),
        json!({ "orders": [{ "id": 1, "customer": { "name": "Ada", "country": "DK" }, "total": 99 }, { "id": 2, "customer": { "name": "Bob", "country": "UK" }, "total": 149 }] }),
        json!({ "items": [{ "config": { "a": { "x": 1 }, "b": { "x": 2 } }, "status": "ok" }, { "status": "down" }] }),
        json!({ "pairs": [["a", "b"], ["c,d", "e:f", "true"]] }),
        json!({ "items": ["#x", { "a": 1 }, "", "  ", "- item"] }),
        json!({ "k": "\u{00a0}x\u{00a0}", "empty": {}, "none": null, "flag": true }),
        json!([{ "id": 1 }, { "id": 2 }]),
        json!({ "nested": { "a": { "b": { "c": "deep" } } }, "tags": [1, 2, 3] }),
    ];
    for case in cases {
        round_trips(case, comma);
    }
}

#[test]
fn active_delimiters_round_trip_through_the_v4_decoder() {
    for delimiter in ['|', '\t'] {
        let options = EncodeV4Options {
            delimiter,
            ..EncodeV4Options::default()
        };
        round_trips(
            json!({ "items": [{ "sku": "A1", "qty": 2, "note": "a,b" }, { "sku": "B2", "qty": 1, "note": "c" }] }),
            options,
        );
        round_trips(json!({ "tags": ["reading", "gaming", "coding"] }), options);
        round_trips(json!({ "pairs": [["a", "b"], ["c", "d"]] }), options);
    }
}

#[test]
fn custom_indent_size_round_trips_through_the_v4_decoder() {
    let options = EncodeV4Options {
        indent_size: 4,
        ..EncodeV4Options::default()
    };
    let value = Value::from_json_value(json!({ "user": { "name": "Ada", "role": "admin" } }));
    let wire = encode_v4(&value, options).expect("v4 encode");
    assert_eq!(wire, "user:\n    name: Ada\n    role: admin");
    let decoded = decode_value_v4(&wire, &DecodeStreamOptions { indent: 4, strict: false })
        .expect("v4 decode")
        .to_json_value();
    assert_eq!(decoded, json!({ "user": { "name": "Ada", "role": "admin" } }));
}

// JSON-model equality (SPEC §2): numbers compare by value after normalization,
// so -0 equals 0 and integer-valued floats equal their integer form.
fn json_model_eq(left: &serde_json::Value, right: &serde_json::Value) -> bool {
    use serde_json::Value as J;
    match (left, right) {
        (J::Number(left), J::Number(right)) => number_eq(left, right),
        (J::Array(left), J::Array(right)) => {
            left.len() == right.len()
                && left.iter().zip(right).all(|(l, r)| json_model_eq(l, r))
        }
        (J::Object(left), J::Object(right)) => {
            left.len() == right.len()
                && left.iter().zip(right).all(|((lk, lv), (rk, rv))| {
                    lk == rk && json_model_eq(lv, rv)
                })
        }
        _ => left == right,
    }
}

fn number_eq(left: &serde_json::Number, right: &serde_json::Number) -> bool {
    if let (Some(left), Some(right)) = (left.as_i64(), right.as_i64()) {
        return left == right;
    }
    if let (Some(left), Some(right)) = (left.as_u64(), right.as_u64()) {
        return left == right;
    }
    match (left.as_f64(), right.as_f64()) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

const SPICY: &str = "\"'\\,:[]{}# \t\n\r\u{0}\u{1f}\u{7f}áé中🙂";

fn key_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z_][a-zA-Z0-9_]{0,12}",
        prop::collection::vec(prop::sample::select(SPICY.chars().collect::<Vec<_>>()), 1..6)
            .prop_map(|chars| chars.into_iter().collect()),
        Just(String::new()),
    ]
}

fn string_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        any::<String>(),
        prop::collection::vec(prop::sample::select(SPICY.chars().collect::<Vec<_>>()), 0..24)
            .prop_map(|chars| chars.into_iter().collect()),
        prop::sample::select(vec![
            "", "true", "false", "null", "42", "-0", "1e10", "  padded  ", "a,b", "[1,2]", "#hash",
            "+1",
        ])
        .prop_map(str::to_owned),
    ]
}

fn number_strategy() -> impl Strategy<Value = serde_json::Value> {
    prop_oneof![
        any::<i64>().prop_map(|value| json!(value)),
        any::<u64>().prop_map(|value| json!(value)),
        any::<f64>()
            .prop_filter("JSON has no NaN or infinity", |value| value.is_finite())
            .prop_map(|value| json!(value)),
    ]
}

fn scalar_strategy() -> impl Strategy<Value = serde_json::Value> {
    prop_oneof![
        Just(serde_json::Value::Null),
        any::<bool>().prop_map(|value| json!(value)),
        number_strategy(),
        string_strategy().prop_map(serde_json::Value::String),
    ]
}

fn value_strategy() -> impl Strategy<Value = serde_json::Value> {
    scalar_strategy().prop_recursive(6, 48, 6, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..6).prop_map(serde_json::Value::Array),
            prop::collection::vec((key_strategy(), inner), 0..6)
                .prop_map(|entries| serde_json::Value::Object(entries.into_iter().collect())),
        ]
    })
}

fn document_strategy() -> impl Strategy<Value = serde_json::Value> {
    prop::collection::vec((key_strategy(), value_strategy()), 0..6)
        .prop_map(|entries| serde_json::Value::Object(entries.into_iter().collect()))
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn encode_v4_then_decode_v4_preserves_the_value(json in document_strategy()) {
        let value = Value::from_json_value(json);
        let wire = encode_v4(&value, EncodeV4Options::default()).expect("v4 encode");
        let decoded = decode_value_v4(&wire, &DecodeStreamOptions::default())
            .expect("v4 decode of self-produced wire")
            .to_json_value();
        prop_assert!(
            json_model_eq(&decoded, &value.to_json_value()),
            "round-trip changed the value\n  wire:    {wire:?}\n  decoded: {decoded}",
        );
    }
}
