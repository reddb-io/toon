use reddb_io_toon::{EncodeOptions, Value};
use serde_json::Value as Json;
use std::fs;
use std::path::PathBuf;

const WIRE_EFFICIENCY_FIXTURE: &str = "../../tests/wire-efficiency/corpora.json";
const PRIMITIVE_ARRAY_COLUMNS_FIXTURE: &str =
    "../../tests/wire-efficiency/primitive-array-columns.json";
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
        if key_part[fields_start..].contains('[') {
            return Err(format!("line {}: invalid array header", index + 1));
        }
    }
    Ok(())
}
