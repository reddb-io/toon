use reddb_io_toon::{
    encode_toonl_values, ParseOptions, ToonlEncoder, ToonlStream, ToonlWriter, Value,
};
use serde_json::Value as Json;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Fixtures come from the `toon-format/spec` submodule, so the corpus tracks
/// upstream instead of drifting from a vendored copy.
const FIXTURE_ROOT: &str = "../../vendor/toon-spec/tests/fixtures";
const LOCAL_FIXTURE_ROOT: &str = "../../tests/toon/fixtures";
const EXPECTED_FAILURE_LEDGER: &str = "../../tests/toon/expected-failures.txt";
const TOONL_FIXTURE_ROOT: &str = "../../tests/toonl/fixtures";

#[test]
fn official_toon_spec_fixtures_do_not_regress() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_root = manifest_dir.join(FIXTURE_ROOT);
    assert!(
        fixture_root.is_dir(),
        "spec fixtures missing at {} — run `git submodule update --init`",
        fixture_root.display()
    );
    let expected_failures = read_expected_failures(&manifest_dir.join(EXPECTED_FAILURE_LEDGER));
    let mut seen = BTreeSet::new();
    let mut unexpected_failures = Vec::new();
    let mut stale_expected_failures = Vec::new();

    let mut paths = fixture_paths(&fixture_root);
    paths.extend(fixture_paths(&manifest_dir.join(LOCAL_FIXTURE_ROOT)));
    paths.sort();

    for fixture_path in paths {
        let fixture = read_fixture(&fixture_path);
        let category = fixture
            .get("category")
            .and_then(Json::as_str)
            .expect("fixture category");
        let tests = fixture
            .get("tests")
            .and_then(Json::as_array)
            .expect("fixture tests");

        for test in tests {
            let name = test.get("name").and_then(Json::as_str).expect("test name");
            let id_root = if fixture_path.starts_with(&fixture_root) {
                &fixture_root
            } else {
                &manifest_dir
            };
            let id = fixture_id(id_root, &fixture_path, name);
            seen.insert(id.clone());

            // Every case declares the decoder options it is written against, and
            // a conformance run has to honour them: `expandPaths` cases are
            // otherwise unsatisfiable, since the same input must be rejected
            // under `strict` and resolve last-write-wins without it.
            let options = decoder_options(test.get("options"));

            let actual_passed = match category {
                "decode" => {
                    let input = test
                        .get("input")
                        .and_then(Json::as_str)
                        .expect("decode input");
                    let should_error = test
                        .get("shouldError")
                        .and_then(Json::as_bool)
                        .unwrap_or(false);

                    match (Value::parse_with_options(input, options), should_error) {
                        // A rejection the spec asked for.
                        (Err(_), true) => true,
                        (Ok(_), true) | (Err(_), false) => false,
                        // Parsing without an error is not enough. The decoded
                        // value has to be the one the spec says it is, and our
                        // own canonical output has to decode back to that same
                        // value — otherwise either the parser returns wrong data
                        // silently, or the serializer emits TOON we cannot read.
                        (Ok(value), false) => {
                            let decoded = value.to_json_value();
                            let matches_spec = test
                                .get("expected")
                                .is_some_and(|expected| decoded == *expected);
                            if matches_spec
                                && test
                                    .get("failClosedV3Strict")
                                    .and_then(Json::as_bool)
                                    .unwrap_or(false)
                            {
                                assert!(
                                    reject_v3_strict(input).is_err(),
                                    "{id}: strict v3 decoder must reject extension header"
                                );
                            }
                            matches_spec && round_trips_to(&value, &decoded)
                        }
                    }
                }
                "encode" => {
                    if test
                        .get("options")
                        .and_then(|options| options.get("keyedMapCollapse"))
                        .and_then(Json::as_bool)
                        .unwrap_or(false)
                    {
                        let input = test.get("input").expect("keyed map encode input");
                        let expected = test
                            .get("expected")
                            .and_then(Json::as_str)
                            .expect("encode expected TOON");
                        let value = Value::from_json_value(input.clone());
                        value.to_toon_with_options(encoder_options(test.get("options"))) == expected
                            && Value::parse_with_options(expected, options)
                                .is_ok_and(|actual| actual.to_json_value() == *input)
                    } else {
                        let expected = test
                            .get("expected")
                            .and_then(Json::as_str)
                            .expect("encode expected TOON");
                        parse_round_trips(expected, options).is_ok()
                    }
                }
                other => panic!("unknown fixture category {other}"),
            };

            let expected_to_fail = expected_failures.contains(&id);
            match (actual_passed, expected_to_fail) {
                (true, true) => stale_expected_failures.push(id),
                (false, false) => unexpected_failures.push(id),
                _ => {}
            }
        }
    }

    let unknown_expected_failures = expected_failures
        .difference(&seen)
        .cloned()
        .collect::<Vec<_>>();

    assert!(
        unexpected_failures.is_empty()
            && stale_expected_failures.is_empty()
            && unknown_expected_failures.is_empty(),
        "TOON conformance drift\nunexpected failures:\n{}\nstale expected failures:\n{}\nunknown expected failures:\n{}",
        format_ids(&unexpected_failures),
        format_ids(&stale_expected_failures),
        format_ids(&unknown_expected_failures)
    );
}

#[test]
fn toonl_fixtures_are_executable_spec_examples() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_root = manifest_dir.join(TOONL_FIXTURE_ROOT);
    assert!(
        fixture_root.is_dir(),
        "TOONL fixtures missing at {}",
        fixture_root.display()
    );

    for fixture_path in fixture_paths_for_category(&fixture_root, "") {
        let fixture = read_fixture(&fixture_path);
        let version = fixture
            .get("version")
            .and_then(Json::as_str)
            .unwrap_or_else(|| {
                panic!("{} declares the TOONL spec version", fixture_path.display())
            });
        assert!(
            matches!(version, "toonl-v0.1" | "toonl-v0.2"),
            "{} declares a supported TOONL spec version, got {version:?}",
            fixture_path.display()
        );
        assert_eq!(
            fixture.get("extension").and_then(Json::as_str),
            Some(".toonl"),
            "{} declares the canonical extension",
            fixture_path.display()
        );
        assert_eq!(
            fixture.get("mediaHint").and_then(Json::as_str),
            Some("application/toonl"),
            "{} declares the media hint",
            fixture_path.display()
        );

        let tests = fixture
            .get("tests")
            .and_then(Json::as_array)
            .expect("TOONL fixture tests");
        for test in tests {
            let name = test.get("name").and_then(Json::as_str).expect("test name");
            let kind = test.get("kind").and_then(Json::as_str).expect("test kind");
            let input = test.get("input").and_then(Json::as_str).unwrap_or_default();

            match kind {
                "decode" => {
                    let actual = ToonlStream::parse(input)
                        .unwrap_or_else(|err| panic!("{name}: decode failed: {err}"));
                    let expected = test.get("segments").expect("decode segments");
                    assert_eq!(
                        segments_to_json(&actual),
                        *expected,
                        "{name}: decoded segments"
                    );
                }
                "encode" => {
                    let expected = test
                        .get("expected")
                        .and_then(Json::as_str)
                        .expect("encode expected");
                    assert_eq!(encode_toonl_fixture(test), expected, "{name}: encoded");
                }
                "encode-records" => {
                    let expected = test
                        .get("expected")
                        .and_then(Json::as_str)
                        .expect("encode expected");
                    assert_eq!(
                        encode_toonl_records_fixture(test),
                        expected,
                        "{name}: encoded"
                    );
                }
                "encode-tagged-records" => {
                    let expected = test
                        .get("expected")
                        .and_then(Json::as_str)
                        .expect("encode expected");
                    assert_eq!(
                        encode_tagged_toonl_records_fixture(test),
                        expected,
                        "{name}: encoded"
                    );
                }
                "close-transform" => {
                    let expected = test
                        .get("expectedToonDocuments")
                        .and_then(Json::as_array)
                        .expect("expected TOON documents");
                    let actual = ToonlStream::parse(input)
                        .and_then(|stream| stream.close_transform_documents())
                        .unwrap_or_else(|err| panic!("{name}: close-transform failed: {err}"));
                    let expected_strings = expected
                        .iter()
                        .map(|value| {
                            value
                                .as_str()
                                .unwrap_or_else(|| panic!("{name}: expected TOON doc string"))
                                .to_owned()
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(actual, expected_strings, "{name}: transformed docs");
                    for document in actual {
                        Value::parse_toon(&document)
                            .unwrap_or_else(|err| panic!("{name}: TOON output invalid: {err}"));
                    }
                    if let Some(expected) = test
                        .get("expectedInterleavedToonDocuments")
                        .and_then(Json::as_array)
                    {
                        let actual = ToonlStream::parse(input)
                            .and_then(|stream| stream.close_transform_interleaved_documents())
                            .unwrap_or_else(|err| {
                                panic!("{name}: interleaved close-transform failed: {err}")
                            });
                        let expected_strings = expected
                            .iter()
                            .map(|value| {
                                value
                                    .as_str()
                                    .unwrap_or_else(|| {
                                        panic!("{name}: expected interleaved TOON doc string")
                                    })
                                    .to_owned()
                            })
                            .collect::<Vec<_>>();
                        assert_eq!(
                            actual, expected_strings,
                            "{name}: interleaved transformed docs"
                        );
                        for document in actual {
                            Value::parse_toon(&document).unwrap_or_else(|err| {
                                panic!("{name}: interleaved TOON output invalid: {err}")
                            });
                        }
                    }
                }
                "error" => {
                    let expected = test
                        .get("expectedError")
                        .and_then(Json::as_str)
                        .expect("expected error");
                    let actual = ToonlStream::parse(input)
                        .and_then(|stream| stream.close_transform_documents())
                        .expect_err("error fixture must be rejected");
                    let actual = actual.to_string();
                    assert!(
                        actual.contains(expected),
                        "{name}: expected error containing {expected:?}, got {actual:?}"
                    );
                }
                "v0.1-error" => {
                    let expected = test
                        .get("expectedError")
                        .and_then(Json::as_str)
                        .expect("expected error");
                    let actual = reject_v0_1_toonl(input).expect_err("v0.1 fixture must reject");
                    assert!(
                        actual.contains(expected),
                        "{name}: expected v0.1 error containing {expected:?}, got {actual:?}"
                    );
                }
                other => panic!("{name}: unknown TOONL fixture kind {other}"),
            }
        }
    }
}

/// Maps a fixture's `options` object onto decoder options. Encoder-only options
/// (`delimiter`, `keyFolding`, `flattenDepth`) carry no decoder meaning and are
/// ignored; `indent` is shared by both sides.
fn decoder_options(options: Option<&Json>) -> ParseOptions {
    let defaults = ParseOptions::default();
    let Some(options) = options.and_then(Json::as_object) else {
        return defaults;
    };

    ParseOptions {
        indent: options
            .get("indent")
            .and_then(Json::as_u64)
            .map_or(defaults.indent, |indent| indent as usize),
        strict: options
            .get("strict")
            .and_then(Json::as_bool)
            .unwrap_or(defaults.strict),
        expand_paths: options
            .get("expandPaths")
            .and_then(Json::as_str)
            .is_some_and(|mode| mode == "safe"),
        ..defaults
    }
}

fn encoder_options(options: Option<&Json>) -> reddb_io_toon::EncodeOptions {
    let Some(options) = options.and_then(Json::as_object) else {
        return reddb_io_toon::EncodeOptions::default();
    };

    reddb_io_toon::EncodeOptions {
        nested_tabular_headers: options
            .get("nestedTabularHeaders")
            .and_then(Json::as_bool)
            .unwrap_or(false),
        keyed_map_collapse: options
            .get("keyedMapCollapse")
            .and_then(Json::as_bool)
            .unwrap_or(false),
        delimiter: options
            .get("delimiter")
            .and_then(Json::as_str)
            .and_then(|delimiter| delimiter.chars().next())
            .unwrap_or(','),
        ..reddb_io_toon::EncodeOptions::default()
    }
}

/// The canonical output is always written in the default profile, so it is
/// re-read with default options no matter what the fixture's input used.
fn canonical_options() -> ParseOptions {
    ParseOptions::default()
}

fn reject_v3_strict(input: &str) -> Result<(), String> {
    for (index, line) in input.lines().enumerate() {
        let trimmed = line.trim_start();
        if let Some(colon) = trimmed.find(':') {
            let key_part = &trimmed[..colon];
            if key_part.contains('{') && key_part.ends_with('}') && !key_part.contains('[') {
                return Err(format!("line {}: invalid keyed map header", index + 1));
            }
            if let Some(fields_start) = key_part.find('{') {
                if key_part[fields_start..].contains('[') {
                    return Err(format!("line {}: invalid array header", index + 1));
                }
            }
        }
    }
    Ok(())
}

fn fixture_paths(root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for category in ["decode", "encode"] {
        let dir = root.join(category);
        paths.extend(fixture_paths_for_category(&dir, ""));
    }
    paths.sort();
    paths
}

fn fixture_paths_for_category(root: &Path, category: &str) -> Vec<PathBuf> {
    let dir = if category.is_empty() {
        root.to_path_buf()
    } else {
        root.join(category)
    };
    let mut paths = Vec::new();
    for entry in fs::read_dir(&dir).unwrap_or_else(|err| panic!("read {}: {err}", dir.display())) {
        let path = entry.expect("fixture dir entry").path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            paths.push(path);
        }
    }
    paths.sort();
    paths
}

fn segments_to_json(segments: &ToonlStream) -> Json {
    Json::Array(
        segments
            .segments()
            .iter()
            .map(|segment| {
                serde_json::json!({
                    "delimiter": segment.delimiter().to_string(),
                    "fields": segment.fields(),
                    "rows": segment.rows(),
                })
            })
            .collect(),
    )
}

fn encode_toonl_fixture(test: &Json) -> String {
    let delimiter = test
        .get("delimiter")
        .and_then(Json::as_str)
        .and_then(|value| value.chars().next())
        .unwrap_or(',');
    let fields = test
        .get("fields")
        .and_then(Json::as_array)
        .expect("encode fields")
        .iter()
        .map(|value| value.as_str().expect("field string"))
        .collect::<Vec<_>>();
    let rows = test
        .get("rows")
        .and_then(Json::as_array)
        .expect("encode rows");

    let mut encoder = ToonlEncoder::new(delimiter, &fields).expect("valid TOONL encoder");
    if let Some(rows) = test.get("continuationEveryRows").and_then(Json::as_u64) {
        encoder
            .set_continuation_every_rows(Some(rows as usize))
            .expect("valid row continuation cadence");
    }
    for row in rows {
        let cells = row
            .as_array()
            .expect("row array")
            .iter()
            .map(|value| value.as_str().expect("cell string"))
            .collect::<Vec<_>>();
        encoder.push_raw_row(&cells).expect("valid TOONL row");
    }
    encoder.finish()
}

fn reject_v0_1_toonl(input: &str) -> Result<(), String> {
    for (offset, raw_line) in input.lines().enumerate() {
        let line_number = offset + 1;
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if line.is_empty() {
            continue;
        }
        if !line.starts_with('[') {
            if looks_like_tagged_row(line) {
                return Err(format!("line {line_number}: row arity mismatch"));
            }
            continue;
        }
        let Some(close_bracket) = line[1..].find(']') else {
            return Err(format!("line {line_number}: invalid header"));
        };
        let bracket = &line[1..close_bracket + 1];
        if bracket.starts_with('=') {
            continue;
        }
        if !matches!(bracket, "" | "|" | "\t") {
            return Err(format!("line {line_number}: invalid header delimiter"));
        }
        let suffix = &line[close_bracket + 2..];
        if !suffix.starts_with('{') || !suffix.ends_with("}:") {
            return Err(format!("line {line_number}: invalid header"));
        }
    }
    Ok(())
}

fn looks_like_tagged_row(line: &str) -> bool {
    let Some(colon) = line.find(':') else {
        return false;
    };
    colon > 0
        && line[..colon]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn encode_toonl_records_fixture(test: &Json) -> String {
    let records = test
        .get("records")
        .and_then(Json::as_array)
        .expect("encode records")
        .iter()
        .cloned()
        .map(Value::from_json_value)
        .collect::<Vec<_>>();

    encode_toonl_values(&records).expect("valid TOONL records")
}

fn encode_tagged_toonl_records_fixture(test: &Json) -> String {
    let mut output = Vec::new();
    {
        let mut writer = ToonlWriter::new(&mut output);
        let operations = test
            .get("operations")
            .and_then(Json::as_array)
            .expect("tagged encode operations");
        for operation in operations {
            let tag = operation
                .get("tag")
                .and_then(Json::as_str)
                .expect("tagged encode tag");
            let record = operation
                .get("record")
                .cloned()
                .map(Value::from_json_value)
                .expect("tagged encode record");
            writer
                .write_tagged_record(tag, &record)
                .expect("valid tagged TOONL record");
        }
        writer.finish().expect("finish tagged writer");
    }
    String::from_utf8(output).expect("TOONL is UTF-8")
}

fn read_fixture(path: &Path) -> Json {
    let json =
        fs::read_to_string(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    serde_json::from_str(&json).unwrap_or_else(|err| panic!("parse {}: {err}", path.display()))
}

fn fixture_id(root: &Path, path: &Path, name: &str) -> String {
    let relative = path
        .strip_prefix(root)
        .expect("fixture path under root")
        .to_string_lossy()
        .replace('\\', "/");
    format!("{relative}::{name}")
}

/// Our canonical output has to decode back to the value we started from.
fn round_trips_to(value: &Value, decoded: &Json) -> bool {
    Value::parse_with_options(&value.to_canonical_toon(), canonical_options())
        .is_ok_and(|reparsed| reparsed.to_json_value() == *decoded)
}

fn parse_round_trips(input: &str, options: ParseOptions) -> Result<(), String> {
    let value = Value::parse_with_options(input, options).map_err(|err| err.to_string())?;
    let canonical = value.to_canonical_toon();
    let reparsed = Value::parse_with_options(&canonical, canonical_options())
        .map_err(|err| format!("canonical output did not parse: {err}"))?;
    if reparsed.to_json_value() != value.to_json_value() {
        return Err("canonical output did not preserve the decoded value".to_owned());
    }
    Ok(())
}

fn read_expected_failures(path: &Path) -> BTreeSet<String> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(ToOwned::to_owned)
        .collect()
}

fn format_ids(ids: &[String]) -> String {
    if ids.is_empty() {
        return "  (none)".to_owned();
    }

    ids.iter()
        .map(|id| format!("  {id}"))
        .collect::<Vec<_>>()
        .join("\n")
}
