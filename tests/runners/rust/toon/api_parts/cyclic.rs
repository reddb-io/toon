#[test]
fn exposes_json_conversions_in_both_directions() {
    let value = Value::from_json_str(r#"{"a":[1,2],"b":null}"#).expect("valid JSON");

    assert_eq!(value.to_json_value(), json!({"a": [1, 2], "b": null}));
    assert_eq!(
        value.to_json_string(true).expect("compact JSON"),
        r#"{"a":[1,2],"b":null}"#
    );
    assert_eq!(
        value.to_json_string(false).expect("pretty JSON"),
        "{\n  \"a\": [\n    1,\n    2\n  ],\n  \"b\": null\n}"
    );
    assert!(Value::from_json_str("{oops").is_err());

    // A number token that is not a number at all degrades to a string rather
    // than panicking.
    assert_eq!(
        Value::Number("not-a-number".to_owned()).to_json_value(),
        json!("not-a-number")
    );
}

#[test]
fn narrows_values_to_objects_and_arrays() {
    let value = parse("a[2]: 1,2\nb: x\n");
    let document = value.as_object().expect("root is an object");

    assert_eq!(document.len(), 2);
    assert!(!document.is_empty());
    assert!(Document::default().is_empty());
    assert!(document.get("missing").is_none());
    assert!(document.get("b").expect("b").as_array().is_none());
    assert!(document.get("b").expect("b").as_object().is_none());

    let array = document
        .get("a")
        .expect("a")
        .as_array()
        .expect("a is array");
    assert_eq!(array.len(), 2);
    assert!(!array.is_empty());
    assert!(Array::List(Vec::new()).is_empty());
    assert_eq!(array.get(0).expect("first").to_json_value(), json!(1));
    assert!(array.get(9).is_none());
    assert_eq!(array.to_json_value(), json!([1, 2]));
    assert_eq!(array.to_canonical_toon(), "[2]: 1,2\n");
}

#[test]
fn slices_both_array_representations_and_clamps_out_of_range_bounds() {
    let list = parse("a[3]: 1,2,3\n")
        .as_object()
        .and_then(|document| document.get("a"))
        .and_then(Value::as_array)
        .expect("list array")
        .clone();
    let table = parse("a[3]{id}:\n  1\n  2\n  3\n")
        .as_object()
        .and_then(|document| document.get("a"))
        .and_then(Value::as_array)
        .expect("tabular array")
        .clone();

    for array in [&list, &table] {
        assert_eq!(array.slice(Some(1), Some(3)).len(), 2);
        assert_eq!(array.slice(None, None).len(), 3);
        // An inverted range and an out-of-range end both clamp to empty/full.
        assert!(array.slice(Some(3), Some(1)).is_empty());
        assert_eq!(array.slice(Some(0), Some(99)).len(), 3);
        assert!(array.slice(Some(99), None).is_empty());
    }

    assert_eq!(
        table.slice(Some(1), Some(2)).to_json_value(),
        json!([{"id": 2}])
    );
    assert_eq!(list.values().len(), 3);
}

#[test]
fn document_parse_accepts_object_roots_and_rejects_the_others() {
    let document = Document::parse("a: 1\n").expect("object root");
    assert_eq!(document.to_json_value(), json!({"a": 1}));
    assert_eq!(document.to_canonical_toon(), "a: 1\n");

    assert!(Document::parse("[2]: 1,2\n").is_err());
    assert!(Document::parse("hello\n").is_err());

    let document = Document::parse_with_options(
        "a.b: 1\n",
        ParseOptions {
            expand_paths: true,
            ..ParseOptions::default()
        },
    )
    .expect("expanded object root");
    assert_eq!(document.to_json_value(), json!({"a": {"b": 1}}));
}

// ---------------------------------------------------------------------------
// Remaining decoder corners
// ---------------------------------------------------------------------------

#[test]
fn rejects_a_keyless_header_and_a_malformed_key_outside_root_position() {
    // Only the root may carry a header with no key.
    assert_eq!(
        error("a:\n  [2]: 1,2\n"),
        "line 2: expected non-empty field name"
    );
    // An unquoted key may not contain whitespace or a stray quote.
    assert_eq!(error("a b: 1\n"), "line 1: expected non-empty field name");
    // A bad header on the first line is an error at root too, not a literal key.
    assert_eq!(error("[03]: a\n"), "line 1: invalid array header");
    // A delimiter symbol the spec does not define.
    assert_eq!(error("items[2x]: a,b\n"), "line 1: invalid array header");
}

#[test]
fn a_key_value_line_at_row_depth_ends_the_rows() {
    // The sibling field stops the row scan, so the declared count goes unmet.
    assert_eq!(
        error("items[2]{id}:\n  1\nother: 2\n"),
        "line 3: array length mismatch"
    );
}

#[test]
fn a_row_is_still_a_row_when_a_cell_holds_a_colon() {
    // The active delimiter comes before the colon, so §9.3 reads this as a row.
    assert_eq!(
        json_of("r[1]{a,b}:\n  1,x:y\n"),
        json!({"r": [{"a": 1, "b": "x:y"}]})
    );
}

#[test]
fn a_single_column_tabular_row_that_looks_like_a_field_ends_the_rows_early() {
    // With no delimiter in the row content at all, §9.3 has nothing to compare
    // the colon's position against, so it reads the line as a sibling field
    // rather than a row — one short of the declared length.
    assert_eq!(
        error("items[2]{a}:\n  1\n  x: 2\n"),
        "line 3: array length mismatch"
    );
}

#[test]
fn non_strict_mode_reads_a_malformed_nested_array_header_as_a_literal_key() {
    // `- [x] : 1` looks like a nested-array list item (§9.4), but `[x]` is not
    // a valid header (no length digits), so it falls through. In non-strict
    // mode the whole bracketed prefix becomes a literal object key instead of
    // an error.
    let options = ParseOptions {
        strict: false,
        ..ParseOptions::default()
    };
    let value =
        Value::parse_with_options("items[1]:\n  - [x] : 1\n", options).expect("literal key item");
    assert_eq!(value.to_json_value(), json!({"items": [{"[x]": 1}]}));
}

#[test]
fn rejects_a_literal_control_character_inside_a_quoted_string() {
    assert_eq!(
        error("v: \"a\u{0001}b\"\n"),
        "line 1: invalid quoted string"
    );
}

#[test]
fn a_number_shaped_token_that_is_not_a_number_stays_a_string() {
    // Each of these trips a different arm of the numeric scanner.
    let cases = ["-", ".5", "1.", "1e", "1e+", "12abc", "--1", "1.2.3"];

    for token in cases {
        let input = format!("v: {token}");
        assert_eq!(
            json_of(&input),
            json!({"v": token}),
            "{token} is not a number"
        );
    }
}

// ---------------------------------------------------------------------------
// Header, keyed-map and structured-row error branches

#[test]
fn rejects_malformed_keyed_map_headers_in_strict_mode() {
    let cases = [
        ("m{a: 1\n", "line 1: invalid keyed map header"),
        ("m{}:\n", "line 1: invalid keyed map header"),
        ("m{a,a}: \n", "line 1: duplicate key"),
        ("{a}:\n", "line 1: expected non-empty field name"),
        ("m{a}: 1\n", "line 1: expected keyed map rows"),
    ];
    for (input, expected) in cases {
        assert_eq!(error(input), expected, "{input:?}");
    }
}

#[test]
fn rejects_malformed_keyed_map_rows() {
    let cases = [
        (
            "m{a,b}:\n  k: 1,2\n    j: 3,4\n",
            "line 3: invalid indentation",
        ),
        (
            "m{a,b}:\n  k: 1,2\n\n  j: 3,4\n",
            "line 4: blank line inside keyed map",
        ),
        (
            "m{a,b}:\n  k: 1,2,3\n",
            "line 2: keyed map row length mismatch",
        ),
        ("m{a,b}:\n  k: 1,2\n  k: 3,4\n", "line 3: duplicate key"),
    ];
    for (input, expected) in cases {
        assert_eq!(error(input), expected, "{input:?}");
    }
}

#[test]
fn decodes_keyed_map_edge_shapes() {
    // No rows at all is an empty map, and a following sibling still parses.
    assert_eq!(json_of("m{a,b}:\nnext: 1\n"), json!({"m": {}, "next": 1}));
    // Quoted field names and non-default delimiters stay legible.
    assert_eq!(
        json_of("m{\"q\",b}:\n  k: 1,2\n"),
        json!({"m": {"k": {"q": 1, "b": 2}}})
    );
    assert_eq!(
        json_of("m{|a|b}:\n  k: 1|2\n"),
        json!({"m": {"k": {"a": 1, "b": 2}}})
    );
    assert_eq!(
        json_of("m{\ta\tb}:\n  k: 1\t2\n"),
        json!({"m": {"k": {"a": 1, "b": 2}}})
    );
}

#[test]
fn rejects_malformed_array_column_field_lists() {
    let cases = [
        "items[1]{,a}:\n  1\n",
        "items[1]{a,}:\n  1\n",
        "items[1]{a[}:\n  1\n",
        "items[1]{a[,]}:\n  1\n",
        "items[1]{a{}}:\n  1\n",
    ];
    for input in cases {
        assert_eq!(error(input), "line 1: invalid array header", "{input:?}");
    }
    assert_eq!(error("items[1]{a,a}:\n  1,2\n"), "line 1: duplicate key");
}

#[test]
fn decodes_primitive_list_columns_with_declared_sub_delimiters() {
    assert_eq!(
        json_of("items[1]{a,tags[;]}:\n  1,x;y\n"),
        json!({"items": [{"a": 1, "tags": ["x", "y"]}]})
    );
    // Any sub-delimiter distinct from the active delimiter is accepted on
    // decode, even ones the encoder would never emit.
    assert_eq!(
        json_of("items[1]{a[x]}:\n  1\n"),
        json!({"items": [{"a": [1]}]})
    );
}

#[test]
fn rejects_malformed_structured_rows() {
    let cases = [
        (
            "items[1]{a,b}:\n  1,2,3\n",
            "line 2: array row length mismatch",
        ),
        ("items[2]{a,b}:\n  1,2\n", "line 2: array length mismatch"),
        (
            "items[1]{a,b}:\n  1,2\n  3,4\n",
            "line 3: array length mismatch",
        ),
        (
            "items[2]{a,b}:\n  1,2\n\n  3,4\n",
            "line 4: blank line inside array",
        ),
        (
            "items[1]{a,kids{x}}:\n  1,2\n    r1\n",
            "line 3: array length mismatch",
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(error(input), expected, "{input:?}");
    }
}

#[test]
fn a_nested_object_column_cell_disambiguates_from_a_child_table() {
    // Without deeper rows, a `field{...}` cell is a nested-object column.
    assert_eq!(
        json_of("items[1]{a,kids{x}}:\n  1,zz\n"),
        json!({"items": [{"a": 1, "kids": {"x": "zz"}}]})
    );
}

#[test]
fn truncation_reports_cover_invalid_indentation_and_child_tables() {
    let report = detect_truncation_with_options("v: 1\n   bad: 2\n", ParseOptions::default());
    assert!(!report.complete);
    assert_eq!(report.line, Some(2));
    assert_eq!(
        report.message.as_deref(),
        Some("line 2: invalid indentation")
    );

    let report = detect_truncation_with_options(
        "items[2]{a,kids{x}}:\n  1,1\n    r1\n",
        ParseOptions::default(),
    );
    assert!(!report.complete);
    assert_eq!(report.declared, Some(2));
    assert_eq!(report.actual, Some(1));
}

#[test]
fn tabular_arrays_decode_rows_lazily_through_the_array_accessors() {
    let value = parse("items[2]{a,meta{x,y}}:\n  1,ha,7\n  2,hb,9\n");
    let document = value.as_object().expect("root object");
    let array = document
        .get("items")
        .expect("items")
        .as_array()
        .expect("tabular array");

    assert_eq!(
        array.get(1).expect("row 1").to_json_value(),
        json!({"a": 2, "meta": {"x": "hb", "y": 9}})
    );
    assert_eq!(
        array.to_json_value(),
        json!([
            {"a": 1, "meta": {"x": "ha", "y": 7}},
            {"a": 2, "meta": {"x": "hb", "y": 9}}
        ])
    );
    assert_eq!(
        array.slice(Some(1), None).to_canonical_toon(),
        "[1]:\n  - a: 2\n    meta:\n      x: hb\n      y: 9\n"
    );
}

#[test]
fn encoding_rejects_a_delimiter_outside_the_declared_set() {
    let value = parse("items[2]{a,b}:\n  1,2\n  3,4\n");
    let options = EncodeOptions {
        delimiter: ';',
        ..EncodeOptions::default()
    };
    let error = value
        .try_to_toon_with_options(options)
        .expect_err("semicolon is not a valid document delimiter");
    assert_eq!(error.to_string(), "invalid array header");
}

#[test]
fn a_non_count_cell_in_a_child_table_column_decodes_as_a_nested_object() {
    // Decoding stays lenient per row: row 1 consumes an indented child table,
    // while row 2's non-count cell falls back to the nested-object reading.
    assert_eq!(
        json_of("items[2]{a,kids{x}}:\n  1,1\n    r1\n  2,zz\n"),
        json!({"items": [
            {"a": 1, "kids": [{"x": "r1"}]},
            {"a": 2, "kids": {"x": "zz"}}
        ]})
    );
}

#[test]
fn an_empty_child_array_in_a_child_table_column_round_trips() {
    assert_eq!(
        json_of("items[2]{a,kids{x}}:\n  1,1\n    r1\n  2,0\n"),
        json!({"items": [
            {"a": 1, "kids": [{"x": "r1"}]},
            {"a": 2, "kids": []}
        ]})
    );
}

#[test]
fn fallible_canonical_encoders_and_error_accessors_round_trip() {
    // Convenience wrappers around the canonical encoders.
    let value = parse("a[2]: 1,2\n");
    assert_eq!(value.try_to_canonical_toon().expect("value"), "a[2]: 1,2\n");
    let document = value.as_object().expect("object");
    let array = document.get("a").expect("a").as_array().expect("array");
    assert_eq!(array.try_to_canonical_toon().expect("array"), "[2]: 1,2\n");

    // detect_truncation with default options mirrors the _with_options form.
    let report = reddb_io_toon::detect_truncation("v: 1\n   bad: 2\n");
    assert!(!report.complete);

    // EncodeError::message exposes the static message.
    let options = EncodeOptions {
        delimiter: ';',
        ..EncodeOptions::default()
    };
    let encode_error = value
        .try_to_toon_with_options(options)
        .expect_err("bad delimiter");
    assert_eq!(encode_error.message(), "invalid array header");

    // Trailing content after a complete root array is a parse error.
    assert_eq!(
        error("[2]: 1,2\nextra: 3\n"),
        "line 2: expected end of document"
    );
}

// ---------------------------------------------------------------------------
// Cyclic discriminated arrays (tabular wire extension)
// ---------------------------------------------------------------------------

/// The canonical cyclic tabular wire from the wire-efficiency corpus: a
/// three-event `login/purchase/logout` cycle repeated four times with a shared
/// `tenant/seq/actor` common prefix.
const CYCLIC_WIRE: &str = "events:\n  order: cycle(login,purchase,logout)*4\n  discriminator: type\n  rows: 12\n  common[12|]{tenant|seq|actor}:\n    acme|1|u1\n    acme|2|u1\n    acme|3|u1\n    acme|4|u2\n    acme|5|u2\n    acme|6|u2\n    acme|7|u3\n    acme|8|u3\n    acme|9|u3\n    acme|10|u4\n    acme|11|u4\n    acme|12|u4\n  login[4|]{ok}:\n    true\n    true\n    false\n    true\n  purchase[4|]{amount|currency}:\n    12.5|USD\n    4|EUR\n    99.95|USD\n    1.25|BRL\n  logout[4|]{durationMs}:\n    1200\n    900\n    1800\n    600\n";

fn cyclic_encode() -> EncodeOptions {
    EncodeOptions {
        cyclic_discriminated_arrays: true,
        ..EncodeOptions::default()
    }
}

/// Encode with the cyclic extension enabled.
fn cyclic_toon(value: &Value) -> String {
    value
        .try_to_toon_with_options(cyclic_encode())
        .expect("cyclic encode succeeds")
}

/// Build a `{ "events": <rows> }` document from a JSON row list.
fn cyclic_events(rows: serde_json::Value) -> Value {
    Value::from_json_value(json!({ "events": rows }))
}

/// Repeat a per-label pattern `repeats` times into a flat row array.
fn cycle_rows(pattern: &[serde_json::Value], repeats: usize) -> serde_json::Value {
    let mut out = Vec::new();
    for _ in 0..repeats {
        for value in pattern {
            out.push(value.clone());
        }
    }
    serde_json::Value::Array(out)
}

/// Assert that encoding with the cyclic extension enabled falls back to the
/// canonical form (i.e. the tabular wire is *not* emitted) for ineligible input.
fn assert_cyclic_fallback(value: &Value) {
    let with = cyclic_toon(value);
    let canonical = value.try_to_canonical_toon().expect("canonical encode");
    assert_eq!(with, canonical, "expected fallback to canonical form");
    assert!(
        !with.contains("order: cycle("),
        "cyclic wire should not be emitted: {with}"
    );
}

#[test]
fn cyclic_wire_round_trips_through_decode_and_encode() {
    // Decode the tabular wire back into the expanded event array.
    let value = parse(CYCLIC_WIRE);
    let json = value.to_json_value();
    assert_eq!(
        json["events"][0],
        json!({ "type": "login", "tenant": "acme", "seq": 1, "actor": "u1", "ok": true })
    );
    assert_eq!(
        json["events"][1],
        json!({
            "type": "purchase", "tenant": "acme", "seq": 2, "actor": "u1",
            "amount": 12.5, "currency": "USD"
        })
    );
    assert_eq!(json["events"].as_array().expect("array").len(), 12);

    // Re-encoding with the extension reproduces the byte-identical wire.
    assert_eq!(cyclic_toon(&value), CYCLIC_WIRE);
    // And the reproduced wire decodes back to the same value.
    assert_eq!(parse(CYCLIC_WIRE).to_json_value(), json);
}

#[test]
fn cyclic_wire_round_trips_nested_array_and_object_payloads() {
    // login rows carry a nested array, purchase rows a nested object; both must
    // survive flatten-on-encode and inflate-on-decode.
    let value = cyclic_events(cycle_rows(
        &[
            json!({ "type": "login", "ok": true, "roles": ["admin", "ops"] }),
            json!({ "type": "purchase", "amount": 12.5, "meta": { "gift": true } }),
            json!({ "type": "logout", "durationMs": 1200 }),
        ],
        4,
    ));
    let json = value.to_json_value();

    let wire = cyclic_toon(&value);
    assert!(
        wire.starts_with("events:\n  order: cycle(login,purchase,logout)*4\n"),
        "nested payloads still emit the cyclic wire: {wire}"
    );
    // The flattened dotted columns appear in the tabular headers.
    assert!(wire.contains("{ok|roles.length|roles.0|roles.1}"));
    assert!(wire.contains("{amount|meta.gift}"));

    // Decoding inflates the dotted columns back into nested structures.
    assert_eq!(parse(&wire).to_json_value(), json);
}

#[test]
fn cyclic_decode_rejects_missing_or_malformed_section_headers() {
    // Section-like (has discriminator/rows) but the order field is absent.
    assert_eq!(
        error("events:\n  discriminator: type\n  rows: 4\n  a[2|]{x}:\n    1\n    2\n  b[2|]{y}:\n    3\n    4\n"),
        "line 1: invalid cyclic array wire"
    );
    // order present, discriminator absent.
    assert_eq!(
        error("events:\n  order: cycle(a,b)*2\n  rows: 4\n  a[2|]{x}:\n    1\n    2\n  b[2|]{y}:\n    3\n    4\n"),
        "line 1: invalid cyclic array wire"
    );
    // order + discriminator present, rows absent.
    assert_eq!(
        error("events:\n  order: cycle(a,b)*2\n  discriminator: type\n  a[2|]{x}:\n    1\n    2\n  b[2|]{y}:\n    3\n    4\n"),
        "line 1: invalid cyclic array wire"
    );
    // rows present but not a non-negative integer.
    assert_eq!(
        error("events:\n  order: cycle(a,b)*2\n  discriminator: type\n  rows: four\n  a[2|]{x}:\n    1\n    2\n  b[2|]{y}:\n    3\n    4\n"),
        "line 1: invalid cyclic array wire"
    );
}

#[test]
fn cyclic_decode_rejects_malformed_common_and_group_columns() {
    // common is a scalar rather than a table.
    assert_eq!(
        error("events:\n  order: cycle(a,b)*2\n  discriminator: type\n  rows: 4\n  common: 5\n  a[2|]{x}:\n    1\n    2\n  b[2|]{y}:\n    3\n    4\n"),
        "line 1: invalid cyclic array wire"
    );
    // common is an array of primitives rather than objects.
    assert_eq!(
        error("events:\n  order: cycle(a,b)*2\n  discriminator: type\n  rows: 4\n  common[4]: 1,2,3,4\n  a[2|]{x}:\n    1\n    2\n  b[2|]{y}:\n    3\n    4\n"),
        "line 1: invalid cyclic array wire"
    );
    // common object count disagrees with the declared row count.
    assert_eq!(
        error("events:\n  order: cycle(a,b)*2\n  discriminator: type\n  rows: 4\n  common[2|]{c}:\n    1\n    2\n  a[2|]{x}:\n    1\n    2\n  b[2|]{y}:\n    3\n    4\n"),
        "line 1: cyclic array length mismatch"
    );
    // a group column is a scalar, not an array.
    assert_eq!(
        error("events:\n  order: cycle(a,b)*2\n  discriminator: type\n  rows: 4\n  a: 5\n  b[2|]{y}:\n    3\n    4\n"),
        "line 1: invalid cyclic array wire"
    );
    // a group is an array of primitives rather than objects.
    assert_eq!(
        error("events:\n  order: cycle(a,b)*2\n  discriminator: type\n  rows: 4\n  a[2]: 1,2\n  b[2|]{y}:\n    3\n    4\n"),
        "line 1: invalid cyclic array wire"
    );
    // no group columns at all.
    assert_eq!(
        error("events:\n  order: cycle(a,b)*2\n  discriminator: type\n  rows: 4\n"),
        "line 1: invalid cyclic array wire"
    );
}

#[test]
fn cyclic_decode_rejects_group_length_disagreements() {
    // The order names a label that has no group table.
    assert_eq!(
        error("events:\n  order: cycle(a,c)*2\n  discriminator: type\n  rows: 4\n  a[2|]{x}:\n    1\n    2\n  b[2|]{y}:\n    3\n    4\n"),
        "line 1: cyclic array group length mismatch"
    );
    // A group runs out of rows before the order is satisfied.
    assert_eq!(
        error("events:\n  order: cycle(a,b)*2\n  discriminator: type\n  rows: 4\n  a[1|]{x}:\n    1\n  b[2|]{y}:\n    3\n    4\n"),
        "line 1: cyclic array group length mismatch"
    );
    // A group has more rows than the order consumes.
    assert_eq!(
        error("events:\n  order: cycle(a,b)*2\n  discriminator: type\n  rows: 4\n  a[2|]{x}:\n    1\n    2\n  b[3|]{y}:\n    3\n    4\n    5\n"),
        "line 1: cyclic array group length mismatch"
    );
}

#[test]
fn cyclic_decode_rejects_malformed_order_expressions() {
    let base = |order: &str| {
        format!("events:\n  order: {order}\n  discriminator: type\n  rows: 4\n  a[2|]{{x}}:\n    1\n    2\n  b[2|]{{y}}:\n    3\n    4\n")
    };
    // Missing the cycle(...) wrapper.
    assert_eq!(error(&base("foo")), "line 1: invalid cyclic array wire");
    // Missing the )* separator.
    assert_eq!(
        error(&base("cycle(a,b)")),
        "line 1: invalid cyclic array wire"
    );
    // Empty cycle body.
    assert_eq!(
        error(&base("cycle()*2")),
        "line 1: invalid cyclic array wire"
    );
    // The rejected +tail(...) suffix.
    assert_eq!(
        error(&base("cycle(a,b)*2+tail(c)")),
        "line 1: invalid cyclic array wire"
    );
    // An empty label inside the cycle.
    assert_eq!(
        error("events:\n  order: cycle(a,,b)*1\n  discriminator: type\n  rows: 3\n  a[1|]{x}:\n    1\n  b[1|]{y}:\n    2\n"),
        "line 1: invalid cyclic array wire"
    );
    // The decoded cycle length disagrees with the declared row count.
    assert_eq!(
        error(&base("cycle(a,b)*3")),
        "line 1: cyclic array length mismatch"
    );
}

#[test]
fn cyclic_decode_rejects_malformed_repeat_counts() {
    let base = |order: &str| {
        format!("events:\n  order: {order}\n  discriminator: type\n  rows: 4\n  a[2|]{{x}}:\n    1\n    2\n  b[2|]{{y}}:\n    3\n    4\n")
    };
    // Non-numeric repeat count.
    assert_eq!(
        error(&base("cycle(a,b)*x")),
        "line 1: invalid cyclic array wire"
    );
    // Empty repeat count.
    assert_eq!(
        error(&base("cycle(a,b)*")),
        "line 1: invalid cyclic array wire"
    );
    // Leading-zero repeat count.
    assert_eq!(
        error(&base("cycle(a,b)*02")),
        "line 1: invalid cyclic array wire"
    );
}

#[test]
fn cyclic_decode_rejects_malformed_percent_escapes_in_labels() {
    let base = |order: &str| {
        format!("events:\n  order: {order}\n  discriminator: type\n  rows: 2\n  a[1|]{{x}}:\n    1\n  b[1|]{{y}}:\n    2\n")
    };
    // A truncated percent escape (not enough hex digits).
    assert_eq!(
        error(&base("cycle(a%,b)*1")),
        "line 1: invalid cyclic array wire"
    );
    // Non-hexadecimal escape digits.
    assert_eq!(
        error(&base("cycle(a%zz,b)*1")),
        "line 1: invalid cyclic array wire"
    );
    // A well-formed escape that decodes to invalid UTF-8.
    assert_eq!(
        error(&base("cycle(%ff,b)*1")),
        "line 1: invalid cyclic array wire"
    );
}

#[test]
fn cyclic_decode_rejects_discriminator_and_field_collisions() {
    // A common column collides with the discriminator name.
    assert_eq!(
        error("events:\n  order: cycle(a,b)*2\n  discriminator: type\n  rows: 4\n  common[4|]{type}:\n    x\n    x\n    x\n    x\n  a[2|]{x}:\n    1\n    2\n  b[2|]{y}:\n    3\n    4\n"),
        "line 1: invalid cyclic array wire"
    );
    // A payload column collides with the discriminator name.
    assert_eq!(
        error("events:\n  order: cycle(a,b)*2\n  discriminator: type\n  rows: 4\n  a[2|]{type}:\n    x\n    x\n  b[2|]{y}:\n    3\n    4\n"),
        "line 1: invalid cyclic array wire"
    );
    // A payload column collides with a common column.
    assert_eq!(
        error("events:\n  order: cycle(a,b)*2\n  discriminator: type\n  rows: 4\n  common[4|]{c}:\n    1\n    2\n    3\n    4\n  a[2|]{c}:\n    9\n    9\n  b[2|]{y}:\n    3\n    4\n"),
        "line 1: invalid cyclic array wire"
    );
}

#[test]
fn cyclic_decode_rejects_flat_payload_that_inflates_to_a_bad_shape() {
    // A payload that flattens to a bare array length with no elements.
    assert_eq!(
        error("events:\n  order: cycle(a,b)*2\n  discriminator: type\n  rows: 4\n  a[2|]{length}:\n    0\n    0\n  b[2|]{y}:\n    3\n    4\n"),
        "line 1: invalid cyclic array wire"
    );
    // A nested array header ("items.length") with no corresponding elements.
    assert_eq!(
        error("events:\n  order: cycle(a,b)*2\n  discriminator: type\n  rows: 4\n  a[2|]{items.length}:\n    2\n    2\n  b[2|]{y}:\n    3\n    4\n"),
        "line 1: invalid cyclic array wire"
    );
}

#[test]
fn cyclic_decode_leaves_non_section_documents_untouched() {
    // A plain object whose fields are not section-like passes through unchanged.
    let value = parse("name: Ada\nage: 3\n");
    assert_eq!(value.to_json_value(), json!({ "name": "Ada", "age": 3 }));
    // An empty document is returned as-is.
    assert_eq!(parse("\n").to_json_value(), json!({}));
}

#[test]
fn cyclic_encode_falls_back_for_structurally_ineligible_input() {
    // A field value that is not an array.
    assert_cyclic_fallback(&Value::from_json_value(json!({ "events": 5 })));
    // A top-level key that is not a bare header token.
    assert_cyclic_fallback(&Value::from_json_value(
        json!({ "bad-key": [{ "type": "a" }] }),
    ));
    // Array rows that are not objects.
    assert_cyclic_fallback(&cyclic_events(json!([1, 2, 3])));
    // Objects with no recognised discriminator key.
    assert_cyclic_fallback(&cyclic_events(json!([{ "a": 1 }, { "a": 2 }, { "a": 3 }])));
}

#[test]
fn cyclic_encode_falls_back_when_no_compressible_cycle_exists() {
    // A discriminator is present but the labels do not form a repeated cycle
    // with enough repeats to beat the canonical output.
    assert_cyclic_fallback(&cyclic_events(json!([
        { "type": "a", "v": 1 },
        { "type": "b", "v": 2 },
        { "type": "a", "v": 3 },
    ])));
}

#[test]
fn cyclic_encode_falls_back_for_non_uniform_or_empty_groups() {
    // A common column whose key is not a bare header token.
    assert_cyclic_fallback(&cyclic_events(cycle_rows(
        &[
            json!({ "type": "login", "bad-key": 1, "ok": true }),
            json!({ "type": "logout", "bad-key": 1 }),
        ],
        5,
    )));
    // A payload column whose key is not a bare header token.
    assert_cyclic_fallback(&cyclic_events(cycle_rows(
        &[
            json!({ "type": "login", "bad-key": 1 }),
            json!({ "type": "logout", "ok": true }),
        ],
        5,
    )));
    // Rows within a group disagree on their payload fields.
    let mut rows = cycle_rows(
        &[
            json!({ "type": "login", "ok": true }),
            json!({ "type": "logout", "durationMs": 1 }),
        ],
        5,
    );
    rows.as_array_mut().expect("array")[0] = json!({ "type": "login", "ok": true, "extra": 9 });
    assert_cyclic_fallback(&cyclic_events(rows));
    // A label whose rows carry no payload at all.
    assert_cyclic_fallback(&cyclic_events(cycle_rows(
        &[json!({ "type": "login" }), json!({ "type": "logout" })],
        5,
    )));
}
