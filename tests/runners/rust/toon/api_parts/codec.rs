#[test]
fn toonl_reader_finishes_once_and_further_polls_stay_exhausted() {
    let mut reader = ToonlReader::new(lines(b"[]{a}:\n1\n[=1]\n"));
    assert_eq!(
        reader
            .next()
            .expect("row")
            .expect("valid row")
            .to_json_value(),
        json!({"a": 1})
    );
    assert!(reader.next().is_none(), "the stream ends after one row");
    // Polling again after exhaustion takes the reader's fast `finished` path
    // rather than re-reading past EOF.
    assert!(reader.next().is_none(), "a finished reader stays finished");
}

#[test]
fn toonl_reader_skips_blank_lines_and_reads_a_colon_led_cell_as_data() {
    assert_eq!(
        ToonlReader::new(lines(b"[]{a}:\n1\n\n2\n[=2]\n"))
            .collect::<Result<Vec<_>, _>>()
            .expect("blank lines between rows are ignored")
            .iter()
            .map(Value::to_json_value)
            .collect::<Vec<_>>(),
        vec![json!({"a": 1}), json!({"a": 2})]
    );

    // A cell that happens to start with `:` is not mistaken for a tag prefix,
    // because a leading colon can never open a valid tag.
    assert_eq!(
        ToonlReader::new(lines(b"[]{a}:\n:x\n[=1]\n"))
            .collect::<Result<Vec<_>, _>>()
            .expect("a colon-led cell decodes as data")
            .iter()
            .map(Value::to_json_value)
            .collect::<Vec<_>>(),
        vec![json!({"a": ":x"})]
    );
}

#[test]
fn toonl_reader_rejects_reserved_prefixes_bad_trailers_and_rows_out_of_place() {
    let reserved = ToonlReader::new(lines(b"[]{a}:\n- x\n"))
        .collect::<Result<Vec<_>, _>>()
        .expect_err("the `- ` prefix is reserved")
        .to_string();
    assert_eq!(reserved, "line 2: reserved line prefix");

    let trailer_mismatch = ToonlReader::new(lines(b"[]{id}:\n1\n[=5]\n"))
        .collect::<Result<Vec<_>, _>>()
        .expect_err("the trailer count does not match")
        .to_string();
    assert_eq!(trailer_mismatch, "line 3: trailer count mismatch");

    let row_before_header = ToonlReader::new(lines(b"1,Ada\n"))
        .collect::<Result<Vec<_>, _>>()
        .expect_err("a row with no header is rejected")
        .to_string();
    assert_eq!(row_before_header, "line 1: row before header");

    let continuation_first = ToonlReader::new(lines(b"[~]{a}:\n1\n"))
        .collect::<Result<Vec<_>, _>>()
        .expect_err("a continuation header cannot open a stream")
        .to_string();
    assert_eq!(
        continuation_first,
        "line 1: continuation header before header"
    );

    // A tag-shaped prefix that is not valid TOONL tag syntax (here a `.`,
    // which is neither alphanumeric nor `_`/`-`) is rejected explicitly
    // rather than silently read as an anonymous row.
    let invalid_tag = ToonlReader::new(lines(b"[]<req>{a}:\na.b:1\n"))
        .collect::<Result<Vec<_>, _>>()
        .expect_err("a malformed tag prefix is rejected")
        .to_string();
    assert_eq!(invalid_tag, "line 2: invalid tag");

    let tagged_arity = ToonlReader::new(lines(b"[]<req>{a,b}:\nreq:1\n"))
        .collect::<Result<Vec<_>, _>>()
        .expect_err("a tagged row with the wrong arity is rejected")
        .to_string();
    assert_eq!(tagged_arity, "line 2: row arity mismatch");
}

#[test]
fn toonl_stream_parse_rejects_arity_mismatches_and_unknown_tags_in_tagged_rows() {
    let arity_mismatch = ToonlStream::parse("[]<req>{a,b}:\nreq:1\n")
        .expect_err("a tagged row with too few cells is rejected")
        .to_string();
    assert_eq!(arity_mismatch, "line 2: row arity mismatch");

    let unknown_tag = ToonlStream::parse("metric:1\n")
        .expect_err("a tag prefix with no declared lane and no anonymous header is rejected")
        .to_string();
    assert_eq!(unknown_tag, "line 1: unknown tag");
}

#[test]
fn toonl_stream_row_values_flattens_every_segment_in_declaration_order() {
    let stream =
        ToonlStream::parse("[]{id,name}:\n1,Ada\n[=1]\n[]{id,name,role}:\n2,Linus,dev\n[=1]\n")
            .expect("valid TOONL stream");

    assert_eq!(
        stream
            .row_values()
            .expect("row values decode")
            .iter()
            .map(Value::to_json_value)
            .collect::<Vec<_>>(),
        vec![
            json!({"id": 1, "name": "Ada"}),
            json!({"id": 2, "name": "Linus", "role": "dev"}),
        ]
    );
}

#[test]
fn toonl_writer_declares_tagged_lanes_explicitly_and_rejects_overflow() {
    let mut output = Vec::new();
    {
        let mut writer = ToonlWriter::new(&mut output);
        writer
            .declare_lane("req", &["a", "b"])
            .expect("declare a lane before any row arrives");
        // Re-declaring the same fields is a no-op: no duplicate header line.
        writer
            .declare_lane("req", &["a", "b"])
            .expect("idempotent re-declaration");
        // Declaring the same tag with a different field order rewrites the
        // header, since it is a different canonical shape.
        writer
            .declare_lane("req", &["b", "a"])
            .expect("re-declaration with new fields rewrites the header");
        writer.finish().expect("finish writer");
    }
    assert_eq!(
        String::from_utf8(output).unwrap(),
        "[]<req>{a,b}:\n[]<req>{b,a}:\n"
    );

    let mut writer = ToonlWriter::new(Vec::new());
    for tag in ["a", "b", "c", "d", "e", "f", "g", "h"] {
        writer
            .declare_lane(tag, &["v"])
            .expect("declare up to the lane limit");
    }
    let error = writer
        .declare_lane("i", &["v"])
        .expect_err("the 9th declared lane is rejected");
    assert!(error.to_string().contains("too many tagged lanes"));
}

#[test]
fn toonl_encoders_support_byte_cadence_and_non_default_delimiters() {
    let mut encoder = ToonlEncoder::new(',', &["id", "name"]).expect("encoder");
    encoder
        .set_continuation_every_bytes(Some(6))
        .expect("byte cadence");
    for row in [["1", "Ada"], ["2", "Linus"], ["3", "Grace"]] {
        encoder.push_raw_row(&row).expect("row");
    }
    assert_eq!(
        encoder.finish(),
        "[]{id,name}:\n1,Ada\n[~]{id,name}:\n2,Linus\n[~]{id,name}:\n3,Grace\n[=3]\n"
    );

    let mut output = Vec::new();
    {
        let mut writer = ToonlWriter::with_delimiter(&mut output, '|');
        writer
            .set_continuation_every_rows(Some(1))
            .expect("row cadence");
        writer
            .write_record(&Value::from_json_str(r#"{"id":1,"name":"Ada"}"#).expect("row"))
            .expect("write row");
        writer
            .write_record(&Value::from_json_str(r#"{"id":2,"name":"Bo"}"#).expect("row"))
            .expect("write row");
        writer.finish().expect("finish writer");
    }
    assert_eq!(
        String::from_utf8(output).unwrap(),
        "[|]{id|name}:\n1|Ada\n[~|]{id|name}:\n2|Bo\n[=2]\n"
    );

    // The writer's own byte cadence, not just the standalone encoder's.
    let mut byte_cadence_output = Vec::new();
    {
        let mut writer = ToonlWriter::new(&mut byte_cadence_output);
        writer
            .set_continuation_every_bytes(Some(6))
            .expect("byte cadence");
        for row in [
            r#"{"id":1,"name":"Ada"}"#,
            r#"{"id":2,"name":"Linus"}"#,
            r#"{"id":3,"name":"Grace"}"#,
        ] {
            writer
                .write_record(&Value::from_json_str(row).expect("row"))
                .expect("write row");
        }
        writer.finish().expect("finish writer");
    }
    assert_eq!(
        String::from_utf8(byte_cadence_output).unwrap(),
        "[]{id,name}:\n1,Ada\n[~]{id,name}:\n2,Linus\n[~]{id,name}:\n3,Grace\n[=3]\n"
    );
}

#[test]
fn toonl_encoder_rejects_malformed_rows_and_configuration() {
    let mut arity = ToonlEncoder::new(',', &["id", "name"]).expect("encoder");
    assert_eq!(
        arity.push_raw_row(&["1"]).unwrap_err().to_string(),
        "row arity mismatch"
    );

    let mut non_object = ToonlEncoder::new(',', &["id"]).expect("encoder");
    assert_eq!(
        non_object
            .push_value_row(&Value::Number("1".to_owned()))
            .unwrap_err()
            .to_string(),
        "TOONL output requires object rows"
    );

    let mut missing_field = ToonlEncoder::new(',', &["id", "name"]).expect("encoder");
    assert_eq!(
        missing_field
            .push_value_row(&Value::from_json_str(r#"{"id":1}"#).expect("row"))
            .unwrap_err()
            .to_string(),
        "TOONL output schema changed"
    );

    let mut non_primitive = ToonlEncoder::new(',', &["id"]).expect("encoder");
    assert_eq!(
        non_primitive
            .push_value_row(&Value::from_json_str(r#"{"id":[1,2]}"#).expect("row"))
            .unwrap_err()
            .to_string(),
        "TOONL rows must be flat objects"
    );

    let mut cadence = ToonlEncoder::new(',', &["id"]).expect("encoder");
    assert_eq!(
        cadence
            .set_continuation_every_rows(Some(0))
            .unwrap_err()
            .to_string(),
        "TOONL continuation cadence must be positive"
    );
    assert_eq!(
        cadence
            .set_continuation_every_bytes(Some(0))
            .unwrap_err()
            .to_string(),
        "TOONL continuation cadence must be positive"
    );

    assert_eq!(
        ToonlEncoder::new('#', &["id"]).unwrap_err().to_string(),
        "invalid header delimiter"
    );
    assert_eq!(
        ToonlEncoder::new(',', &[] as &[&str])
            .unwrap_err()
            .to_string(),
        "TOONL header requires fields"
    );
    assert_eq!(
        ToonlEncoder::new(',', &["\"\""]).unwrap_err().to_string(),
        "TOONL header requires fields"
    );
}

#[test]
fn encode_toonl_values_rejects_non_object_empty_and_nested_rows() {
    assert_eq!(
        encode_toonl_values(&[Value::Number("1".to_owned())])
            .unwrap_err()
            .to_string(),
        "TOONL output requires object rows"
    );
    assert_eq!(
        encode_toonl_values(&[Value::Object(Document::default())])
            .unwrap_err()
            .to_string(),
        "TOONL output requires object rows"
    );
    assert_eq!(
        encode_toonl_values(&[Value::from_json_str(r#"{"a":[1]}"#).expect("row")])
            .unwrap_err()
            .to_string(),
        "TOONL rows must be flat objects"
    );
}

#[test]
fn jsonl_to_toonl_skips_blank_lines() {
    let mut output = Vec::new();
    jsonl_to_toonl(lines(b"{\"a\":1}\n\n{\"a\":2}\n"), &mut output)
        .expect("blank lines between JSONL records are ignored");
    assert_eq!(String::from_utf8(output).unwrap(), "[]{a}:\n1\n2\n[=2]\n");
}

#[test]
fn toonl_bridges_propagate_read_and_write_failures() {
    let mut sink = Vec::new();
    let read_error = jsonl_to_toonl(FailingReader, &mut sink)
        .expect_err("a failing reader surfaces as a read error");
    assert_eq!(read_error.to_string(), "read error: simulated read failure");

    let mut reader = ToonlReader::new(FailingReader);
    let reader_error = reader
        .next()
        .expect("a failing reader still yields one item")
        .expect_err("the item is the propagated read error");
    assert_eq!(
        reader_error.to_string(),
        "read error: simulated read failure"
    );

    let record = Value::from_json_str(r#"{"a":1}"#).expect("row");

    let mut writer = ToonlWriter::new(FailingWriter);
    let write_error = writer
        .write_record(&record)
        .expect_err("a failing writer surfaces as a write error");
    assert_eq!(
        write_error.to_string(),
        "write error: simulated write failure"
    );

    let mut lane_writer = ToonlWriter::new(FailingWriter);
    let declare_error = lane_writer
        .declare_lane("req", &["a"])
        .expect_err("declaring a lane on a failing writer fails to write");
    assert_eq!(
        declare_error.to_string(),
        "write error: simulated write failure"
    );

    let mut tagged_writer = ToonlWriter::new(FailingWriter);
    let tagged_error = tagged_writer
        .write_tagged_record("req", &record)
        .expect_err("writing a tagged record on a failing writer fails to write");
    assert_eq!(
        tagged_error.to_string(),
        "write error: simulated write failure"
    );

    let write_side_error = jsonl_to_toonl(lines(b"{\"a\":1}\n"), FailingWriter)
        .expect_err("a failing writer surfaces as a write error during jsonl_to_toonl");
    assert_eq!(
        write_side_error.to_string(),
        "write error: simulated write failure"
    );

    let toonl = b"[]{a}:\n1\n[=1]\n";
    let to_jsonl_error = toonl_to_jsonl(lines(toonl), FailingWriter)
        .expect_err("a failing writer surfaces as a write error during toonl_to_jsonl");
    assert_eq!(
        to_jsonl_error.to_string(),
        "write error: simulated write failure"
    );

    let close_error = close_transform_stream(lines(toonl), FailingWriter)
        .expect_err("a failing writer surfaces as a write error during close_transform_stream");
    assert_eq!(
        close_error.to_string(),
        "write error: simulated write failure"
    );
}

#[test]
fn toonl_header_syntax_errors_name_the_offending_shape() {
    // A bracket that is not empty, `~`, `|`, `\t`, or an `=`-led trailer shape
    // is not a delimiter TOONL defines.
    let unknown_delimiter = ToonlReader::new(lines(b"[x]{a}:\na\n"))
        .collect::<Result<Vec<_>, _>>()
        .expect_err("an undefined delimiter symbol is rejected")
        .to_string();
    assert_eq!(unknown_delimiter, "line 1: invalid header delimiter");

    let combined_tag_and_delimiter = ToonlReader::new(lines(b"[|]<req>{a}:\nreq:1\n"))
        .collect::<Result<Vec<_>, _>>()
        .expect_err("a tag cannot be combined with an explicit delimiter")
        .to_string();
    assert_eq!(
        combined_tag_and_delimiter,
        "line 1: invalid header delimiter"
    );

    let missing_braces = ToonlReader::new(lines(b"[]notabrace:\na\n"))
        .collect::<Result<Vec<_>, _>>()
        .expect_err("a header without a `{fields}:` body is rejected")
        .to_string();
    assert_eq!(missing_braces, "line 1: invalid header");

    let empty_field_name = ToonlReader::new(lines(b"[]{a,,b}:\na,x,b\n"))
        .collect::<Result<Vec<_>, _>>()
        .expect_err("a blank field name between delimiters is rejected")
        .to_string();
    assert_eq!(empty_field_name, "line 1: invalid header fields");

    let no_fields = ToonlReader::new(lines(b"[]{}:\n"))
        .collect::<Result<Vec<_>, _>>()
        .expect_err("an empty field list is rejected")
        .to_string();
    assert_eq!(no_fields, "line 1: invalid header fields");
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
fn a_trailing_decimal_point_is_not_numeric_like_and_needs_no_quotes() {
    // "1." fails the digits-after-the-dot check the numeric-like scan uses to
    // decide quoting, so unlike "1e-6" or "05" it is written bare. It still
    // round-trips as a string, since the decoder's own number grammar (§4)
    // requires digits after the dot too.
    assert_eq!(Value::String("1.".to_owned()).to_canonical_toon(), "1.");
    assert_eq!(json_of("v: 1.\n"), json!({"v": "1."}));
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

