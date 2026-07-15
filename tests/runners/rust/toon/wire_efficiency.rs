use reddb_io_toon::{EncodeOptions, Value};
use serde_json::Value as Json;
use std::fs;
use std::path::PathBuf;

const WIRE_EFFICIENCY_FIXTURE: &str = "../../tests/corpus/wire-efficiency/corpora.json";
const PRIMITIVE_ARRAY_COLUMNS_FIXTURE: &str =
    "../../tests/corpus/wire-efficiency/primitive-array-columns.json";
const OBJECT_ARRAY_COLUMNS_FIXTURE: &str =
    "../../tests/corpus/wire-efficiency/object-array-columns.json";
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
