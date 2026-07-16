use std::io::{self, Read, Write};

use reddb_io_toon::{
    close_transform_stream, close_transform_stream_interleaved, detect_toonl_truncation,
    detect_truncation_with_options, encode_toonl_values, jsonl_to_toonl, toonl_to_jsonl, Array,
    Document, EncodeOptions, ParseOptions, ToonlCursor, ToonlCursorInvalidation, ToonlEncoder,
    ToonlReader, ToonlResumeError, ToonlStream, ToonlWriter, Value,
};
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

fn lines(input: &[u8]) -> std::io::Cursor<&[u8]> {
    std::io::Cursor::new(input)
}

fn deeply_nested_toon(depth: usize) -> String {
    let mut input = String::new();
    for index in 0..=depth {
        input.push_str(&"  ".repeat(index));
        input.push_str(&format!("k{index}:\n"));
    }
    input
}

fn deeply_nested_value(depth: usize) -> Value {
    let mut value = Value::String("leaf".to_owned());
    for index in (0..=depth).rev() {
        value = Value::from_json_value(json!({ format!("k{index}"): value.to_json_value() }));
    }
    value
}

/// A reader that always fails, so read-error propagation paths can be
/// exercised without depending on real I/O failures.
struct FailingReader;

impl Read for FailingReader {
    fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("simulated read failure"))
    }
}

impl std::io::BufRead for FailingReader {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        Err(io::Error::other("simulated read failure"))
    }

    fn consume(&mut self, _amount: usize) {}
}

/// A writer that always fails, so write-error propagation paths can be
/// exercised without depending on real I/O failures.
struct FailingWriter;

impl Write for FailingWriter {
    fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("simulated write failure"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("simulated write failure"))
    }
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
fn detects_truncation_with_the_shared_structured_report_corpus() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../../corpus/truncation.json")).expect("corpus json");
    for fixture in corpus.as_array().expect("corpus is an array") {
        let input = fixture["input"].as_str().expect("fixture input");
        let report = match fixture["format"].as_str().expect("fixture format") {
            "toon" => detect_truncation_with_options(input, ParseOptions::default()),
            "toonl" => detect_toonl_truncation(input),
            format => panic!("unexpected format {format}"),
        };
        assert_eq!(
            report.to_json_value(),
            fixture["report"],
            "{}",
            fixture["name"].as_str().unwrap_or("fixture")
        );
    }
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
fn decode_enforces_max_depth_and_supports_an_explicit_opt_out() {
    let options = ParseOptions {
        max_depth: 2,
        ..ParseOptions::default()
    };
    let value = Value::parse_with_options("a:\n  b:\n    c: 1\n", options).expect("at limit");
    assert_eq!(value.to_json_value(), json!({"a": {"b": {"c": 1}}}));

    let error = Value::parse_with_options(
        "a:\n  b:\n    c: 1\n",
        ParseOptions {
            max_depth: 1,
            ..ParseOptions::default()
        },
    )
    .expect_err("over custom limit");
    assert_eq!(
        error.to_string(),
        "line 3: maximum nesting depth exceeded (maxDepth 1)"
    );

    let header_error = Value::parse_with_options(
        "rows[1]{a{b{c}}}:\n  1\n",
        ParseOptions {
            max_depth: 2,
            ..ParseOptions::default()
        },
    )
    .expect_err("header nesting is guarded too");
    assert_eq!(
        header_error.to_string(),
        "line 1: maximum nesting depth exceeded (maxDepth 2)"
    );

    let hostile = deeply_nested_toon(1001);
    let error = Value::parse_toon(&hostile).expect_err("over default limit");
    assert_eq!(error.line(), 1002);
    assert!(
        error.to_string().contains("maxDepth 1000"),
        "depth limit appears in {error}"
    );

    Value::parse_with_options(
        "a:\n  b:\n    c: 1\n",
        ParseOptions {
            max_depth: 0,
            ..ParseOptions::default()
        },
    )
    .expect("maxDepth 0 disables the guard");
}

#[test]
fn encode_enforces_max_depth_and_supports_an_explicit_opt_out() {
    let value = deeply_nested_value(1001);
    let error = value
        .try_to_canonical_toon()
        .expect_err("over default encode limit");
    assert_eq!(
        error.to_string(),
        "maximum nesting depth exceeded (maxDepth 1000)"
    );

    value
        .try_to_toon_with_options(EncodeOptions {
            max_depth: 0,
            ..EncodeOptions::default()
        })
        .expect("maxDepth 0 disables the guard");
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

#[test]
fn toonl_reader_and_writer_stream_rows_with_schema_rotation() {
    let input = b"[]{id,name}:\n1,Ada\n[=1]\n[]{id,name,role}:\n2,Linus,dev\n[=1]\n";
    let rows = ToonlReader::new(lines(input))
        .collect::<Result<Vec<_>, _>>()
        .expect("valid TOONL rows");

    assert_eq!(
        rows.iter()
            .map(Value::to_json_value)
            .collect::<Vec<serde_json::Value>>(),
        vec![
            json!({"id": 1, "name": "Ada"}),
            json!({"id": 2, "name": "Linus", "role": "dev"}),
        ]
    );

    let mut output = Vec::new();
    let mut writer = ToonlWriter::new(&mut output);
    for row in &rows {
        writer.write_record(row).expect("write row");
    }
    writer.finish().expect("finish writer");

    assert_eq!(
        String::from_utf8(output).unwrap(),
        String::from_utf8(input.to_vec()).unwrap()
    );
}

#[test]
fn toonl_reader_streams_tagged_rows_in_wire_order() {
    let input = b"[]{event}:\n[]<req>{method,path,status}:\nstarted\nreq:GET,/health,200\nfinished\nreq:POST,/login,401\n";
    let rows = ToonlReader::new(lines(input))
        .collect::<Result<Vec<_>, _>>()
        .expect("valid tagged TOONL rows");

    assert_eq!(
        rows.iter()
            .map(Value::to_json_value)
            .collect::<Vec<serde_json::Value>>(),
        vec![
            json!({"event": "started"}),
            json!({"method": "GET", "path": "/health", "status": 200}),
            json!({"event": "finished"}),
            json!({"method": "POST", "path": "/login", "status": 401}),
        ]
    );
}

#[test]
fn toonl_reader_rejects_unknown_and_overflow_tagged_lanes() {
    let unknown = ToonlReader::new(lines(b"[]<req>{method,path}:\nmetric:cpu,0.42\n"))
        .collect::<Result<Vec<_>, _>>()
        .expect_err("unknown tag is rejected")
        .to_string();
    assert!(unknown.contains("unknown tag"));

    let overflow =
        b"[]<a>{v}:\n[]<b>{v}:\n[]<c>{v}:\n[]<d>{v}:\n[]<e>{v}:\n[]<f>{v}:\n[]<g>{v}:\n[]<h>{v}:\n[]<i>{v}:\n";
    let error = ToonlReader::new(lines(overflow))
        .collect::<Result<Vec<_>, _>>()
        .expect_err("9th live tagged lane is rejected")
        .to_string();
    assert!(error.contains("too many tagged lanes"));
}

#[test]
fn toonl_streaming_reader_accepts_matching_continuation_headers() {
    let input = b"[]{id,name}:\n1,Ada\n[~]{id,name}:\n2,Linus\n[=2]\n";
    let rows = ToonlReader::new(lines(input))
        .collect::<Result<Vec<_>, _>>()
        .expect("valid TOONL rows");

    assert_eq!(
        rows.iter()
            .map(Value::to_json_value)
            .collect::<Vec<serde_json::Value>>(),
        vec![
            json!({"id": 1, "name": "Ada"}),
            json!({"id": 2, "name": "Linus"})
        ]
    );

    let mismatch = b"[]{id,name}:\n1,Ada\n[~]{id,role}:\n2,dev\n";
    let error = ToonlReader::new(lines(mismatch))
        .collect::<Result<Vec<_>, _>>()
        .expect_err("mismatched continuation is rejected")
        .to_string();
    assert!(error.contains("continuation header mismatch"));
}

#[test]
fn toonl_reader_resumes_from_a_serialized_cursor() {
    let input = b"[]{id,name}:\n1,Ada\n2,Linus\n[=2]\n";
    let mut reader = ToonlReader::new(lines(input));

    let first = reader.next().expect("first row").expect("valid row");
    assert_eq!(first.to_json_value(), json!({"id": 1, "name": "Ada"}));
    let cursor = reader.cursor().expect("cursor after first row");
    let persisted = cursor.to_json_string();
    let restored = ToonlCursor::from_json_str(&persisted).expect("cursor JSON round-trip");

    let resumed = ToonlReader::resume_from_bytes(input, restored)
        .expect("valid cursor")
        .collect::<Result<Vec<_>, _>>()
        .expect("resumed rows decode");
    let sequential = ToonlReader::new(lines(input))
        .skip(1)
        .collect::<Result<Vec<_>, _>>()
        .expect("sequential suffix decodes");

    assert_eq!(resumed, sequential);
}

#[test]
fn toonl_reader_resumes_across_continuation_headers() {
    let input = b"[]{id,name}:\n1,Ada\n2,Linus\n[~]{id,name}:\n3,Grace\n[=3]\n";
    let mut reader = ToonlReader::new(lines(input));

    reader.next().expect("first row").expect("valid row");
    reader.next().expect("second row").expect("valid row");
    let cursor = reader.cursor().expect("cursor before continuation");

    let resumed = ToonlReader::resume_from_bytes(input, cursor)
        .expect("valid cursor")
        .collect::<Result<Vec<_>, _>>()
        .expect("resumed rows decode");
    assert_eq!(
        resumed
            .iter()
            .map(Value::to_json_value)
            .collect::<Vec<serde_json::Value>>(),
        vec![json!({"id": 3, "name": "Grace"})]
    );
}

#[test]
fn toonl_reader_reports_cursor_invalidation_distinctly() {
    let input = b"[]{id,name}:\n1,Ada\n2,Linus\n";
    let mut reader = ToonlReader::new(lines(input));
    reader.next().expect("first row").expect("valid row");
    let cursor = reader.cursor().expect("cursor after first row");

    let truncated = ToonlCursor::new(999, "[]{id,name}:\n", 0);
    let error = ToonlReader::resume_from_bytes(input, truncated).expect_err("truncated cursor");
    assert!(matches!(
        error,
        ToonlResumeError::Invalid(ToonlCursorInvalidation::Truncated { .. })
    ));

    let mut rewritten = input.to_vec();
    let row_byte = rewritten
        .iter()
        .position(|byte| *byte == b'1')
        .expect("row byte");
    rewritten[row_byte] = b'9';
    let error = ToonlReader::resume_from_bytes(&rewritten, cursor).expect_err("mutated cursor");
    assert!(matches!(
        error,
        ToonlResumeError::Invalid(ToonlCursorInvalidation::AnchorMismatch { .. })
    ));
}

#[test]
fn toonl_encoders_emit_continuation_headers_only_when_configured() {
    let rows = [
        Value::from_json_str(r#"{"id":1,"name":"Ada"}"#).expect("row"),
        Value::from_json_str(r#"{"id":2,"name":"Linus"}"#).expect("row"),
        Value::from_json_str(r#"{"id":3,"name":"Grace"}"#).expect("row"),
    ];

    assert_eq!(
        encode_toonl_values(&rows).expect("default output"),
        "[]{id,name}:\n1,Ada\n2,Linus\n3,Grace\n[=3]\n"
    );

    let mut encoder = ToonlEncoder::new(',', &["id", "name"]).expect("encoder");
    encoder
        .set_continuation_every_rows(Some(2))
        .expect("row cadence");
    for row in [["1", "Ada"], ["2", "Linus"], ["3", "Grace"]] {
        encoder.push_raw_row(&row).expect("row");
    }
    assert_eq!(
        encoder.finish(),
        "[]{id,name}:\n1,Ada\n2,Linus\n[~]{id,name}:\n3,Grace\n[=3]\n"
    );

    let mut output = Vec::new();
    let mut writer = ToonlWriter::new(&mut output);
    writer
        .set_continuation_every_rows(Some(2))
        .expect("row cadence");
    for row in &rows {
        writer.write_record(row).expect("write row");
    }
    writer.finish().expect("finish writer");
    assert_eq!(
        String::from_utf8(output).unwrap(),
        "[]{id,name}:\n1,Ada\n2,Linus\n[~]{id,name}:\n3,Grace\n[=3]\n"
    );
}

#[test]
fn toonl_record_encoders_canonicalize_shuffled_shape_field_order() {
    let rows = [
        Value::from_json_str(r#"{"id":1,"name":"Ada"}"#).expect("row"),
        Value::from_json_str(r#"{"name":"Linus","id":2}"#).expect("row"),
    ];

    assert_eq!(
        encode_toonl_values(&rows).expect("valid TOONL rows"),
        "[]{id,name}:\n1,Ada\n2,Linus\n[=2]\n"
    );

    let mut output = Vec::new();
    let mut writer = ToonlWriter::new(&mut output);
    for row in &rows {
        writer.write_record(row).expect("write row");
    }
    writer.finish().expect("finish writer");

    assert_eq!(
        String::from_utf8(output).unwrap(),
        "[]{id,name}:\n1,Ada\n2,Linus\n[=2]\n"
    );
}

#[test]
fn toonl_writer_interleaves_tagged_lanes_and_rejects_overflow_before_writing() {
    let req_one =
        Value::from_json_str(r#"{"method":"GET","path":"/health","status":200}"#).expect("req row");
    let metric_one = Value::from_json_str(r#"{"name":"cpu","value":0.42}"#).expect("metric row");
    let req_two =
        Value::from_json_str(r#"{"status":401,"path":"/login","method":"POST"}"#).expect("req row");
    let metric_two = Value::from_json_str(r#"{"value":0.55,"name":"cpu"}"#).expect("metric row");

    let mut output = Vec::new();
    {
        let mut writer = ToonlWriter::new(&mut output);
        writer
            .write_tagged_record("req", &req_one)
            .expect("write req row");
        writer
            .write_tagged_record("metric", &metric_one)
            .expect("write metric row");
        writer
            .write_tagged_record("req", &req_two)
            .expect("write req row");
        writer
            .write_tagged_record("metric", &metric_two)
            .expect("write metric row");
        writer.finish().expect("finish writer");
    }

    let output = String::from_utf8(output).unwrap();
    assert_eq!(
        output,
        "[]<req>{method,path,status}:\n\
         req:GET,/health,200\n\
         []<metric>{name,value}:\n\
         metric:cpu,0.42\n\
         req:POST,/login,401\n\
         metric:cpu,0.55\n"
    );

    let rows = ToonlReader::new(std::io::Cursor::new(output.as_bytes()))
        .collect::<Result<Vec<_>, _>>()
        .expect("tagged rows decode");
    let req_two_wire_order =
        Value::from_json_str(r#"{"method":"POST","path":"/login","status":401}"#).expect("req row");
    let metric_two_wire_order =
        Value::from_json_str(r#"{"name":"cpu","value":0.55}"#).expect("metric row");
    assert_eq!(
        rows,
        vec![
            req_one,
            metric_one,
            req_two_wire_order,
            metric_two_wire_order
        ]
    );

    let mut writer = ToonlWriter::new(Vec::new());
    for tag in ["a", "b", "c", "d", "e", "f", "g", "h"] {
        let row = Value::from_json_str(&format!(r#"{{"v":"{tag}"}}"#)).expect("row");
        writer.write_tagged_record(tag, &row).expect("write lane");
    }
    let row = Value::from_json_str(r#"{"v":"i"}"#).expect("row");
    let error = writer
        .write_tagged_record("i", &row)
        .expect_err("9th live tagged lane is rejected");
    assert!(error.to_string().contains("too many tagged lanes"));
    let output = String::from_utf8(writer.finish().expect("finish writer")).unwrap();
    assert!(!output.contains("[]<i>{v}:"));
    assert!(!output.contains("i:i"));
}

#[test]
fn streaming_bridges_round_trip_jsonl_toonl_jsonl_and_close_documents() {
    let jsonl = br#"{"id":1,"name":"Ada"}
{"id":2,"name":"Linus","role":"dev"}
"#;
    let mut toonl = Vec::new();
    jsonl_to_toonl(lines(jsonl), &mut toonl).expect("jsonl to toonl");

    assert_eq!(
        String::from_utf8(toonl.clone()).unwrap(),
        "[]{id,name}:\n1,Ada\n[=1]\n[]{id,name,role}:\n2,Linus,dev\n[=1]\n"
    );

    let mut back = Vec::new();
    toonl_to_jsonl(lines(&toonl), &mut back).expect("toonl to jsonl");
    assert_eq!(
        String::from_utf8(back).unwrap(),
        "{\"id\":1,\"name\":\"Ada\"}\n{\"id\":2,\"name\":\"Linus\",\"role\":\"dev\"}\n"
    );

    let mut documents = Vec::new();
    close_transform_stream(lines(&toonl), &mut documents).expect("close transform");
    assert_eq!(
        String::from_utf8(documents).unwrap(),
        "[1]{id,name}:\n  1,Ada\n[1]{id,name,role}:\n  2,Linus,dev\n"
    );
}

#[test]
fn close_transform_interleaved_preserves_tagged_row_runs() {
    let toonl =
        b"[]<req>{method,path,status}:\n[]<metric>{name,value}:\nreq:GET,/health,200\nmetric:cpu,0.42\nreq:POST,/login,401\n";

    let mut documents = Vec::new();
    close_transform_stream(lines(toonl), &mut documents).expect("close transform");
    assert_eq!(
        String::from_utf8(documents).unwrap(),
        "[2]{method,path,status}:\n  GET,/health,200\n  POST,/login,401\n[1]{name,value}:\n  cpu,0.42\n"
    );

    let mut interleaved = Vec::new();
    close_transform_stream_interleaved(lines(toonl), &mut interleaved)
        .expect("interleaved close transform");
    assert_eq!(
        String::from_utf8(interleaved).unwrap(),
        "[1]{method,path,status}:\n  GET,/health,200\n[1]{name,value}:\n  cpu,0.42\n[1]{method,path,status}:\n  POST,/login,401\n"
    );
}

#[test]
fn close_transform_interleaved_keeps_anonymous_streams_byte_identical() {
    let toonl = b"[]{id,name}:\n1,Ada\n[=1]\n[|]{id|name}:\n2|Linus\n[=1]\n";

    let mut documents = Vec::new();
    close_transform_stream(lines(toonl), &mut documents).expect("close transform");

    let mut interleaved = Vec::new();
    close_transform_stream_interleaved(lines(toonl), &mut interleaved)
        .expect("interleaved close transform");

    assert_eq!(interleaved, documents);
}

#[test]
fn toonl_error_exposes_the_line_and_message_it_was_built_from() {
    // A cell that fails scalar validation routes through
    // `ToonlError::from_parse_error`, so the accessors surface the wrapped
    // `ParseError`'s line and message rather than a generic one.
    let input = b"[]{a}:\n\"open\n";
    let failure = ToonlReader::new(lines(input))
        .collect::<Result<Vec<_>, _>>()
        .expect_err("an unterminated quote is rejected");

    assert_eq!(failure.line(), 2);
    assert_eq!(failure.message(), "invalid quoted string");
    assert_eq!(failure.to_string(), "line 2: invalid quoted string");
}

#[test]
fn cursor_invalidation_and_resume_error_render_distinct_messages() {
    assert_eq!(
        ToonlCursorInvalidation::Truncated {
            byte_offset: 5,
            file_size: 2
        }
        .to_string(),
        "TOONL cursor invalidated by truncation"
    );
    assert_eq!(
        ToonlCursorInvalidation::AnchorMismatch { byte_offset: 5 }.to_string(),
        "TOONL cursor invalidated by anchor mismatch"
    );

    let input = b"[]{id,name}:\n1,Ada\n[=1]\n";

    // A cursor whose active header line is itself malformed is a `Parse`
    // resume error, not an `Invalid` one, and both variants render through
    // the inner error's own `Display`.
    let malformed = ToonlCursor::new(0u64, "[bad\n", 0usize);
    let malformed_error = ToonlReader::resume_from_bytes(input, malformed)
        .expect_err("an unterminated header bracket is rejected");
    assert_eq!(malformed_error.to_string(), "invalid header");
    assert!(matches!(malformed_error, ToonlResumeError::Parse(_)));

    // An `Invalid` resume error's `Display` also just forwards to the wrapped
    // `ToonlCursorInvalidation`.
    let truncated_cursor = ToonlCursor::new(999, "[]{id,name}:\n", 0);
    let truncated_error = ToonlReader::resume_from_bytes(input, truncated_cursor)
        .expect_err("a cursor past the end of the file is rejected");
    assert_eq!(
        truncated_error.to_string(),
        "TOONL cursor invalidated by truncation"
    );
    assert!(matches!(truncated_error, ToonlResumeError::Invalid(_)));

    // A cursor whose active header line is a continuation or a tagged header
    // is never a legal resume point, since resuming replays it as the
    // anonymous primary header.
    let continuation = ToonlCursor::new(0u64, "[~]{id,name}:\n", 0usize);
    let continuation_error = ToonlReader::resume_from_bytes(input, continuation)
        .expect_err("a continuation header cannot anchor a resume");
    assert_eq!(
        continuation_error.to_string(),
        "invalid cursor activeHeaderLine"
    );

    let tagged = ToonlCursor::new(0u64, "[]<req>{a}:\n", 0usize);
    let tagged_error = ToonlReader::resume_from_bytes(input, tagged)
        .expect_err("a tagged header cannot anchor a resume");
    assert_eq!(tagged_error.to_string(), "invalid cursor activeHeaderLine");

    // A line that is not a header at all (not even a malformed one, since it
    // never opens a bracket) is rejected the same way.
    let non_header = ToonlCursor::new(0u64, "not a header\n", 0usize);
    let non_header_error = ToonlReader::resume_from_bytes(input, non_header)
        .expect_err("a non-header active line is rejected");
    assert_eq!(
        non_header_error.to_string(),
        "invalid cursor activeHeaderLine"
    );

    // A trailer-shaped bracket (`[=…]`) is deliberately not read as a header,
    // so it takes the same "not a header" branch as `non_header` above.
    let trailer_shaped = ToonlCursor::new(0u64, "[=5]{a}:\n", 0usize);
    let trailer_shaped_error = ToonlReader::resume_from_bytes(input, trailer_shaped)
        .expect_err("a trailer-shaped active line is rejected");
    assert_eq!(
        trailer_shaped_error.to_string(),
        "invalid cursor activeHeaderLine"
    );
}
