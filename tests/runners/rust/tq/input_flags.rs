//! Input-side CLI flags: `-n`, `--raw-input`, `--arg`, and `--argjson`.
//!
//! Expectations are jq 1.7.1's, including the edge cases the golden fixtures
//! cannot express: values containing spaces, unreadable stdin, and diagnostics.

use std::io::{self, Write};
use std::process::{Command, Stdio};

const TOONL_ROWS: &str = "[]{id,name}:\n1,Ada\n2,Linus\n[=2]\n";

#[test]
fn null_input_evaluates_the_filter_once_against_null() {
    let output = run_tq(&["-n", "-o", "json", "-c", "."], "");

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout is utf-8"),
        "null\n"
    );
}

/// `-n` never decodes stdin, so input that no reader could parse is harmless.
#[test]
fn null_input_ignores_unparsable_stdin() {
    for flag in ["-n", "--null-input"] {
        let output = run_tq(&[flag, "-p", "json", "-o", "json", "-c", "1 + 1"], "{{{");

        assert_eq!(output.status.code(), Some(0), "{flag} exits cleanly");
        assert_eq!(
            String::from_utf8(output.stdout).expect("stdout is utf-8"),
            "2\n",
            "{flag} evaluates without stdin"
        );
    }
}

/// A file operand is likewise left unread under `-n`, even a missing one.
#[test]
fn null_input_ignores_the_file_operand() {
    let output = run_tq(&["-n", "-o", "json", "-c", ".", "no-such-file.toon"], "");

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout is utf-8"),
        "null\n"
    );
}

#[test]
fn raw_input_evaluates_one_string_per_line() {
    for flag in ["-R", "--raw-input"] {
        let output = run_tq(&[flag, "-o", "json", "-c", "."], "alpha\nbeta\n");

        assert_eq!(output.status.code(), Some(0), "{flag} exits cleanly");
        assert_eq!(
            String::from_utf8(output.stdout).expect("stdout is utf-8"),
            "\"alpha\"\n\"beta\"\n",
            "{flag} reads lines as strings"
        );
    }
}

/// The trailing newline ends the last line instead of starting an empty one,
/// an unterminated last line is still a line, and empty input has none.
#[test]
fn raw_input_splits_lines_the_way_jq_does() {
    let cases = [
        ("alpha\nbeta\n", "\"alpha\"\n\"beta\"\n"),
        ("alpha\nbeta", "\"alpha\"\n\"beta\"\n"),
        ("\n", "\"\"\n"),
        ("a\n\nb\n", "\"a\"\n\"\"\n\"b\"\n"),
        ("", ""),
    ];

    for (input, expected) in cases {
        let output = run_tq(&["--raw-input", "-o", "json", "-c", "."], input);

        assert_eq!(output.status.code(), Some(0), "{input:?} exits cleanly");
        assert_eq!(
            String::from_utf8(output.stdout).expect("stdout is utf-8"),
            expected,
            "{input:?} splits into lines"
        );
    }
}

/// `--raw-input --slurp` keeps the whole input, trailing newline included, as
/// one string document.
#[test]
fn raw_input_slurp_evaluates_the_whole_input_as_one_string() {
    let output = run_tq(
        &["--raw-input", "--slurp", "-o", "json", "-c", "[length, .]"],
        "alpha\nbeta\n",
    );

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout is utf-8"),
        "[11,\"alpha\\nbeta\\n\"]\n"
    );
}

/// Raw input replaces decoding, so an explicit input format is overridden
/// rather than applied to each line.
#[test]
fn raw_input_overrides_the_input_format() {
    let output = run_tq(
        &["-p", "json", "--raw-input", "-o", "json", "-c", "."],
        "{{{\n",
    );

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout is utf-8"),
        "\"{{{\"\n"
    );
}

#[test]
fn arg_binds_a_string_variable() {
    let output = run_tq(
        &[
            "-p",
            "json",
            "-o",
            "json",
            "-c",
            "--arg",
            "greeting",
            "hello there",
            "{name: .name, greeting: $greeting}",
        ],
        r#"{"name":"Ada"}"#,
    );

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout is utf-8"),
        "{\"name\":\"Ada\",\"greeting\":\"hello there\"}\n"
    );
}

/// `--arg` never interprets its value, so a number stays a string and a
/// flag-shaped value is still a value.
#[test]
fn arg_values_are_taken_verbatim() {
    let output = run_tq(
        &[
            "-n",
            "-o",
            "json",
            "-c",
            "--arg",
            "count",
            "7",
            "--arg",
            "flagish",
            "--oops",
            "[$count, $flagish]",
        ],
        "",
    );

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout is utf-8"),
        "[\"7\",\"--oops\"]\n"
    );
}

#[test]
fn argjson_binds_a_parsed_json_variable() {
    let output = run_tq(
        &[
            "-n",
            "-o",
            "json",
            "-c",
            "--argjson",
            "limits",
            r#"{"max":10,"tags":["a","b"]}"#,
            "[$limits.max, ($limits.tags | length)]",
        ],
        "",
    );

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout is utf-8"),
        "[10,2]\n"
    );
}

/// Flag variables reach every evaluation mode, including the per-row TOONL
/// stream where the query is applied once per record.
#[test]
fn named_variables_resolve_in_every_evaluation_mode() {
    let single = run_tq(
        &[
            "-p",
            "json",
            "-o",
            "json",
            "-c",
            "--arg",
            "tag",
            "prod",
            "{name: .name, tag: $tag}",
        ],
        r#"{"name":"Ada"}"#,
    );
    assert_eq!(
        single.status.code(),
        Some(0),
        "single document exits cleanly"
    );
    assert_eq!(
        String::from_utf8(single.stdout).expect("stdout is utf-8"),
        "{\"name\":\"Ada\",\"tag\":\"prod\"}\n"
    );

    let rows = run_tq(
        &[
            "-p",
            "toonl",
            "-o",
            "json",
            "-c",
            "--arg",
            "tag",
            "prod",
            "{name: .name, tag: $tag}",
        ],
        TOONL_ROWS,
    );
    assert_eq!(rows.status.code(), Some(0), "toonl rows exit cleanly");
    assert_eq!(
        String::from_utf8(rows.stdout).expect("stdout is utf-8"),
        "{\"name\":\"Ada\",\"tag\":\"prod\"}\n{\"name\":\"Linus\",\"tag\":\"prod\"}\n"
    );

    let slurped = run_tq(
        &[
            "-p",
            "toonl",
            "-o",
            "json",
            "-c",
            "-s",
            "--argjson",
            "keep",
            "1",
            "map(.name)[:$keep]",
        ],
        TOONL_ROWS,
    );
    assert_eq!(slurped.status.code(), Some(0), "toonl slurp exits cleanly");
    assert_eq!(
        String::from_utf8(slurped.stdout).expect("stdout is utf-8"),
        "[\"Ada\"]\n"
    );

    let raw = run_tq(
        &[
            "--raw-input",
            "-o",
            "json",
            "-c",
            "--arg",
            "sep",
            "-",
            "[., $sep] | join(\"\")",
        ],
        "alpha\nbeta\n",
    );
    assert_eq!(raw.status.code(), Some(0), "raw input exits cleanly");
    assert_eq!(
        String::from_utf8(raw.stdout).expect("stdout is utf-8"),
        "\"alpha-\"\n\"beta-\"\n"
    );
}

/// jq exposes the same variables through `$ARGS.named`; `positional` stays
/// empty because tq has no `--args` operand list.
#[test]
fn args_object_reports_the_named_variables() {
    let empty = run_tq(&["-n", "-o", "json", "-c", "$ARGS"], "");
    assert_eq!(empty.status.code(), Some(0), "bare $ARGS exits cleanly");
    assert_eq!(
        String::from_utf8(empty.stdout).expect("stdout is utf-8"),
        "{\"positional\":[],\"named\":{}}\n"
    );

    let named = run_tq(
        &[
            "-n",
            "-o",
            "json",
            "-c",
            "--arg",
            "a",
            "1",
            "--argjson",
            "b",
            "[2]",
            "$ARGS",
        ],
        "",
    );
    assert_eq!(named.status.code(), Some(0), "$ARGS exits cleanly");
    assert_eq!(
        String::from_utf8(named.stdout).expect("stdout is utf-8"),
        "{\"positional\":[],\"named\":{\"a\":\"1\",\"b\":[2]}}\n"
    );
}

/// A repeated name keeps its first binding, and a query binding of the same
/// name still shadows the flag inside its own scope.
#[test]
fn repeated_and_shadowed_names_follow_jq() {
    let repeated = run_tq(
        &[
            "-n",
            "-o",
            "json",
            "-c",
            "--arg",
            "x",
            "first",
            "--arg",
            "x",
            "second",
            "[$x, $ARGS.named.x]",
        ],
        "",
    );
    assert_eq!(repeated.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(repeated.stdout).expect("stdout is utf-8"),
        "[\"first\",\"first\"]\n"
    );

    let shadowed = run_tq(
        &[
            "-n",
            "-o",
            "json",
            "-c",
            "--arg",
            "x",
            "flag",
            "\"inner\" as $x | $x",
        ],
        "",
    );
    assert_eq!(shadowed.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(shadowed.stdout).expect("stdout is utf-8"),
        "\"inner\"\n"
    );
}

#[test]
fn missing_and_invalid_flag_values_are_reported() {
    let cases: [(&[&str], &str); 7] = [
        (
            &["-n", "--arg"],
            "`--arg` expects a variable name and a value",
        ),
        (
            &["-n", "--arg", "name"],
            "`--arg` expects a variable name and a value",
        ),
        (
            &["-n", "--argjson"],
            "`--argjson` expects a variable name and JSON text",
        ),
        (
            &["-n", "--argjson", "name"],
            "`--argjson` expects a variable name and JSON text",
        ),
        (
            &["-n", "--argjson", "name", "nope", "."],
            "`--argjson` value for `$name` is not valid JSON",
        ),
        (
            &["-n", "--argjson", "name", "", "."],
            "`--argjson` value for `$name` is not valid JSON",
        ),
        (
            &["-n", "-o", "json", "$missing"],
            "variable `$missing` is not defined",
        ),
    ];

    for (args, message) in cases {
        let output = run_tq(args, "");
        let stderr = String::from_utf8(output.stderr).expect("stderr is utf-8");

        assert_eq!(output.status.code(), Some(1), "{args:?} fails");
        assert_eq!(output.stdout, b"", "{args:?} prints no result");
        assert!(
            stderr.contains(message),
            "{args:?} reports `{message}`, got: {stderr}"
        );
    }
}

const TOONL_STREAM: &str = "[]{id,name}:\n1,Ada\n2,Linus\n3,Grace\n4,Ken\n[=4]\n";

/// `inputs` reads the rows the loop has not reached yet, so a filter that
/// gathers them sees the rest of the stream and the loop then has none left.
#[test]
fn inputs_draws_the_rest_of_a_toonl_stream() {
    let output = run_tq(
        &["-p", "toonl", "-o", "json", "-c", "[.,inputs]|map(.name)"],
        TOONL_STREAM,
    );

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout is utf-8"),
        "[\"Ada\",\"Linus\",\"Grace\",\"Ken\"]\n"
    );
}

/// The reader is one cursor, not a copy of the stream: `input` takes the row
/// the loop would have taken next, so four rows pair into two results.
#[test]
fn input_and_the_row_loop_share_one_cursor() {
    let output = run_tq(
        &["-p", "toonl", "-o", "json", "-c", "[.name,input.name]"],
        TOONL_STREAM,
    );

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout is utf-8"),
        "[\"Ada\",\"Linus\"]\n[\"Grace\",\"Ken\"]\n"
    );
}

/// `-n` runs the filter once and leaves every row for `inputs` to draw.
#[test]
fn null_input_leaves_the_whole_stream_to_inputs() {
    let output = run_tq(
        &["-n", "-p", "toonl", "-o", "json", "-c", "[inputs|.id]"],
        TOONL_STREAM,
    );

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout is utf-8"),
        "[1,2,3,4]\n"
    );
}

/// The rows come from the live reader rather than a slurped array: a row the
/// filter never asks for is never decoded, so a stream that is malformed
/// further along still succeeds.
#[test]
fn a_row_the_filter_never_reaches_is_never_decoded() {
    let truncated = "[]{id,name}:\n1,Ada\n2,Bob,surplus\n";

    let read_one = run_tq(
        &["-n", "-p", "toonl", "-o", "json", "-c", "input.name"],
        truncated,
    );
    assert_eq!(read_one.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(read_one.stdout).expect("stdout is utf-8"),
        "\"Ada\"\n"
    );

    // The counterexample: reaching that row does report it.
    let read_all = run_tq(&["-p", "toonl", "-o", "json", "-c", "."], truncated);
    assert_eq!(read_all.status.code(), Some(1));
    assert!(
        String::from_utf8(read_all.stderr)
            .expect("stderr is utf-8")
            .contains("row arity mismatch"),
        "the malformed row is reported once it is read"
    );
}

/// `--slurp` has already consumed the stream into the document, so nothing is
/// left for `inputs` to read.
#[test]
fn slurping_leaves_no_rows_for_inputs() {
    let output = run_tq(
        &[
            "-s",
            "-p",
            "toonl",
            "-o",
            "json",
            "-c",
            "[length,([inputs]|length)]",
        ],
        TOONL_STREAM,
    );

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout is utf-8"),
        "[4,0]\n"
    );
}

/// `--raw-input` streams lines the same way, one string document at a time.
#[test]
fn inputs_draws_the_rest_of_the_raw_input_lines() {
    let output = run_tq(
        &["-R", "-o", "json", "-c", "[.,inputs]"],
        "alpha\nbeta\ngamma\n",
    );

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout is utf-8"),
        "[\"alpha\",\"beta\",\"gamma\"]\n"
    );
}

/// A single document is the whole input, so `input` has nothing to hand back.
#[test]
fn input_reports_an_exhausted_stream() {
    let output = run_tq(&["-p", "json", "-o", "json", "-c", "input"], "1");
    let stderr = String::from_utf8(output.stderr).expect("stderr is utf-8");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"");
    assert!(stderr.contains("No more inputs"), "{stderr}");
}

/// `halt_error` mid-stream keeps the rows already written, writes its payload
/// to stderr, and exits with jq's status for an unnumbered halt.
#[test]
fn halt_error_in_a_stream_keeps_the_rows_before_it() {
    let output = run_tq(
        &[
            "-p",
            "toonl",
            "-o",
            "json",
            "-c",
            ".ok//(\"row 2 is not ok\"|halt_error)",
        ],
        "[]{id,ok}:\n1,true\n2,false\n3,true\n[=3]\n",
    );

    assert_eq!(output.status.code(), Some(5));
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout is utf-8"),
        "true\n"
    );
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr is utf-8"),
        "row 2 is not ok"
    );
}

/// The usage line advertises the input-side flags.
#[test]
fn usage_lists_the_input_flags() {
    let output = run_tq(&["--not-a-flag"], "");
    let stderr = String::from_utf8(output.stderr).expect("stderr is utf-8");

    assert_eq!(output.status.code(), Some(1));
    for flag in [
        "[-n|--null-input]",
        "[-R|--raw-input]",
        "[--arg name value]",
        "[--argjson name json]",
    ] {
        assert!(stderr.contains(flag), "usage lists {flag}, got: {stderr}");
    }
}

fn run_tq(args: &[&str], stdin: &str) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_tq"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn tq");

    if let Err(error) = child
        .stdin
        .as_mut()
        .expect("stdin is piped")
        .write_all(stdin.as_bytes())
    {
        // A tq that never reads stdin (usage error, or `-n`) may exit first;
        // the resulting broken pipe is not a test failure.
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe, "write stdin");
    }

    child.wait_with_output().expect("wait for tq")
}
