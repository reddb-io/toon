use reddb_io_toon::Value;
use serde_json::Value as Json;
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

const JSON_LIMITS_FIXTURE: &str = "../../tests/json-limits/corpus.json";
const EXPECTED_CASE_COUNT: usize = 24;
const REQUIRED_CATEGORIES: [&str; 4] = [
    "numbers",
    "strings-unicode",
    "structure",
    "adversarial-round-trip",
];

#[test]
fn json_limits_corpus_resolves_consistently_for_rust() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_path = manifest_dir.join(JSON_LIMITS_FIXTURE);
    let fixture = read_fixture(&fixture_path);

    assert_eq!(
        fixture.get("version").and_then(Json::as_str),
        Some("json-limits-v0.1")
    );

    let tests = fixture
        .get("tests")
        .and_then(Json::as_array)
        .expect("JSON limits tests");
    let mut categories = BTreeSet::new();

    for test in tests {
        let name = test.get("name").and_then(Json::as_str).expect("test name");
        let category = test
            .get("category")
            .and_then(Json::as_str)
            .expect("test category");
        categories.insert(category.to_owned());

        let raw_json = test
            .get("rawJson")
            .and_then(Json::as_str)
            .expect("raw JSON input");
        let expected = test
            .get("expected")
            .and_then(|value| value.get("rust"))
            .expect("Rust expectation");

        if let Some(expected_error) = expected.get("error").and_then(Json::as_str) {
            let actual = Value::from_json_str(raw_json).expect_err("case must reject");
            assert!(
                actual.to_string().contains(expected_error),
                "{name}: expected error containing {expected_error:?}, got {actual}"
            );
            continue;
        }

        let value =
            Value::from_json_str(raw_json).unwrap_or_else(|err| panic!("{name}: JSON parse: {err}"));
        let toon = value.to_canonical_toon();
        assert_eq!(
            toon,
            expected
                .get("toon")
                .and_then(Json::as_str)
                .expect("expected canonical TOON"),
            "{name}: canonical TOON"
        );

        let actual_round_trip = Value::parse_toon(&toon)
            .unwrap_or_else(|err| panic!("{name}: TOON parse: {err}"))
            .to_json_value();
        let expected_round_trip: Json = serde_json::from_str(
            expected
                .get("roundTripJson")
                .and_then(Json::as_str)
                .expect("expected round-trip JSON"),
        )
        .unwrap_or_else(|err| panic!("{name}: expected round-trip JSON parse: {err}"));
        assert_eq!(
            actual_round_trip, expected_round_trip,
            "{name}: round-trip JSON"
        );
    }

    assert_eq!(tests.len(), EXPECTED_CASE_COUNT, "JSON limits case count changed");
    assert_eq!(
        categories,
        REQUIRED_CATEGORIES
            .iter()
            .map(|category| category.to_string())
            .collect::<BTreeSet<_>>(),
        "all JSON limits categories are covered"
    );
}

fn read_fixture(path: &PathBuf) -> Json {
    let json =
        fs::read_to_string(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    serde_json::from_str(&json).unwrap_or_else(|err| panic!("parse {}: {err}", path.display()))
}
