//! Performance smoke guards for the known pathological payloads.
//!
//! These are *shape* guards, not measurements: each bound is deliberately
//! loose enough that a slow or loaded CI machine never flakes, while the
//! quadratic behaviour they protect against — per-character string building,
//! re-scanning a row for every cell — blows past them by orders of magnitude.
//! The mirror of `packages/toon/test/html-payload.test.mjs` on the Rust side.
//!
//! Detailed numbers belong in `cargo bench` (`crates/toon/benches/codec.rs`),
//! never in this gate.

use std::time::Instant;

use reddb_io_toon::Value;

/// Dense-quote markup: the payload class behind the GC-thrashing encode
/// regression fixed in #194.
const HTML_BLOCK: &str = concat!(
    "<section class=\"prose dark\" data-config='{\"key\": [1, 2]}'>\n",
    "  <script>if (a < b && c > d) { log(\"x, y: z\", '\\\\srv\\\\share'); }</script>\n",
    "  <p>Inline \"quoted\" text, commas: colons; <a href=\"https://example.com?a=1&b=2\">link</a></p>\n",
    "</section>\n",
);

fn html_string(target_bytes: usize) -> String {
    HTML_BLOCK.repeat(target_bytes.div_ceil(HTML_BLOCK.len()))
}

/// Asserts a loose wall-clock ceiling, reporting the real cost on failure.
fn assert_within(budget_ms: u128, label: &str, work: impl FnOnce()) {
    let started = Instant::now();
    work();
    let elapsed = started.elapsed();
    assert!(
        elapsed.as_millis() < budget_ms,
        "{label} took {}ms, over the {budget_ms}ms smoke budget — \
         suspect a quadratic regression (measure with `cargo bench -p reddb-io-toon`)",
        elapsed.as_millis(),
    );
}

#[test]
fn multi_megabyte_html_strings_encode_and_decode_without_collapsing() {
    let body = html_string(800_000);
    let json = serde_json::json!({
        "rows": (0..8)
            .map(|index| serde_json::json!({ "id": index, "body": body }))
            .collect::<Vec<_>>(),
    });
    let value = Value::from_json_value(json.clone());

    assert_within(120_000, "6MB of HTML encode+decode", || {
        let wire = value.try_to_canonical_toon().expect("encode");
        let decoded = Value::parse_toon(&wire).expect("decode");
        assert_eq!(decoded.to_json_value(), json);
    });
}

/// A single enormous scalar: the encoder must copy runs, not characters.
#[test]
fn one_huge_dense_quoted_string_round_trips() {
    let json = serde_json::json!({ "body": html_string(4_000_000) });
    let value = Value::from_json_value(json.clone());

    assert_within(120_000, "4MB single-string encode+decode", || {
        let wire = value.try_to_canonical_toon().expect("encode");
        let decoded = Value::parse_toon(&wire).expect("decode");
        assert_eq!(decoded.to_json_value(), json);
    });
}

/// Wide tabular data: rows must not be re-scanned per cell.
#[test]
fn a_long_tabular_array_encodes_and_decodes_linearly() {
    let json = serde_json::json!({
        "rows": (0..20_000)
            .map(|index| {
                serde_json::json!({
                    "id": index,
                    "name": format!("record-{index}"),
                    "note": "text, with a comma and \"quotes\"",
                    // Never integer-valued: SPEC §2 folds an integral float to
                    // its integer form, which is a number-semantics concern the
                    // spec corpus owns, not this timing guard's business.
                    "score": index as f64 + 0.5,
                    "ok": index % 2 == 0,
                })
            })
            .collect::<Vec<_>>(),
    });
    let value = Value::from_json_value(json.clone());

    assert_within(60_000, "20k-row tabular encode+decode", || {
        let wire = value.try_to_canonical_toon().expect("encode");
        let decoded = Value::parse_toon(&wire).expect("decode");
        assert_eq!(decoded.to_json_value(), json);
    });
}

/// Deep nesting: the recursive paths must not rebuild their prefix per level.
#[test]
fn deeply_nested_objects_encode_and_decode() {
    let mut json = serde_json::json!({ "leaf": "value, with delimiter" });
    for _ in 0..60 {
        json = serde_json::json!({ "child": json });
    }
    let value = Value::from_json_value(json.clone());

    assert_within(30_000, "60-level nesting encode+decode", || {
        let wire = value.try_to_canonical_toon().expect("encode");
        let decoded = Value::parse_toon(&wire).expect("decode");
        assert_eq!(decoded.to_json_value(), json);
    });
}
