use reddb_io_toon::{EncodeOptions, ParseOptions, Value};
use serde_json::Value as Json;
use std::fs;
use std::path::PathBuf;

const WIRE_EFFICIENCY_FIXTURE: &str = "../../tests/corpus/wire-efficiency/corpora.json";
const PRIMITIVE_ARRAY_COLUMNS_FIXTURE: &str =
    "../../tests/corpus/wire-efficiency/primitive-array-columns.json";
const OBJECT_ARRAY_COLUMNS_FIXTURE: &str =
    "../../tests/corpus/wire-efficiency/object-array-columns.json";
const CYCLIC_DISCRIMINATED_ARRAYS_FIXTURE: &str =
    "../../tests/corpus/wire-efficiency/cyclic-discriminated-arrays.json";
const EXPECTED_CASE_COUNT: usize = 9;

#[test]
fn wire_efficiency_corpora_assert_encoded_byte_sizes_for_rust() {
    let fixture_path = fixture_path(WIRE_EFFICIENCY_FIXTURE);
    let fixture = read_fixture(&fixture_path);

    assert_eq!(fixture["seed"], "0x5eed0096");
    let cases = fixture["cases"]
        .as_array()
        .expect("wire-efficiency fixture cases");
    assert_eq!(
        cases.len(),
        EXPECTED_CASE_COUNT,
        "wire-efficiency case count changed"
    );

    for test_case in cases {
        let name = test_case["name"].as_str().expect("case name");
        let value = test_case["value"].clone();
        let expected = &test_case["expectedBytes"];
        let json_min = serde_json::to_string(&value).expect("compact JSON");
        let toon_value = Value::from_json_value(value.clone());
        let toon_v3 = toon_value.to_canonical_toon();
        let toon_tab = toon_value.to_toon_with_options(EncodeOptions {
            delimiter: '\t',
            ..EncodeOptions::default()
        });
        let toon_ext = toon_value.to_toon_with_options(ext_options());

        assert_eq!(
            json_min.len(),
            expected["jsonMin"].as_u64().expect("JSON min byte count") as usize,
            "{name}: JSON min bytes"
        );
        assert_eq!(
            toon_v3.len(),
            expected["toonV3"].as_u64().expect("TOON v3 byte count") as usize,
            "{name}: TOON v3 bytes"
        );
        assert_eq!(
            toon_tab.len(),
            expected["toonTab"].as_u64().expect("TOON tab byte count") as usize,
            "{name}: TOON tab bytes"
        );
        assert_eq!(
            toon_ext.len(),
            expected["toonExt"].as_u64().expect("TOON+ext byte count") as usize,
            "{name}: TOON+ext bytes"
        );
        assert_eq!(
            Value::parse_toon(&toon_v3)
                .unwrap_or_else(|err| panic!("{name}: TOON v3 parse: {err}"))
                .to_json_value(),
            value,
            "{name}: TOON v3 round trip"
        );
        assert_eq!(
            Value::parse_toon(&toon_tab)
                .unwrap_or_else(|err| panic!("{name}: TOON tab parse: {err}"))
                .to_json_value(),
            value,
            "{name}: TOON tab round trip"
        );
        assert_eq!(
            Value::parse_toon(&toon_ext)
                .unwrap_or_else(|err| panic!("{name}: TOON+ext parse: {err}"))
                .to_json_value(),
            value,
            "{name}: TOON+ext round trip"
        );

        if test_case["honestyZeroDelta"].as_bool().unwrap_or(false) {
            assert_eq!(
                toon_ext, toon_v3,
                "{name}: extensions must not change ineligible wire bytes"
            );
        }
    }
}

#[test]
fn primitive_array_column_corpus_decodes_identically_for_rust() {
    let fixture_path = fixture_path(PRIMITIVE_ARRAY_COLUMNS_FIXTURE);
    let fixture = read_fixture(&fixture_path);
    assert_eq!(fixture["version"], 1);

    let cases = fixture["cases"]
        .as_array()
        .expect("primitive-array column cases");
    for test_case in cases {
        let name = test_case["name"].as_str().expect("case name");
        let input = test_case["input"].as_str().expect("case input");
        let expected = test_case.get("expected").expect("case expected");
        let actual = Value::parse_toon(input)
            .unwrap_or_else(|err| panic!("{name}: decode failed: {err}"))
            .to_json_value();
        assert_eq!(actual, *expected, "{name}: decoded value");
        if test_case["failClosedV3Strict"].as_bool().unwrap_or(false) {
            assert!(
                reject_v3_strict(input).is_err(),
                "{name}: strict v3 rejects extension form"
            );
        }
    }

    let errors = fixture["errors"]
        .as_array()
        .expect("primitive-array column errors");
    for test_case in errors {
        let name = test_case["name"].as_str().expect("error case name");
        let input = test_case["input"].as_str().expect("error case input");
        let expected_line = test_case["line"].as_u64().expect("error line") as usize;
        let expected_reason = test_case["reason"].as_str().expect("error reason");
        let error = Value::parse_toon(input).expect_err(name);
        assert_eq!(error.line(), expected_line, "{name}: error line");
        assert_eq!(error.message(), expected_reason, "{name}: error reason");
        assert_eq!(
            error.to_string(),
            format!("line {expected_line}: {expected_reason}"),
            "{name}: error display"
        );
    }
}

#[test]
fn object_array_column_corpus_decodes_identically_for_rust() {
    let fixture: Json = serde_json::from_str(
        &fs::read_to_string(fixture_path(OBJECT_ARRAY_COLUMNS_FIXTURE))
            .expect("object-array column fixture"),
    )
    .expect("object-array column fixture json");
    assert_eq!(fixture.get("version").and_then(Json::as_u64), Some(1));

    for test_case in fixture
        .get("cases")
        .and_then(Json::as_array)
        .expect("object-array column cases")
    {
        let name = test_case.get("name").and_then(Json::as_str).unwrap();
        let input = test_case.get("input").and_then(Json::as_str).unwrap();
        let expected = test_case.get("expected").unwrap();
        let decoded = Value::parse_toon(input)
            .unwrap_or_else(|error| panic!("{name}: parse failed: {error}"))
            .to_json_value();
        assert_eq!(&decoded, expected, "{name}: decoded value");

        if let Some(literal) = test_case.get("strictV3Literal") {
            let decoded = Value::parse_with_options(
                input,
                ParseOptions {
                    cyclic_discriminated_arrays: false,
                    ..ParseOptions::default()
                },
            )
            .unwrap_or_else(|error| panic!("{name}: strict v3 parse failed: {error}"))
            .to_json_value();
            assert_eq!(&decoded, literal, "{name}: strict v3 literal object");
        }
    }

    for test_case in fixture
        .get("errors")
        .and_then(Json::as_array)
        .expect("object-array column errors")
    {
        let name = test_case.get("name").and_then(Json::as_str).unwrap();
        let input = test_case.get("input").and_then(Json::as_str).unwrap();
        let line = test_case.get("line").and_then(Json::as_u64).unwrap() as usize;
        let reason = test_case.get("reason").and_then(Json::as_str).unwrap();
        let error = match Value::parse_toon(input) {
            Ok(_) => panic!("{name}: expected error"),
            Err(error) => error,
        };
        assert_eq!(error.line(), line, "{name}: line");
        assert_eq!(error.message(), reason, "{name}: reason");
        assert_eq!(error.to_string(), format!("line {line}: {reason}"));
    }

    for test_case in fixture
        .get("encodings")
        .and_then(Json::as_array)
        .expect("object-array column encodings")
    {
        let name = test_case.get("name").and_then(Json::as_str).unwrap();
        let value = test_case.get("value").unwrap().clone();
        let toon_value = Value::from_json_value(value.clone());
        let encoded = toon_value.to_toon_with_options(encode_options(
            test_case.get("options").unwrap_or(&Json::Null),
        ));
        let expected = test_case.get("expected").and_then(Json::as_str).unwrap();
        assert_eq!(encoded, expected, "{name}: encoded wire");
        assert_eq!(
            Value::parse_toon(&encoded)
                .unwrap_or_else(|error| panic!("{name}: parse failed: {error}"))
                .to_json_value(),
            value,
            "{name}: round trip"
        );
        if test_case.get("sameAsV3").and_then(Json::as_bool) == Some(true) {
            assert_eq!(
                encoded,
                toon_value.to_canonical_toon(),
                "{name}: v3.3 fallback"
            );
        } else {
            assert_ne!(
                encoded,
                toon_value.to_canonical_toon(),
                "{name}: extension wire"
            );
        }
        if test_case.get("failClosedV3Strict").and_then(Json::as_bool) == Some(true) {
            assert!(
                reject_v3_strict(&encoded).is_err(),
                "{name}: strict v3 rejects extension form"
            );
        }
    }
}

#[test]
fn cyclic_discriminated_array_corpus_decodes_identically_for_rust() {
    let fixture = read_fixture(&fixture_path(CYCLIC_DISCRIMINATED_ARRAYS_FIXTURE));
    assert_eq!(fixture.get("version").and_then(Json::as_u64), Some(1));

    for test_case in fixture
        .get("cases")
        .and_then(Json::as_array)
        .expect("cyclic discriminated-array cases")
    {
        let name = test_case.get("name").and_then(Json::as_str).unwrap();
        let input = test_case.get("input").and_then(Json::as_str).unwrap();
        let expected = test_case.get("expected").unwrap();
        let decoded = Value::parse_toon(input)
            .unwrap_or_else(|error| panic!("{name}: parse failed: {error}"))
            .to_json_value();
        assert_eq!(&decoded, expected, "{name}: decoded value");

        if test_case.get("failClosedV3Strict").and_then(Json::as_bool) == Some(true) {
            assert!(
                reject_v3_strict(input).is_err(),
                "{name}: strict v3 rejects extension form"
            );
        }
    }

    for test_case in fixture
        .get("errors")
        .and_then(Json::as_array)
        .expect("cyclic discriminated-array errors")
    {
        let name = test_case.get("name").and_then(Json::as_str).unwrap();
        let input = test_case.get("input").and_then(Json::as_str).unwrap();
        let line = test_case.get("line").and_then(Json::as_u64).unwrap() as usize;
        let reason = test_case.get("reason").and_then(Json::as_str).unwrap();
        let error = match Value::parse_toon(input) {
            Ok(_) => panic!("{name}: expected error"),
            Err(error) => error,
        };
        assert_eq!(error.line(), line, "{name}: line");
        assert_eq!(error.message(), reason, "{name}: reason");
        assert_eq!(error.to_string(), format!("line {line}: {reason}"));
    }
}

#[test]
fn cyclic_discriminated_array_decoder_handles_quoted_labels_and_nested_arrays_for_rust() {
    let input = "events:\n  order: cycle(log%20in,deploy%2Fcheck)*3\n  discriminator: type\n  rows: 6\n  \"log in\"[3|]{seq|tags.length|tags.0}:\n    1|1|auth\n    3|1|auth\n    5|1|auth\n  \"deploy/check\"[3|]{seq|tags.length|tags.0|tags.1}:\n    2|2|deploy|check\n    4|2|deploy|check\n    6|2|deploy|check\n";
    let decoded = Value::parse_toon(input)
        .expect("decode tabular cyclic labels")
        .to_json_value();

    assert_eq!(
        decoded,
        serde_json::json!({
            "events": [
                { "type": "log in", "seq": 1, "tags": ["auth"] },
                { "type": "deploy/check", "seq": 2, "tags": ["deploy", "check"] },
                { "type": "log in", "seq": 3, "tags": ["auth"] },
                { "type": "deploy/check", "seq": 4, "tags": ["deploy", "check"] },
                { "type": "log in", "seq": 5, "tags": ["auth"] },
                { "type": "deploy/check", "seq": 6, "tags": ["deploy", "check"] }
            ]
        })
    );
    assert_eq!(
        Value::parse_with_options(
            input,
            ParseOptions {
                cyclic_discriminated_arrays: false,
                ..ParseOptions::default()
            },
        )
        .unwrap()
        .to_json_value(),
        serde_json::json!({
            "events": {
                "order": "cycle(log%20in,deploy%2Fcheck)*3",
                "discriminator": "type",
                "rows": 6,
                "log in": [
                    { "seq": 1, "tags.length": 1, "tags.0": "auth" },
                    { "seq": 3, "tags.length": 1, "tags.0": "auth" },
                    { "seq": 5, "tags.length": 1, "tags.0": "auth" }
                ],
                "deploy/check": [
                    { "seq": 2, "tags.length": 2, "tags.0": "deploy", "tags.1": "check" },
                    { "seq": 4, "tags.length": 2, "tags.0": "deploy", "tags.1": "check" },
                    { "seq": 6, "tags.length": 2, "tags.0": "deploy", "tags.1": "check" }
                ]
            }
        })
    );
}

#[test]
fn cyclic_discriminated_array_encoding_is_opt_in_and_pins_the_frozen_wire_for_rust() {
    let fixture = read_fixture(&fixture_path(CYCLIC_DISCRIMINATED_ARRAYS_FIXTURE));
    let test_case = fixture
        .get("cases")
        .and_then(Json::as_array)
        .and_then(|cases| cases.first())
        .expect("cyclic discriminated-array case");
    let expected_value = test_case.get("expected").unwrap();
    let value = Value::from_json_value(expected_value.clone());
    let default_encoded = value.to_canonical_toon();
    let encoded = value.to_toon_with_options(EncodeOptions {
        cyclic_discriminated_arrays: true,
        ..EncodeOptions::default()
    });
    let expected_wire = test_case.get("input").and_then(Json::as_str).unwrap();

    assert_ne!(encoded, default_encoded);
    assert_eq!(encoded, expected_wire);
    assert_eq!(
        value.to_toon_with_options(EncodeOptions::default()),
        default_encoded
    );
    assert_eq!(
        Value::parse_toon(&encoded).unwrap().to_json_value(),
        expected_value.clone()
    );
    assert_eq!(
        Value::parse_with_options(
            &encoded,
            ParseOptions {
                cyclic_discriminated_arrays: false,
                ..ParseOptions::default()
            },
        )
        .unwrap()
        .to_json_value(),
        test_case.get("strictV3Literal").unwrap().clone()
    );
}

#[test]
fn cyclic_discriminated_array_encoding_emits_percent_encoded_multi_section_wire_for_rust() {
    let events = (0..12)
        .map(|index| {
            let label = if index % 2 == 0 {
                "log in"
            } else {
                "deploy/check"
            };
            serde_json::json!({
                "type": label,
                "tenant": "acme",
                "seq": index + 1,
                "ok": index % 3 != 0,
                "detail": {
                    "bucket": if label == "log in" { "auth" } else { "deploy" },
                    "attempt": (index / 2) + 1
                }
            })
        })
        .collect::<Vec<_>>();
    let audits = (0..12)
        .map(|index| {
            let label = if index % 2 == 0 {
                "alpha beta"
            } else {
                "gamma/delta"
            };
            serde_json::json!({
                "kind": label,
                "tenant": "acme",
                "seq": index + 1,
                "actor": format!("u{}", (index / 2) + 1),
                "detail": {
                    "bucket": if label == "alpha beta" { "alpha" } else { "gamma" },
                    "attempt": (index / 2) + 1
                }
            })
        })
        .collect::<Vec<_>>();
    let input = serde_json::json!({
        "events": events,
        "audits": audits
    });
    let value = Value::from_json_value(input.clone());
    let encoded = value.to_toon_with_options(EncodeOptions {
        cyclic_discriminated_arrays: true,
        ..EncodeOptions::default()
    });

    assert!(encoded.starts_with("events:\n"));
    assert!(encoded.contains("  order: cycle(log%20in,deploy%2Fcheck)*6\n"));
    assert!(encoded.contains("  \"log in\"[6|]{detail.bucket|detail.attempt}:\n"));
    assert!(encoded.contains("audits:\n"));
    assert!(encoded.contains("  order: cycle(alpha%20beta,gamma%2Fdelta)*6\n"));
    assert!(encoded.contains("  \"alpha beta\"[6|]{detail.bucket|detail.attempt}:\n"));
    assert_eq!(Value::parse_toon(&encoded).unwrap().to_json_value(), input);
}

#[test]
fn cyclic_discriminated_array_encoding_falls_back_for_boundary_cases_for_rust() {
    for (name, input) in [
        (
            "two repeats is below the repeat threshold",
            serde_json::json!({
                "events": [
                    { "type": "login", "tenant": "acme", "seq": 1, "actor": "u1" },
                    { "type": "purchase", "tenant": "acme", "seq": 2, "actor": "u1" },
                    { "type": "login", "tenant": "acme", "seq": 3, "actor": "u2" },
                    { "type": "purchase", "tenant": "acme", "seq": 4, "actor": "u2" }
                ]
            }),
        ),
        (
            "partial cycle is ineligible",
            serde_json::json!({
                "events": [
                    { "type": "login", "tenant": "acme", "seq": 1, "actor": "u1" },
                    { "type": "purchase", "tenant": "acme", "seq": 2, "actor": "u1" },
                    { "type": "logout", "tenant": "acme", "seq": 3, "actor": "u1" },
                    { "type": "login", "tenant": "acme", "seq": 4, "actor": "u2" },
                    { "type": "purchase", "tenant": "acme", "seq": 5, "actor": "u2" }
                ]
            }),
        ),
        (
            "irregular order is ineligible",
            serde_json::json!({
                "events": [
                    { "type": "login", "tenant": "acme", "seq": 1, "actor": "u1" },
                    { "type": "purchase", "tenant": "acme", "seq": 2, "actor": "u1" },
                    { "type": "logout", "tenant": "acme", "seq": 3, "actor": "u1" },
                    { "type": "purchase", "tenant": "acme", "seq": 4, "actor": "u2" },
                    { "type": "login", "tenant": "acme", "seq": 5, "actor": "u2" },
                    { "type": "logout", "tenant": "acme", "seq": 6, "actor": "u2" }
                ]
            }),
        ),
        (
            "compact order must beat the threshold",
            serde_json::json!({
                "events": [
                    { "type": "a", "seq": 1 },
                    { "type": "b", "seq": 2 },
                    { "type": "a", "seq": 3 },
                    { "type": "b", "seq": 4 },
                    { "type": "a", "seq": 5 },
                    { "type": "b", "seq": 6 }
                ]
            }),
        ),
        (
            "single-label cycle is ineligible",
            serde_json::json!({
                "events": [
                    { "type": "login", "seq": 1 },
                    { "type": "login", "seq": 2 },
                    { "type": "login", "seq": 3 },
                    { "type": "login", "seq": 4 },
                    { "type": "login", "seq": 5 },
                    { "type": "login", "seq": 6 }
                ]
            }),
        ),
        (
            "non object rows are ineligible",
            serde_json::json!({
                "events": [
                    { "type": "login", "seq": 1 },
                    "not-an-object",
                    { "type": "login", "seq": 3 },
                    "not-an-object",
                    { "type": "login", "seq": 5 },
                    "not-an-object"
                ]
            }),
        ),
        (
            "common field names must be header tokens",
            serde_json::json!({
                "events": [
                    { "type": "login", "tenant id": "acme", "seq": 1 },
                    { "type": "purchase", "tenant id": "acme", "seq": 2 },
                    { "type": "login", "tenant id": "acme", "seq": 3 },
                    { "type": "purchase", "tenant id": "acme", "seq": 4 },
                    { "type": "login", "tenant id": "acme", "seq": 5 },
                    { "type": "purchase", "tenant id": "acme", "seq": 6 },
                    { "type": "login", "tenant id": "acme", "seq": 7 },
                    { "type": "purchase", "tenant id": "acme", "seq": 8 }
                ]
            }),
        ),
        (
            "non-uniform nested payload shape within a group is ineligible",
            serde_json::json!({
                "events": [
                    { "type": "login", "seq": 1, "payload": { "ok": true, "ip": "127.0.0.1" } },
                    { "type": "purchase", "seq": 2, "payload": { "amount": 2 } },
                    { "type": "login", "seq": 3, "payload": { "ok": true } },
                    { "type": "purchase", "seq": 4, "payload": { "amount": 4 } },
                    { "type": "login", "seq": 5, "payload": { "ok": true } },
                    { "type": "purchase", "seq": 6, "payload": { "amount": 6 } },
                    { "type": "login", "seq": 7, "payload": { "ok": true } },
                    { "type": "purchase", "seq": 8, "payload": { "amount": 8 } },
                    { "type": "login", "seq": 9, "payload": { "ok": true } },
                    { "type": "purchase", "seq": 10, "payload": { "amount": 10 } },
                    { "type": "login", "seq": 11, "payload": { "ok": true } },
                    { "type": "purchase", "seq": 12, "payload": { "amount": 12 } }
                ]
            }),
        ),
    ] {
        let value = Value::from_json_value(input.clone());
        let encoded = value.to_toon_with_options(EncodeOptions {
            cyclic_discriminated_arrays: true,
            ..EncodeOptions::default()
        });
        assert_eq!(encoded, value.to_canonical_toon(), "{name}");
        assert_eq!(Value::parse_toon(&encoded).unwrap().to_json_value(), input);
    }
}

#[test]
fn primitive_array_column_encoding_is_opt_in_and_falls_back_losslessly_for_rust() {
    let eligible = serde_json::json!({
        "items": [
            { "id": 1, "tags": ["hot", "fragile"], "note": "a,b" },
            { "id": 2, "tags": ["semi;quoted"], "note": "plain" }
        ]
    });
    let value = Value::from_json_value(eligible.clone());
    let encoded = value.to_toon_with_options(EncodeOptions {
        primitive_array_columns: true,
        ..EncodeOptions::default()
    });
    assert_eq!(
        encoded,
        "items[2]{id,tags[;],note}:\n  1,hot;fragile,\"a,b\"\n  2,\"semi;quoted\",plain\n"
    );
    assert_eq!(
        value.to_canonical_toon(),
        "items[2]:\n  - id: 1\n    tags[2]: hot,fragile\n    note: \"a,b\"\n  - id: 2\n    tags[1]: semi;quoted\n    note: plain\n"
    );
    assert_eq!(
        Value::parse_toon(&encoded).unwrap().to_json_value(),
        eligible
    );

    let ineligible = serde_json::json!({
        "items": [
            { "id": 1, "tags": null },
            { "id": 2, "tags": ["ok"] }
        ]
    });
    let value = Value::from_json_value(ineligible.clone());
    let encoded = value.to_toon_with_options(EncodeOptions {
        primitive_array_columns: true,
        ..EncodeOptions::default()
    });
    assert_eq!(encoded, value.to_canonical_toon());
    assert_eq!(
        Value::parse_toon(&encoded).unwrap().to_json_value(),
        ineligible
    );
}

#[test]
fn object_array_column_encoding_is_opt_in_and_falls_back_losslessly_for_rust() {
    let eligible = serde_json::json!({
        "orders": [
            {
                "id": "ord_001",
                "customer": "cust_a",
                "items": [
                    {
                        "sku": "sku_1",
                        "quantity": 3,
                        "components": [{ "part": "part_a", "lot": "lot_1", "ok": true }]
                    },
                    { "sku": "sku_2", "quantity": 1, "components": [] }
                ]
            },
            { "id": "ord_002", "customer": "cust_b", "items": [] }
        ]
    });
    let value = Value::from_json_value(eligible.clone());
    let encoded = value.to_toon_with_options(EncodeOptions {
        object_array_columns: true,
        delimiter: '|',
        ..EncodeOptions::default()
    });
    assert_eq!(
        encoded,
        "orders[2|]{id|customer|items{sku|quantity|components{part|lot|ok}}}:\n  ord_001|cust_a|2\n    sku_1|3|1\n      part_a|lot_1|true\n    sku_2|1|0\n  ord_002|cust_b|0\n"
    );
    assert_ne!(
        encoded,
        value.to_toon_with_options(EncodeOptions {
            delimiter: '|',
            ..EncodeOptions::default()
        })
    );
    assert_eq!(
        Value::parse_toon(&encoded).unwrap().to_json_value(),
        eligible
    );

    let matrix = serde_json::json!({ "matrix": [[1, 2, 3], [4, 5, 6]] });
    let matrix_value = Value::from_json_value(matrix.clone());
    let matrix_encoded = matrix_value.to_toon_with_options(EncodeOptions {
        object_array_columns: true,
        delimiter: '|',
        ..EncodeOptions::default()
    });
    assert_eq!(
        matrix_encoded,
        "matrix[2|]{values[3|]}:\n  1|2|3\n  4|5|6\n"
    );
    assert_eq!(
        Value::parse_toon(&matrix_encoded).unwrap().to_json_value(),
        matrix
    );

    let ineligible = serde_json::json!({
        "orders": [
            { "id": "ord_001", "items": [{ "sku": "a" }] },
            { "id": "ord_002", "items": [1] }
        ]
    });
    let value = Value::from_json_value(ineligible.clone());
    let encoded = value.to_toon_with_options(EncodeOptions {
        object_array_columns: true,
        ..EncodeOptions::default()
    });
    assert_eq!(encoded, value.to_canonical_toon());
    assert_eq!(
        Value::parse_toon(&encoded).unwrap().to_json_value(),
        ineligible
    );
}

fn fixture_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn read_fixture(path: &PathBuf) -> Json {
    let json =
        fs::read_to_string(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    serde_json::from_str(&json).unwrap_or_else(|err| panic!("parse {}: {err}", path.display()))
}

fn ext_options() -> EncodeOptions {
    EncodeOptions {
        nested_tabular_headers: true,
        keyed_map_collapse: true,
        primitive_array_columns: true,
        object_array_columns: true,
        cyclic_discriminated_arrays: true,
        ..EncodeOptions::default()
    }
}

fn encode_options(options: &Json) -> EncodeOptions {
    EncodeOptions {
        nested_tabular_headers: options
            .get("nestedTabularHeaders")
            .and_then(Json::as_bool)
            .unwrap_or(false),
        keyed_map_collapse: options
            .get("keyedMapCollapse")
            .and_then(Json::as_bool)
            .unwrap_or(false),
        primitive_array_columns: options
            .get("primitiveArrayColumns")
            .and_then(Json::as_bool)
            .unwrap_or(false),
        object_array_columns: options
            .get("objectArrayColumns")
            .and_then(Json::as_bool)
            .unwrap_or(false),
        cyclic_discriminated_arrays: options
            .get("cyclicDiscriminatedArrays")
            .and_then(Json::as_bool)
            .unwrap_or(false),
        delimiter: options
            .get("delimiter")
            .and_then(Json::as_str)
            .and_then(|value| value.chars().next())
            .unwrap_or(','),
        ..EncodeOptions::default()
    }
}

fn reject_v3_strict(input: &str) -> Result<(), String> {
    for (index, line) in input.lines().enumerate() {
        let trimmed = line.trim_start();
        let Some(colon) = trimmed.find(':') else {
            continue;
        };
        let key_part = &trimmed[..colon];
        let Some(fields_start) = key_part.find('{') else {
            continue;
        };
        if key_part[fields_start..].contains('[') || key_part[fields_start + 1..].contains('{') {
            return Err(format!("line {}: invalid array header", index + 1));
        }
    }
    Ok(())
}
