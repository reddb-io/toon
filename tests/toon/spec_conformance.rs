use reddb_io_toon::{ParseOptions, Value};
use serde_json::Value as Json;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Fixtures come from the `toon-format/spec` submodule, so the corpus tracks
/// upstream instead of drifting from a vendored copy.
const FIXTURE_ROOT: &str = "../../vendor/toon-spec/tests/fixtures";
const EXPECTED_FAILURE_LEDGER: &str = "../../tests/toon/expected-failures.txt";

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

    for fixture_path in fixture_paths(&fixture_root) {
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
            let id = fixture_id(&fixture_root, &fixture_path, name);
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
                            matches_spec && round_trips_to(&value, &decoded)
                        }
                    }
                }
                "encode" => {
                    let expected = test
                        .get("expected")
                        .and_then(Json::as_str)
                        .expect("encode expected TOON");
                    parse_round_trips(expected, options).is_ok()
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
    }
}

/// The canonical output is always written in the default profile, so it is
/// re-read with default options no matter what the fixture's input used.
fn canonical_options() -> ParseOptions {
    ParseOptions::default()
}

fn fixture_paths(root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for category in ["decode", "encode"] {
        let dir = root.join(category);
        for entry in
            fs::read_dir(&dir).unwrap_or_else(|err| panic!("read {}: {err}", dir.display()))
        {
            let path = entry.expect("fixture dir entry").path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                paths.push(path);
            }
        }
    }
    paths.sort();
    paths
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
