use reddb_io_toon::Document;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Fixtures come from the `toon-format/spec` submodule, so the corpus tracks
/// upstream instead of drifting from a vendored copy.
const FIXTURE_ROOT: &str = "../../vendor/toon-spec/tests/fixtures";
const EXPECTED_FAILURE_LEDGER: &str = "tests/expected-failures.txt";

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
            .and_then(Value::as_str)
            .expect("fixture category");
        let tests = fixture
            .get("tests")
            .and_then(Value::as_array)
            .expect("fixture tests");

        for test in tests {
            let name = test.get("name").and_then(Value::as_str).expect("test name");
            let id = fixture_id(&fixture_root, &fixture_path, name);
            seen.insert(id.clone());

            let actual_passed = match category {
                "decode" => {
                    let input = test
                        .get("input")
                        .and_then(Value::as_str)
                        .expect("decode input");
                    let should_error = test
                        .get("shouldError")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);

                    match (Document::parse(input), should_error) {
                        // A rejection the spec asked for.
                        (Err(_), true) => true,
                        (Ok(_), true) | (Err(_), false) => false,
                        // Parsing without an error is not enough. The decoded
                        // value has to be the one the spec says it is, and our
                        // own canonical output has to decode back to that same
                        // value — otherwise either the parser returns wrong data
                        // silently, or the serializer emits TOON we cannot read.
                        (Ok(document), false) => {
                            let decoded = document.to_json_value();
                            let matches_spec = test
                                .get("expected")
                                .is_some_and(|expected| decoded == *expected);
                            matches_spec && round_trips_to(&document, &decoded)
                        }
                    }
                }
                "encode" => {
                    let expected = test
                        .get("expected")
                        .and_then(Value::as_str)
                        .expect("encode expected TOON");
                    parse_round_trips(expected).is_ok()
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

fn read_fixture(path: &Path) -> Value {
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
fn round_trips_to(document: &Document, decoded: &Value) -> bool {
    Document::parse(&document.to_canonical_toon())
        .is_ok_and(|reparsed| reparsed.to_json_value() == *decoded)
}

fn parse_round_trips(input: &str) -> Result<(), String> {
    let document = Document::parse(input).map_err(|err| err.to_string())?;
    let canonical = document.to_canonical_toon();
    Document::parse(&canonical)
        .map(|_| ())
        .map_err(|err| format!("canonical output did not parse: {err}"))
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
