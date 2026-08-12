//! The TOON v4.1 encoder (#210): replacer semantics mirroring the TypeScript
//! reference, plus round-trip proofs over the one encoder/decoder pair
//! ([`encode_with_options`] + [`decode_with_options`]).

use std::cell::RefCell;

use proptest::prelude::*;
use reddb_io_toon::{
    build_value_from_events, decode_event_stream, decode_with_options, detect_truncation_with_options,
    encode_toonl_values, encode_with_options, encode_with_replacer, DecodeStreamOptions, EncodeOptions,
    ParseError, PathSegment, ToonlStream, Value,
};
use serde_json::json;

fn decode(wire: &str) -> serde_json::Value {
    decode_with_options(wire, &DecodeStreamOptions::default())
        .expect("decode of self-produced wire")
        .to_json_value()
}

fn decode_via_events(
    wire: &str,
    options: &DecodeStreamOptions,
) -> Result<serde_json::Value, ParseError> {
    let events = decode_event_stream(wire, options).collect::<Result<Vec<_>, _>>()?;
    Ok(build_value_from_events(&events).to_json_value())
}

fn encode(input: serde_json::Value, replacer: &reddb_io_toon::EncodeReplacer) -> String {
    encode_with_replacer(
        &Value::from_json_value(input),
        EncodeOptions::default(),
        replacer,
    )
    .expect("canonical encode")
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
                vec![PathSegment::Key("rows".to_owned()), PathSegment::Index(0)]
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
// Round-trip — encode_with_options + decode_with_options
// ---------------------------------------------------------------------------

fn round_trips(input: serde_json::Value, options: EncodeOptions) {
    let value = Value::from_json_value(input);
    let wire = encode_with_options(&value, options).expect("canonical encode");
    let decoded = decode_with_options(&wire, &DecodeStreamOptions::default())
        .unwrap_or_else(|err| panic!("decode failed for {wire:?}: {err}"))
        .to_json_value();
    assert!(
        json_model_eq(&decoded, &value.to_json_value()),
        "round-trip changed the value\n  wire:    {wire:?}\n  decoded: {decoded}",
    );
}

#[test]
fn canonical_forms_round_trip_through_the_decoder() {
    let comma = EncodeOptions::default();
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
fn active_delimiters_round_trip_through_the_decoder() {
    for delimiter in ['|', '\t'] {
        let options = EncodeOptions {
            delimiter,
            ..EncodeOptions::default()
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
fn custom_indent_size_round_trips_through_the_decoder() {
    let options = EncodeOptions {
        indent_size: 4,
        ..EncodeOptions::default()
    };
    let value = Value::from_json_value(json!({ "user": { "name": "Ada", "role": "admin" } }));
    let wire = encode_with_options(&value, options).expect("canonical encode");
    assert_eq!(wire, "user:\n    name: Ada\n    role: admin");
    let decoded = decode_with_options(
        &wire,
        &DecodeStreamOptions {
            indent: 4,
            strict: false,
            ..DecodeStreamOptions::default()
        },
    )
    .expect("decode")
    .to_json_value();
    assert_eq!(
        decoded,
        json!({ "user": { "name": "Ada", "role": "admin" } })
    );
}

#[test]
fn zero_indent_matches_the_reference_edges() {
    let value = Value::from_json_value(json!({ "user": { "name": "Ada" } }));
    let wire = encode_with_options(
        &value,
        EncodeOptions {
            indent_size: 0,
            ..EncodeOptions::default()
        },
    )
    .expect("zero-indent encode");
    assert_eq!(wire, "user:\nname: Ada");

    let error = decode_with_options(
        "name: Ada",
        &DecodeStreamOptions {
            indent: 0,
            ..DecodeStreamOptions::default()
        },
    )
    .expect_err("zero-indent decode rejects non-empty input");
    assert_eq!(error.reason(), "invalid indentation");
}

// ---------------------------------------------------------------------------
// Extensions rebuilt on the v4.1 entry points (#215)
// ---------------------------------------------------------------------------

#[test]
fn extension_options_pin_shared_primitive_and_child_table_wires() {
    let primitive_corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../corpus/wire-efficiency/primitive-array-columns.json"
    ))
    .expect("primitive-column corpus");
    let primitive_fixture = &primitive_corpus["cases"][0];
    assert_eq!(
        decode_with_options(
            primitive_fixture["input"]
                .as_str()
                .expect("primitive fixture wire"),
            &DecodeStreamOptions::default(),
        )
        .expect("shared primitive fixture decode")
        .to_json_value(),
        primitive_fixture["expected"],
    );

    let primitive = json!({
        "items": [
            { "id": 1, "tags": ["hot", "fragile"], "note": "a,b" },
            { "id": 2, "tags": ["semi;quoted"], "note": "plain" }
        ]
    });
    let primitive_wire =
        "items[2]{id,tags[;],note}:\n  1,hot;fragile,\"a,b\"\n  2,\"semi;quoted\",plain";
    assert_eq!(
        encode_with_options(
            &Value::from_json_value(primitive.clone()),
            EncodeOptions {
                primitive_array_columns: true,
                ..EncodeOptions::default()
            },
        )
        .expect("primitive columns encode"),
        primitive_wire,
    );
    assert_eq!(
        decode_with_options(primitive_wire, &DecodeStreamOptions::default())
            .expect("primitive columns decode")
            .to_json_value(),
        primitive,
    );

    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../corpus/wire-efficiency/object-array-columns.json"
    ))
    .expect("child-table corpus");
    for fixture in corpus["cases"].as_array().expect("cases") {
        let wire = fixture["input"].as_str().expect("wire").trim_end();
        let expected = &fixture["expected"];
        assert_eq!(
            decode_with_options(wire, &DecodeStreamOptions::default())
                .unwrap_or_else(|error| panic!("{}: {error}", fixture["name"]))
                .to_json_value(),
            *expected,
        );
        assert!(decode_with_options(
            wire,
            &DecodeStreamOptions {
                object_array_columns: false,
                ..DecodeStreamOptions::default()
            },
        )
        .is_err());
    }
    for fixture in corpus["errors"].as_array().expect("errors") {
        let error = decode_with_options(
            fixture["input"].as_str().expect("invalid wire"),
            &DecodeStreamOptions::default(),
        )
        .expect_err("invalid child table");
        assert_eq!(error.line(), fixture["line"].as_u64().unwrap() as usize);
        assert_eq!(error.message(), fixture["reason"].as_str().unwrap());
    }

    let fixture = &corpus["encodings"][0];
    assert_eq!(
        encode_with_options(
            &Value::from_json_value(fixture["value"].clone()),
            EncodeOptions {
                object_array_columns: true,
                ..EncodeOptions::default()
            },
        )
        .expect("child tables encode"),
        fixture["expected"]
            .as_str()
            .expect("expected wire")
            .trim_end(),
    );
}

#[test]
fn extensions_decode_value_identically_through_the_event_stream() {
    let primitive_corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../corpus/wire-efficiency/primitive-array-columns.json"
    ))
    .expect("primitive-column corpus");
    for fixture in primitive_corpus["cases"].as_array().expect("cases") {
        assert_eq!(
            decode_via_events(
                fixture["input"].as_str().expect("primitive fixture wire"),
                &DecodeStreamOptions::default(),
            )
            .unwrap_or_else(|error| panic!("{}: {error}", fixture["name"])),
            fixture["expected"],
        );
    }
    for fixture in primitive_corpus["errors"].as_array().expect("errors") {
        let error = decode_via_events(
            fixture["input"].as_str().expect("invalid primitive wire"),
            &DecodeStreamOptions::default(),
        )
        .expect_err("invalid primitive column");
        assert_eq!(error.line(), fixture["line"].as_u64().unwrap() as usize);
        assert_eq!(error.message(), fixture["reason"].as_str().unwrap());
    }

    let child_corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../corpus/wire-efficiency/object-array-columns.json"
    ))
    .expect("child-table corpus");
    for fixture in child_corpus["cases"].as_array().expect("cases") {
        let wire = fixture["input"].as_str().expect("child fixture wire");
        assert_eq!(
            decode_via_events(wire, &DecodeStreamOptions::default())
                .unwrap_or_else(|error| panic!("{}: {error}", fixture["name"])),
            fixture["expected"],
        );
        assert!(decode_via_events(
            wire,
            &DecodeStreamOptions {
                object_array_columns: false,
                ..DecodeStreamOptions::default()
            },
        )
        .is_err());
    }
    for fixture in child_corpus["errors"].as_array().expect("errors") {
        let error = decode_via_events(
            fixture["input"].as_str().expect("invalid child wire"),
            &DecodeStreamOptions::default(),
        )
        .expect_err("invalid child table");
        assert_eq!(error.line(), fixture["line"].as_u64().unwrap() as usize);
        assert_eq!(error.message(), fixture["reason"].as_str().unwrap());
    }
}

#[test]
fn cyclic_fixture_is_literal_without_opt_in_and_deterministic_with_it() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../corpus/wire-efficiency/cyclic-discriminated-arrays.json"
    ))
    .expect("cyclic corpus");
    let fixture = &corpus["cases"][0];
    let wire = fixture["input"].as_str().expect("wire").trim_end();

    assert_eq!(
        decode_with_options(wire, &DecodeStreamOptions::default())
            .expect("literal cyclic metadata")
            .to_json_value(),
        fixture["canonicalLiteral"],
    );
    assert_eq!(
        decode_with_options(
            wire,
            &DecodeStreamOptions {
                cyclic_discriminated_arrays: true,
                ..DecodeStreamOptions::default()
            },
        )
        .expect("cyclic graph reconstruction")
        .to_json_value(),
        fixture["expected"],
    );
    assert_eq!(
        encode_with_options(
            &Value::from_json_value(fixture["expected"].clone()),
            EncodeOptions {
                cyclic_discriminated_arrays: true,
                ..EncodeOptions::default()
            },
        )
        .expect("cyclic graph encode"),
        wire,
    );
}

#[test]
fn toonl_truncation_and_depth_results_are_exact() {
    let value = Value::from_json_value(json!({
        "people": { "ada": { "name": "Ada" }, "linus": { "name": "Linus" } }
    }));
    let wire = encode_with_options(&value, EncodeOptions::default()).expect("canonical encode");
    assert_eq!(wire, "people[2:]{name}:\n  ada: Ada\n  linus: Linus");
    assert_eq!(
        decode_with_options(
            &format!("# generated\n{wire}"),
            &DecodeStreamOptions::default()
        )
        .expect("decode")
        .to_json_value(),
        value.to_json_value(),
    );

    let rows = [
        Value::from_json_value(json!({ "id": 1, "name": "Ada" })),
        Value::from_json_value(json!({ "id": 2, "name": "Linus" })),
    ];
    let toonl = encode_toonl_values(&rows).expect("TOONL encode");
    assert_eq!(toonl, "[]{id,name}:\n1,Ada\n2,Linus\n[=2]\n");
    assert_eq!(
        ToonlStream::parse(&toonl)
            .expect("TOONL decode")
            .row_values()
            .expect("TOONL rows")
            .iter()
            .map(Value::to_json_value)
            .collect::<Vec<_>>(),
        rows.iter().map(Value::to_json_value).collect::<Vec<_>>(),
    );

    assert_eq!(
        detect_truncation_with_options(
            "# users\n[2:]{name}:\n  ada: Ada",
            &DecodeStreamOptions::default(),
        )
        .to_json_value(),
        json!({
            "complete": false,
            "kind": "array_length_mismatch",
            "line": 3,
            "declared": 2,
            "actual": 1,
            "message": "declared 2 rows but received 1",
        }),
    );

    let nested = Value::from_json_value(json!({ "a": { "b": { "c": 1 } } }));
    assert_eq!(
        encode_with_options(
            &nested,
            EncodeOptions {
                max_depth: 1,
                ..EncodeOptions::default()
            },
        )
        .expect_err("encode depth guard")
        .to_string(),
        "maximum nesting depth exceeded (maxDepth 1)",
    );
    assert_eq!(
        decode_with_options(
            "a:\n  b:\n    c: 1",
            &DecodeStreamOptions {
                max_depth: 1,
                ..DecodeStreamOptions::default()
            },
        )
        .expect_err("decode depth guard")
        .to_string(),
        "line 3: maximum nesting depth exceeded (maxDepth 1)",
    );
}

// JSON-model equality (SPEC §2): numbers compare by value after normalization,
// so -0 equals 0 and integer-valued floats equal their integer form.
fn json_model_eq(left: &serde_json::Value, right: &serde_json::Value) -> bool {
    use serde_json::Value as J;
    match (left, right) {
        (J::Number(left), J::Number(right)) => number_eq(left, right),
        (J::Array(left), J::Array(right)) => {
            left.len() == right.len() && left.iter().zip(right).all(|(l, r)| json_model_eq(l, r))
        }
        (J::Object(left), J::Object(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|((lk, lv), (rk, rv))| lk == rk && json_model_eq(lv, rv))
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
        prop::collection::vec(
            prop::sample::select(SPICY.chars().collect::<Vec<_>>()),
            1..6
        )
        .prop_map(|chars| chars.into_iter().collect()),
        Just(String::new()),
    ]
}

fn string_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        any::<String>(),
        prop::collection::vec(
            prop::sample::select(SPICY.chars().collect::<Vec<_>>()),
            0..24
        )
        .prop_map(|chars| chars.into_iter().collect()),
        prop::sample::select(vec![
            "",
            "true",
            "false",
            "null",
            "42",
            "-0",
            "1e10",
            "  padded  ",
            "a,b",
            "[1,2]",
            "#hash",
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
    fn encode_then_decode_preserves_the_value(json in document_strategy()) {
        let value = Value::from_json_value(json);
        let wire = encode_with_options(&value, EncodeOptions::default()).expect("canonical encode");
        let decoded = decode_with_options(&wire, &DecodeStreamOptions::default())
            .expect("decode of self-produced wire")
            .to_json_value();
        prop_assert!(
            json_model_eq(&decoded, &value.to_json_value()),
            "round-trip changed the value\n  wire:    {wire:?}\n  decoded: {decoded}",
        );
    }
}
