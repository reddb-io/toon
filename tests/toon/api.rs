//! Library surface: the decoder options, the value accessors, and the corners of
//! the encoder and the error paths that the spec corpus does not reach on its own.

use std::io::{self, Read, Write};

use reddb_io_toon::{
    close_transform_stream, close_transform_stream_interleaved, encode_toonl_values,
    jsonl_to_toonl, toonl_to_jsonl, Array, Document, ParseOptions, ToonlCursor,
    ToonlCursorInvalidation, ToonlEncoder, ToonlReader, ToonlResumeError, ToonlStream, ToonlWriter,
    Value,
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
