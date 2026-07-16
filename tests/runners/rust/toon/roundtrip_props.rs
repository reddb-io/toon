//! Property-based round-trip proofs.
//!
//! The spec corpus pins the shapes the spec cares about; these properties
//! cover the space between them — strings carrying unicode, control and
//! quoting characters, numbers at the edges of the JSON grammar, and nesting
//! deep enough to exercise the recursive encoder paths.
//!
//! Two properties, both over arbitrary values:
//!
//! * `json -> toon -> json` preserves the value exactly.
//! * `toon -> value -> toon` is a fixed point: re-encoding a decoded document
//!   reproduces the same wire bytes.
//!
//! A failure prints a minimal reproducing case and persists it to
//! `tests/runners/rust/toon/roundtrip_props.proptest-regressions`, which is
//! committed so the shrunk case becomes a permanent regression test.

use proptest::prelude::*;
use reddb_io_toon::Value;

/// JSON-model equality as SPEC §2 defines it for `decode(encode(x)) == x`:
/// arrays by length and order, objects by the same *ordered* key sequence, and
/// numbers "by mathematical value after §2 numeric normalization, so -0 equals
/// 0 and integer-valued numbers compare equal to their integer form".
///
/// `serde_json`'s own `PartialEq` is the wrong oracle here: it compares a
/// number's storage tag, so it reports `-0.0 != 0` and `1e19_f64 != 1e19_u64`
/// even though §2 mandates the first normalization and permits the second.
fn json_model_eq(left: &serde_json::Value, right: &serde_json::Value) -> bool {
    use serde_json::Value as J;
    match (left, right) {
        (J::Number(left), J::Number(right)) => number_eq(left, right),
        (J::Array(left), J::Array(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| json_model_eq(left, right))
        }
        (J::Object(left), J::Object(right)) => {
            left.len() == right.len()
                && left.iter().zip(right).all(
                    |((left_key, left_value), (right_key, right_value))| {
                        left_key == right_key && json_model_eq(left_value, right_value)
                    },
                )
        }
        _ => left == right,
    }
}

fn number_eq(left: &serde_json::Number, right: &serde_json::Number) -> bool {
    // Integers first, so precision beyond f64 (which the encoder preserves
    // verbatim) is compared exactly rather than through a lossy widening.
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

/// Characters that push the encoder onto every branch of its quoting decision:
/// the delimiter, the quote itself, escapes, structural punctuation,
/// whitespace that is significant at a line edge, C0 controls, and multi-byte
/// codepoints (including one above the BMP).
const SPICY: &str = "\"'\\,:[]{}# \t\n\r\u{0}\u{1f}\u{7f}áé中🙂";

fn key_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        // Plain keys: the common path, and the only one a bare key can take.
        "[a-zA-Z_][a-zA-Z0-9_]{0,12}",
        // Keys that must survive quoting.
        prop::collection::vec(
            prop::sample::select(SPICY.chars().collect::<Vec<_>>()),
            1..6
        )
        .prop_map(|chars| chars.into_iter().collect()),
        // The empty key is legal JSON and a known encoder edge.
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
        // Strings that look like other TOON scalars must stay strings.
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
        ])
        .prop_map(str::to_owned),
    ]
}

fn number_strategy() -> impl Strategy<Value = serde_json::Value> {
    prop_oneof![
        any::<i64>().prop_map(|value| serde_json::json!(value)),
        any::<u64>().prop_map(|value| serde_json::json!(value)),
        // Only finite doubles are representable in JSON; serde_json rejects
        // NaN and the infinities at construction, so they are out of scope.
        any::<f64>()
            .prop_filter("JSON has no NaN or infinity", |value| value.is_finite())
            .prop_map(|value| serde_json::json!(value)),
        prop::sample::select(vec![0i64, -1, i64::MIN, i64::MAX]).prop_map(|v| serde_json::json!(v)),
    ]
}

fn scalar_strategy() -> impl Strategy<Value = serde_json::Value> {
    prop_oneof![
        Just(serde_json::Value::Null),
        any::<bool>().prop_map(|value| serde_json::json!(value)),
        number_strategy(),
        string_strategy().prop_map(serde_json::Value::String),
    ]
}

/// Arbitrary JSON values, nested up to 6 levels deep.
fn value_strategy() -> impl Strategy<Value = serde_json::Value> {
    scalar_strategy().prop_recursive(6, 48, 6, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..6).prop_map(serde_json::Value::Array),
            prop::collection::vec((key_strategy(), inner), 0..6)
                .prop_map(|entries| { serde_json::Value::Object(entries.into_iter().collect()) }),
        ]
    })
}

/// The encoder's document root is an object, which is also the shape every
/// real payload takes.
fn document_strategy() -> impl Strategy<Value = serde_json::Value> {
    prop::collection::vec((key_strategy(), value_strategy()), 0..6)
        .prop_map(|entries| serde_json::Value::Object(entries.into_iter().collect()))
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn json_to_toon_to_json_preserves_the_value(json in document_strategy()) {
        let value = Value::from_json_value(json.clone());
        let wire = value.try_to_canonical_toon().expect("canonical encode");
        let decoded = Value::parse_toon(&wire).expect("decode of self-produced wire").to_json_value();
        prop_assert!(
            json_model_eq(&decoded, &json),
            "round-trip changed the value\n  json:    {json}\n  wire:    {wire:?}\n  decoded: {decoded}",
        );
    }

    #[test]
    fn re_encoding_a_decoded_document_is_a_fixed_point(json in document_strategy()) {
        let wire = Value::from_json_value(json)
            .try_to_canonical_toon()
            .expect("canonical encode");
        let re_encoded = Value::parse_toon(&wire)
            .expect("decode of self-produced wire")
            .try_to_canonical_toon()
            .expect("re-encode");
        prop_assert_eq!(re_encoded, wire);
    }
}
