//! Library surface: the decoder options, the value accessors, and the corners of
//! the encoder and the error paths that the spec corpus does not reach on its own.

use reddb_io_toon::{Array, Document, ParseOptions, Value};
use serde_json::json;

fn parse(input: &str) -> Value {
    Value::parse_toon(input).unwrap_or_else(|error| panic!("parse {input:?}: {error}"))
}

fn error(input: &str) -> String {
    Value::parse_toon(input)
        .expect_err(&format!("{input:?} is rejected"))
        .to_string()
}

fn json_of(input: &str) -> serde_json::Value {
    parse(input).to_json_value()
}

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

#[test]
fn defaults_to_two_space_indent_strict_mode_and_literal_dotted_keys() {
    let options = ParseOptions::default();

    assert_eq!(options.indent, 2);
    assert!(options.strict);
    assert!(!options.expand_paths);
    assert_eq!(json_of("user.name: Ada"), json!({"user.name": "Ada"}));
}

#[test]
fn honours_a_custom_indent_width() {
    let options = ParseOptions {
        indent: 4,
        ..ParseOptions::default()
    };

    let value = Value::parse_with_options("a:\n    b: 1\n", options).expect("four-space indent");
    assert_eq!(value.to_json_value(), json!({"a": {"b": 1}}));

    // The same document is misindented when a level is two spaces wide.
    assert_eq!(error("a:\n    b: 1\n"), "line 2: invalid indentation");
}

#[test]
fn an_indent_of_zero_is_clamped_rather_than_dividing_by_zero() {
    let options = ParseOptions {
        indent: 0,
        ..ParseOptions::default()
    };

    let value = Value::parse_with_options("a: 1\n", options).expect("clamped indent");
    assert_eq!(value.to_json_value(), json!({"a": 1}));
}

#[test]
fn non_strict_mode_tolerates_off_grid_indentation_and_resolves_duplicates_last_write_wins() {
    let options = ParseOptions {
        strict: false,
        ..ParseOptions::default()
    };

    let indented =
        Value::parse_with_options("a:\n   b: 1\n", options).expect("three-space indent is floored");
    assert_eq!(indented.to_json_value(), json!({"a": {"b": 1}}));

    let duplicate =
        Value::parse_with_options("name: Ada\nname: Bob\n", options).expect("last write wins");
    assert_eq!(duplicate.to_json_value(), json!({"name": "Bob"}));
}

#[test]
fn non_strict_mode_keeps_a_malformed_header_as_a_literal_key() {
    let options = ParseOptions {
        strict: false,
        ..ParseOptions::default()
    };

    let value = Value::parse_with_options("foo[2]extra: a,b\n", options).expect("literal key");
    assert_eq!(value.to_json_value(), json!({"foo[2]extra": "a,b"}));

    // A root line that opens with a bracket but is not a header falls through
    // to the same key-value reading.
    let root = Value::parse_with_options("[bad]: 1\n", options).expect("literal key at root");
    assert_eq!(root.to_json_value(), json!({"[bad]": 1}));
}

#[test]
fn path_expansion_splits_dotted_keys_and_deep_merges_them() {
    let options = ParseOptions {
        expand_paths: true,
        ..ParseOptions::default()
    };

    let value = Value::parse_with_options("a.b.c: 1\na.b.d: 2\na.e: 3\n", options)
        .expect("dotted keys expand");

    assert_eq!(
        value.to_json_value(),
        json!({"a": {"b": {"c": 1, "d": 2}, "e": 3}})
    );
}

#[test]
fn path_expansion_conflicts_error_in_strict_mode_and_resolve_last_write_wins_without_it() {
    let strict = ParseOptions {
        expand_paths: true,
        ..ParseOptions::default()
    };
    let lenient = ParseOptions {
        strict: false,
        ..strict
    };

    let conflict = Value::parse_with_options("a: 1\na.b: 2\n", strict)
        .expect_err("a primitive cannot become an object");
    assert_eq!(conflict.message(), "path expansion conflict");

    let resolved = Value::parse_with_options("a: 1\na.b: 2\n", lenient).expect("last write wins");
    assert_eq!(resolved.to_json_value(), json!({"a": {"b": 2}}));
}

#[test]
fn path_expansion_leaves_quoted_and_non_identifier_keys_alone() {
    let options = ParseOptions {
        expand_paths: true,
        ..ParseOptions::default()
    };

    // A quoted key stays literal even when it contains the separator, and a
    // segment that is not an IdentifierSegment blocks the whole split.
    let value = Value::parse_with_options("\"c.d\": 1\n9a.b: 2\n", options).expect("literal keys");

    assert_eq!(value.to_json_value(), json!({"c.d": 1, "9a.b": 2}));
}

// ---------------------------------------------------------------------------
// Decoding
// ---------------------------------------------------------------------------

#[test]
fn decodes_each_root_form() {
    assert_eq!(json_of(""), json!({}));
    assert_eq!(json_of("\n\n"), json!({}));
    assert_eq!(json_of("hello"), json!("hello"));
    assert_eq!(json_of("42"), json!(42));
    assert_eq!(json_of("true"), json!(true));
    assert_eq!(json_of("null"), json!(null));
    assert_eq!(json_of("[]"), json!([]));
    assert_eq!(json_of("[0]:"), json!([]));
    assert_eq!(json_of("[2]: a,b"), json!(["a", "b"]));
    assert_eq!(json_of("[1]{id}:\n  7"), json!([{"id": 7}]));
    assert_eq!(json_of("a: 1"), json!({"a": 1}));
}

#[test]
fn decodes_tab_and_pipe_delimited_arrays() {
    assert_eq!(json_of("t[2\t]: a\tb"), json!({"t": ["a", "b"]}));
    assert_eq!(json_of("t[2|]: a|b"), json!({"t": ["a", "b"]}));
    // A nested header re-declares its own delimiter; comma never inherits.
    assert_eq!(
        json_of("t[1|]:\n  - inner[2]: a,b"),
        json!({"t": [{"inner": ["a", "b"]}]})
    );
}

#[test]
fn a_carriage_return_line_ending_is_stripped() {
    assert_eq!(json_of("a: 1\r\nb: 2\r\n"), json!({"a": 1, "b": 2}));
}

#[test]
fn rejects_the_strict_mode_error_checklist() {
    let cases = [
        // Counts and widths (§14.1).
        ("tags[2]: a,b,c", "array length mismatch"),
        ("tags[3]: a,b", "array length mismatch"),
        ("items[2]:\n  - a", "array length mismatch"),
        ("items[1]:\n  - a\n  - b", "array length mismatch"),
        ("items[1]{id}:\n  1\n  2", "array length mismatch"),
        ("items[2]{id}:\n  1", "array length mismatch"),
        (
            "items[2]{id,name}:\n  1,Ada\n  2",
            "array row length mismatch",
        ),
        // Headers (§6, §14.2).
        ("items[03]: a,b,c", "invalid array header"),
        ("items[-1]: a", "invalid array header"),
        ("items[bar]: a", "invalid array header"),
        ("items[1][bar]: a", "invalid array header"),
        ("items[2]extra: a,b", "invalid array header"),
        ("items[2] : a,b", "invalid array header"),
        ("items[2]{a,b}\n  1,2", "expected `key: value`"),
        ("items[2]{a,b}: inline", "expected tabular rows"),
        ("items[2\t: a", "invalid array header"),
        // Structure (§14.2).
        ("hello\nworld", "expected `key: value`"),
        ("a:\n  user", "expected `key: value`"),
        (": 1", "expected non-empty field name"),
        ("[2]: 1,2\nstray: 1", "expected end of document"),
        ("items[1]:\n  a", "expected array item"),
        ("  a: 1", "invalid indentation"),
        ("a: 1\n\tb: 2", "invalid indentation"),
        ("a:\n    b: 1", "invalid indentation"),
        ("items[1]:\n    - a", "invalid indentation"),
        ("items[1]{id}:\n      1", "invalid indentation"),
        // Blank lines inside arrays (§12).
        ("items[2]:\n  - a\n\n  - b", "blank line inside array"),
        ("items[2]{id}:\n  1\n\n  2", "blank line inside array"),
        // Duplicates (§14.4).
        ("name: Ada\nname: Bob", "duplicate key"),
        ("outer:\n  a: 1\n  a: 2", "duplicate key"),
        ("items[1]:\n  - id: 1\n    id: 2", "duplicate key"),
        // Quoted tokens (§7.1).
        ("v: \"a\\x\"", "invalid quoted string"),
        ("v: \"a\\u00b\"", "invalid quoted string"),
        ("v: \"a\\uD800b\"", "invalid quoted string"),
        ("v: \"a\\", "invalid quoted string"),
        ("\"unterminated", "invalid quoted string"),
        ("v: \"a\" trailing", "invalid quoted string"),
        ("v: mid\"quote", "invalid quoted string"),
        ("v: \"a\nb\"", "invalid quoted string"),
        ("\"a\"b: 1", "invalid quoted string"),
    ];

    for (input, message) in cases {
        let reported = error(input);
        assert!(
            reported.contains(message),
            "{input:?} reports `{message}`, got `{reported}`"
        );
    }
}

#[test]
fn reports_the_line_the_error_occurred_on() {
    let failure = Value::parse_toon("a: 1\nb: 2\nc[2]: x\n").expect_err("length mismatch");

    assert_eq!(failure.line(), 3);
    assert_eq!(failure.message(), "array length mismatch");
    assert_eq!(failure.to_string(), "line 3: array length mismatch");
}

#[test]
fn blank_lines_are_ignored_outside_arrays() {
    assert_eq!(json_of("a: 1\n\nb: 2\n"), json!({"a": 1, "b": 2}));
    assert_eq!(
        json_of("a:\n  b: 1\n\n  c: 2\n"),
        json!({"a": {"b": 1, "c": 2}})
    );
    assert_eq!(
        json_of("items[1]:\n  - a\n\nb: 2\n"),
        json!({"items": ["a"], "b": 2})
    );
    // A whitespace-only line is blank even at an off-grid column.
    assert_eq!(json_of("a: 1\n   \nb: 2\n"), json!({"a": 1, "b": 2}));
}

#[test]
fn a_quoted_colon_keeps_a_tabular_row_from_reading_as_a_field() {
    assert_eq!(
        json_of("links[1]{id,url}:\n  1,\"http://a:b\"\n"),
        json!({"links": [{"id": 1, "url": "http://a:b"}]})
    );
}

// ---------------------------------------------------------------------------
// Numbers
// ---------------------------------------------------------------------------

#[test]
fn canonicalizes_numbers_on_the_way_in_and_back_out() {
    let cases = [
        ("1.5000", json!(1.5), "1.5"),
        ("-1E+03", json!(-1000), "-1000"),
        ("2.5e2", json!(250), "250"),
        ("-0", json!(0), "0"),
        ("-0.0", json!(0), "0"),
        ("0e1", json!(0), "0"),
        ("1e6", json!(1000000), "1000000"),
        ("1e-6", json!(1e-6), "0.000001"),
        (
            "9007199254740992",
            json!(9007199254740992i64),
            "9007199254740992",
        ),
        (
            "0.3333333333333333",
            json!(0.3333333333333333),
            "0.3333333333333333",
        ),
    ];

    for (token, expected, canonical) in cases {
        let input = format!("v: {token}");
        assert_eq!(json_of(&input), json!({"v": expected}), "decoding {token}");
        assert_eq!(
            parse(&input).to_canonical_toon(),
            format!("v: {canonical}\n"),
            "re-encoding {token}"
        );
    }
}

#[test]
fn leading_zero_tokens_decode_as_strings_and_are_quoted_on_the_way_out() {
    assert_eq!(
        json_of("nums[4]: 05,007,-05,0123"),
        json!({"nums": ["05", "007", "-05", "0123"]})
    );
    assert_eq!(
        parse("nums[1]: 05").to_canonical_toon(),
        "nums[1]: \"05\"\n"
    );
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

#[test]
fn quotes_only_the_strings_the_spec_requires() {
    let bare = [
        "hello",
        "Ada_99",
        "hello 👋 world",
        "café",
        "你好",
        "a,b is fine inside a row cell only when the delimiter differs",
    ];
    for value in bare {
        let encoded = Value::String(value.to_owned()).to_canonical_toon();
        // Only the comma case needs quoting as an object value, so check the
        // ones with no structural character at all.
        if !value.contains(',') {
            assert_eq!(encoded, value, "{value:?} needs no quotes");
        }
    }

    let quoted = [
        ("", "\"\""),
        (" padded ", "\" padded \""),
        ("true", "\"true\""),
        ("null", "\"null\""),
        ("42", "\"42\""),
        ("1e-6", "\"1e-6\""),
        ("05", "\"05\""),
        ("a:b", "\"a:b\""),
        ("a,b", "\"a,b\""),
        ("[test]", "\"[test]\""),
        ("{key}", "\"{key}\""),
        ("-", "\"-\""),
        ("- item", "\"- item\""),
        ("say \"hi\"", "\"say \\\"hi\\\"\""),
        ("C:\\path", "\"C:\\\\path\""),
        ("line1\nline2", "\"line1\\nline2\""),
        ("tab\there", "\"tab\\there\""),
        ("return\rhere", "\"return\\rhere\""),
        ("a\u{0004}b", "\"a\\u0004b\""),
    ];
    for (value, expected) in quoted {
        assert_eq!(
            Value::String(value.to_owned()).to_canonical_toon(),
            expected,
            "{value:?} is quoted"
        );
    }
}

#[test]
fn quotes_only_the_keys_the_spec_requires() {
    let cases = [
        (json!({"user.name": 1}), "user.name: 1\n"),
        (json!({"_private": 1}), "_private: 1\n"),
        (json!({"order:id": 1}), "\"order:id\": 1\n"),
        (json!({"a,b": 1}), "\"a,b\": 1\n"),
        (json!({"full name": 1}), "\"full name\": 1\n"),
        (json!({"-lead": 1}), "\"-lead\": 1\n"),
        (json!({"123": 1}), "\"123\": 1\n"),
        (json!({"": 1}), "\"\": 1\n"),
        (json!({"[index]": 1}), "\"[index]\": 1\n"),
    ];

    for (input, expected) in cases {
        assert_eq!(Value::from_json_value(input).to_canonical_toon(), expected);
    }
}

/// Every shape an array can take on the way out, and each one has to read back.
#[test]
fn encodes_and_reparses_every_array_shape() {
    let cases = [
        (json!({"a": []}), "a: []\n"),
        (json!({"a": [1, 2]}), "a[2]: 1,2\n"),
        (
            json!({"a": [{"id": 1}, {"id": 2}]}),
            "a[2]{id}:\n  1\n  2\n",
        ),
        // Key order differs between rows, so the header order wins.
        (
            json!({"a": [{"id": 1, "n": "x"}, {"n": "y", "id": 2}]}),
            "a[2]{id,n}:\n  1,x\n  2,y\n",
        ),
        // A differing key set falls back to the expanded list.
        (
            json!({"a": [{"id": 1}, {"other": 2}]}),
            "a[2]:\n  - id: 1\n  - other: 2\n",
        ),
        // A nested value in any row disqualifies the tabular form.
        (
            json!({"a": [{"id": 1}, {"id": {"n": 2}}]}),
            "a[2]:\n  - id: 1\n  - id:\n      n: 2\n",
        ),
        // An empty object anywhere disqualifies it too, and encodes bare.
        (json!({"a": [{}, {}]}), "a[2]:\n  -\n  -\n"),
        // Mixed element kinds.
        (
            json!({"a": [1, {"k": 1}, "t"]}),
            "a[3]:\n  - 1\n  - k: 1\n  - t\n",
        ),
        // Arrays of arrays, including the empty inner array's [0] form.
        (
            json!({"a": [[1, 2], []]}),
            "a[2]:\n  - [2]: 1,2\n  - [0]:\n",
        ),
        // A nested array of objects uses the expanded list under the hyphen.
        (
            json!({"a": [[{"id": 1}, {"id": 2}]]}),
            "a[1]:\n  - [2]:\n    - id: 1\n    - id: 2\n",
        ),
        // A tabular array as a list item's first field: header on the hyphen
        // line, rows two levels under it, siblings one level under it (§10).
        (
            json!({"a": [{"u": [{"id": 1}, {"id": 2}], "s": "ok"}]}),
            "a[1]:\n  - u[2]{id}:\n      1\n      2\n    s: ok\n",
        ),
        // An empty array on the hyphen line.
        (
            json!({"a": [{"d": [], "n": "x"}]}),
            "a[1]:\n  - d: []\n    n: x\n",
        ),
        // Root arrays, with and without a key.
        (json!([]), "[]\n"),
        (json!([1, 2]), "[2]: 1,2\n"),
        (json!([{"id": 1}]), "[1]{id}:\n  1\n"),
    ];

    for (input, expected) in cases {
        let value = Value::from_json_value(input.clone());
        let encoded = value.to_canonical_toon();

        assert_eq!(encoded, expected, "encoding {input}");
        assert_eq!(
            Value::parse_toon(&encoded)
                .unwrap_or_else(|error| panic!("re-read {encoded:?}: {error}"))
                .to_json_value(),
            input,
            "round trip for {input}"
        );
    }
}

#[test]
fn encodes_objects_and_scalars() {
    assert_eq!(Value::from_json_value(json!({})).to_canonical_toon(), "");
    assert_eq!(
        Value::from_json_value(json!({"user": {}})).to_canonical_toon(),
        "user:\n"
    );
    assert_eq!(
        Value::from_json_value(json!({"a": {"b": {"c": "deep"}}})).to_canonical_toon(),
        "a:\n  b:\n    c: deep\n"
    );
    assert_eq!(Value::Bool(true).to_canonical_toon(), "true");
    assert_eq!(Value::Bool(false).to_canonical_toon(), "false");
    assert_eq!(Value::Null.to_canonical_toon(), "null");
}

// ---------------------------------------------------------------------------
// Value, Array and Document accessors
// ---------------------------------------------------------------------------

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
