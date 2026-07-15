//! CLI behaviour: builtins checked against jq as an oracle, plus the argument
//! handling, diagnostics, and output modes jq has no opinion about.

use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

const SAMPLE: &str = r#"{"users":[{"name":"Ada","score":41,"team":"research"},{"name":"Bob","score":7,"team":"ops"}],"tags":["ops","core","ops"],"meta":{"team":"core","level":2},"phrase":"Ada-Lovelace","n":-3.5,"z":null,"flag":true}"#;

/// Every filter here must agree with jq, value for value.
///
/// `-` is a legal character in a tq identifier, because TOON keys such as
/// `x-items` are legal. Subtraction therefore needs surrounding spaces, and the
/// oracle confirms the spaced form means what jq means.
#[test]
fn jq_oracle_builtins_arithmetic_and_ordering() {
    let filters = [
        // length across every type it accepts
        ".n|length",
        ".phrase|length",
        ".z|length",
        ".meta|length",
        ".tags|length",
        // keys / has over both containers
        ".tags|keys",
        ".meta|keys",
        ".tags|has(1)",
        ".meta|has(\"nope\")",
        // arithmetic and its operand pairings
        ".users[0].score + 1",
        ".users[0].score - 1",
        ".users[0].score * 2",
        ".users[0].score / 2",
        ".phrase + \"!\"",
        ".tags + [\"x\"]",
        ".meta + {\"a\":1}",
        ".z + 1",
        ".tags - [\"ops\"]",
        "(.n) - (.n)",
        // comparisons
        ".users[0].score >= 41",
        ".users[0].score < 7",
        ".users[0].score <= 41",
        ".users[0].name != \"Ada\"",
        // aggregation and ordering
        ".users|map(.score)|add",
        ".tags|add",
        ".users|sort_by([.team,.score])|map(.name)",
        ".users|min_by(.score)|.name",
        ".users|max_by(.score)|.name",
        ".users|group_by(.team)|length",
        ".tags|unique",
        // strings
        ".tags|unique|join(\"|\")",
        ".users|map(.score)|join(\",\")",
        ".phrase|split(\"\")|length",
        ".phrase|test(\"lace$\")",
        // entries
        ".meta|to_entries|map(.key)",
        ".meta|to_entries|from_entries",
        // paths, slices, iteration
        ".users[0:1]|map(.name)",
        // A slice with an omitted bound reaches all the way to that edge.
        ".tags[:2]",
        ".tags[2:]",
        ".users[]|.name",
        "[.users[]|.score]|add",
        ".users|map(select(.score > 10))|length",
        ".flag",
    ];

    for filter in filters {
        let tq = run_tq(&["-p", "json", "-o", "json", "-c", filter], SAMPLE);
        assert_eq!(
            tq.status.code(),
            Some(0),
            "tq exits cleanly for {filter}: {}",
            String::from_utf8_lossy(&tq.stderr)
        );

        let jq = run_jq(filter, SAMPLE);
        assert_eq!(
            jq.status.code(),
            Some(0),
            "jq exits cleanly for {filter}: {}",
            String::from_utf8_lossy(&jq.stderr)
        );

        assert_eq!(
            String::from_utf8(tq.stdout).expect("tq stdout is utf-8"),
            String::from_utf8(jq.stdout).expect("jq stdout is utf-8"),
            "oracle match for {filter}"
        );
    }
}

/// jq orders values null < false < true < numbers < strings < arrays < objects.
#[test]
fn jq_oracle_orders_mixed_types_and_containers() {
    let cases = [
        (
            "sort_by(.k)|map(.k)",
            r#"[{"k":"s"},{"k":1},{"k":null},{"k":true},{"k":[1]},{"k":{"a":1}}]"#,
        ),
        (
            "sort_by(.m)|map(.n)",
            r#"[{"n":2,"m":{"k":2}},{"n":1,"m":{"k":1}}]"#,
        ),
        (
            "sort_by(.k)|map(.n)",
            r#"[{"n":1,"k":[2]},{"n":2,"k":[1]}]"#,
        ),
        ("group_by(.k)|map(length)", r#"[{"k":1},{"k":1},{"k":2}]"#),
        // Two booleans of the same rank compare by value, not just by rank.
        (
            "sort_by(.k)|map(.k)",
            r#"[{"k":true},{"k":false},{"k":true}]"#,
        ),
    ];

    for (filter, input) in cases {
        let tq = run_tq(&["-p", "json", "-o", "json", "-c", filter], input);
        let jq = run_jq(filter, input);

        assert_eq!(
            String::from_utf8(tq.stdout).expect("tq stdout is utf-8"),
            String::from_utf8(jq.stdout).expect("jq stdout is utf-8"),
            "oracle match for {filter} over {input}"
        );
    }
}

/// Filters that produce a value rather than an error, where jq either errors or
/// has no equivalent, so tq's own contract is the reference.
#[test]
fn resolves_missing_paths_and_degenerate_ranges_to_empty_values() {
    let cases = [
        // A path that does not exist yields null rather than failing.
        (".x", "[1]", "null\n"),
        (".[0]", r#"{"a":1}"#, "null\n"),
        (".[]", r#"{"a":1}"#, "null\n"),
        // An inverted or out-of-bounds slice clamps to empty.
        (".[3:1]", "[1,2,3,4]", "[]\n"),
        (".[2:99]", "[1,2,3,4]", "[3,4]\n"),
        // Aggregations over nothing are null, not errors.
        ("add", "[]", "null\n"),
        ("min_by(.x)", "[]", "null\n"),
        ("max_by(.x)", "[]", "null\n"),
        // to_entries indexes arrays positionally.
        ("to_entries|map(.key)", r#"["a","b"]"#, "[0,1]\n"),
        // from_entries accepts the name/value spelling too.
        ("from_entries", r#"[{"name":"a","value":1}]"#, "{\"a\":1}\n"),
        // join stringifies non-strings; null becomes empty.
        ("join(\"-\")", r#"[true,null,1,"x"]"#, "\"true--1-x\"\n"),
        // has() with a number against an object looks the number up as a key
        // rather than failing the way jq does.
        ("has(1)", r#"{"team":"core"}"#, "false\n"),
        ("has(1)", r#"{"1":"core"}"#, "true\n"),
    ];

    for (filter, input, expected) in cases {
        let output = run_tq(&["-p", "json", "-o", "json", "-c", filter], input);

        assert_eq!(
            output.status.code(),
            Some(0),
            "{filter} exits cleanly: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8(output.stdout).expect("stdout is utf-8"),
            expected,
            "{filter} over {input}"
        );
    }
}

/// Every builtin rejects the types it cannot handle, with a diagnostic naming it.
#[test]
fn reports_type_and_arity_errors_per_builtin() {
    let cases = [
        (".z|add", "add cannot be applied to this value"),
        (".flag|length", "boolean has no length"),
        (".phrase|keys", "keys cannot be applied to this value"),
        (".phrase|has(\"x\")", "has() cannot check this value"),
        (
            ".phrase|to_entries",
            "to_entries cannot be applied to this value",
        ),
        (".tags|from_entries", "from_entries expects object entries"),
        (
            ".phrase|from_entries",
            "from_entries cannot be applied to this value",
        ),
        (
            ".tags|split(\",\")",
            "split cannot be applied to this value",
        ),
        (
            ".phrase|join(\",\")",
            "join cannot be applied to this value",
        ),
        (".phrase|unique", "unique cannot be applied to this value"),
        (".phrase|sort_by(.x)", "cannot order non-array"),
        (".phrase|map(.x)", "cannot iterate over non-array"),
        (".flag|test(\"x\")", "test cannot be applied to this value"),
        (".phrase|split(1)", "split argument must be a string"),
        (".users[0].score / 0", "division by zero"),
        (".tags + 1", "cannot add these values"),
        (".tags - 1", "cannot subtract these values"),
    ];

    for (filter, message) in cases {
        assert_error(&["-p", "json", "-o", "json", "-c", filter], SAMPLE, message);
    }

    assert_error(
        &["-p", "json", "-o", "json", "-c", "from_entries"],
        r#"[{"key":null,"value":1}]"#,
        "from_entries keys must be strings or numbers",
    );
    assert_error(
        &["-p", "json", "-o", "json", "-c", ".a * .a"],
        r#"{"a":1e308}"#,
        "number is not finite",
    );
}

#[test]
fn reports_filter_syntax_errors() {
    let cases = [
        (". | tostring", "unsupported identifier `tostring`"),
        (".users[x]", "expected array index"),
        (".[", "expected array index"),
        (". .", "expected identifier"),
        (". |", "unexpected token"),
        ("\"unterminated", "unterminated string literal"),
    ];

    for (filter, message) in cases {
        assert_error(&["-p", "json", "-o", "json", "-c", filter], SAMPLE, message);
    }
}

#[test]
fn reports_argument_and_input_errors() {
    let usage = "usage: tq";

    assert_error(&["-p", "yaml", "."], SAMPLE, "unsupported format `yaml`");
    assert_error(&["-o", "yaml", "."], SAMPLE, "unsupported format `yaml`");
    // A format flag with nothing after it.
    assert_error(&["-p"], SAMPLE, usage);
    // An unknown flag, no query at all, and too many positionals.
    assert_error(&["-z", "."], SAMPLE, usage);
    assert_error(&[], SAMPLE, usage);
    assert_error(&["a", "b", "c"], SAMPLE, usage);
    // An unreadable input file.
    assert_error(
        &["-p", "json", ".", "/nonexistent/tq-input.json"],
        "",
        "No such file or directory",
    );
    // Malformed input in either format.
    assert_error(&["-p", "json", "."], "{not json", "key must be a string");
    assert_error(&["."], "a: 1\n  b: 2\n", "invalid indentation");
}

#[test]
fn reports_trim_and_close_argument_errors() {
    let trim_usage = "usage: tq trim --keep-last N";
    let close_usage = "usage: tq close";

    // Too many positionals, and `--in-place` with no file to write back to.
    assert_error(&["trim", "--keep-last", "1", "a", "b"], "", trim_usage);
    assert_error(
        &["trim", "--keep-last", "1", "--in-place"],
        "[]{a}:\n1\n[=1]\n",
        "--in-place requires FILE",
    );
    // An unknown flag is rejected the same way as for the query subcommand.
    assert_error(&["close", "--bogus"], "", close_usage);
    assert_error(&["close", "a", "b"], "", close_usage);
    // A close target that cannot be read.
    assert_error(
        &["close", "/nonexistent/tq-close-input.toonl"],
        "",
        "No such file or directory",
    );
}

#[test]
fn trim_and_close_read_the_query_after_a_double_dash() {
    let path = temp_file("tq-trim-close-dashdash.toonl");
    std::fs::write(&path, "[]{id,name}:\n1,Ada\n[=1]\n").expect("write toonl input");

    // `--` still ends flag parsing for the trim and close subcommands, the
    // same way it does for the default query mode.
    let trimmed = run_tq(
        &["trim", "--keep-last", "1", "--", path.to_str().unwrap()],
        "",
    );
    assert_eq!(
        trimmed.status.code(),
        Some(0),
        "trim after -- exits cleanly"
    );
    assert_eq!(
        String::from_utf8(trimmed.stdout).expect("stdout is utf-8"),
        "[]{id,name}:\n1,Ada\n[=1]\n"
    );

    let closed = run_tq(&["close", "--", path.to_str().unwrap()], "");
    assert_eq!(
        closed.status.code(),
        Some(0),
        "close after -- exits cleanly"
    );
    assert_eq!(
        String::from_utf8(closed.stdout).expect("stdout is utf-8"),
        "[1]{id,name}:\n  1,Ada\n"
    );

    std::fs::remove_file(&path).expect("remove toonl input");
}

#[test]
fn trims_toonl_across_blank_lines_colon_led_rows_and_tagged_schema_rotation() {
    // A blank line between rows is ignored while scanning trim units, the
    // same way the TOONL reader ignores it.
    let blank = run_tq(&["trim", "--keep-last", "1"], "[]{a}:\n1\n\n2\n[=2]\n");
    assert_eq!(
        blank.status.code(),
        Some(0),
        "blank-line trim exits cleanly"
    );
    assert_eq!(
        String::from_utf8(blank.stdout).expect("stdout is utf-8"),
        "[]{a}:\n2\n[=1]\n"
    );

    // A row whose cell text happens to start with `:` is not mistaken for a
    // tagged-row prefix while scanning.
    let colon_led = run_tq(&["trim", "--keep-last", "1"], "[]{a}:\n:x\n2\n[=2]\n");
    assert_eq!(
        colon_led.status.code(),
        Some(0),
        "colon-led row trim exits cleanly"
    );
    assert_eq!(
        String::from_utf8(colon_led.stdout).expect("stdout is utf-8"),
        "[]{a}:\n2\n[=1]\n"
    );

    // A tagged lane re-declared with a new schema mid-stream updates the
    // live header tq tracks for that tag, rather than appending a second one.
    let rotated = run_tq(
        &["trim", "--keep-last", "1"],
        "[]<req>{a}:\nreq:1\n[]<req>{a,b}:\nreq:2,x\n",
    );
    assert_eq!(
        rotated.status.code(),
        Some(0),
        "tagged schema rotation trim exits cleanly: {}",
        String::from_utf8_lossy(&rotated.stderr)
    );
    assert_eq!(
        String::from_utf8(rotated.stdout).expect("stdout is utf-8"),
        "[]<req>{a,b}:\nreq:2,x\n"
    );
}

#[test]
fn trims_toonl_regenerates_a_missing_trailing_newline_on_retained_headers() {
    // The last header in the file has no trailing newline (a malformed but
    // tolerated input); `--keep-last 0` retains only the live headers, and
    // writing one back out has to add the newline `line_with_lf` owns.
    let path = temp_file("tq-trim-no-trailing-newline.toonl");
    std::fs::write(&path, "[]{a}:\n1\n2\n[]{a,b}:").expect("write toonl input");

    let output = run_tq(&["trim", "--keep-last", "0", path.to_str().unwrap()], "");
    assert_eq!(
        output.status.code(),
        Some(0),
        "trim exits cleanly: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout is utf-8"),
        "[]{a,b}:\n"
    );

    std::fs::remove_file(&path).expect("remove toonl input");
}

#[test]
fn trims_toonl_in_place_reports_the_underlying_filesystem_error() {
    // When the temp-then-rename write itself fails for a reason other than a
    // name collision (here, a read-only parent directory), the atomic writer
    // surfaces the OS error instead of retrying.
    let dir = std::env::temp_dir().join(format!("tq-trim-readonly-dir.{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    let path = dir.join("data.toonl");
    std::fs::write(&path, "[]{id,name}:\n1,Ada\n2,Bob\n[=2]\n").expect("write toonl input");

    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555))
        .expect("make directory read-only");

    // Root ignores directory write permissions, so this guard keeps the test
    // meaningful when it runs as root instead of failing for the wrong reason.
    let probe = dir.join(".tq-trim-write-probe");
    let root_bypasses_permissions = std::fs::write(&probe, b"x").is_ok();
    let _ = std::fs::remove_file(&probe);

    if !root_bypasses_permissions {
        let output = run_tq(
            &[
                "trim",
                "--keep-last",
                "1",
                "--in-place",
                path.to_str().unwrap(),
            ],
            "",
        );
        assert_eq!(
            output.status.code(),
            Some(1),
            "in-place trim into a read-only directory fails"
        );
        let stderr = String::from_utf8(output.stderr).expect("stderr is utf-8");
        assert!(
            stderr.contains("Permission denied") || stderr.contains("permission denied"),
            "reports the filesystem error: {stderr}"
        );
    }

    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755))
        .expect("restore directory permissions");
    std::fs::remove_dir_all(&dir).expect("remove scratch dir");
}

#[test]
fn reports_the_crate_version_for_the_version_flags() {
    for flag in ["--version", "-V"] {
        let output = run_tq(&[flag], "");
        assert_eq!(output.status.code(), Some(0), "{flag} exits cleanly");
        assert_eq!(
            String::from_utf8(output.stdout).expect("stdout is utf-8"),
            format!("tq {}\n", env!("CARGO_PKG_VERSION"))
        );
    }
}

#[test]
fn reads_the_query_after_a_double_dash_and_the_input_from_a_file() {
    let path = std::env::temp_dir().join("tq-behavior-input.json");
    std::fs::write(&path, SAMPLE).expect("write temp input");

    let output = run_tq(
        &[
            "-p",
            "json",
            "-o",
            "json",
            "-c",
            "--",
            ".meta.team",
            path.to_str().expect("temp path is utf-8"),
        ],
        "",
    );

    assert_eq!(output.status.code(), Some(0), "reads the file");
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout is utf-8"),
        "\"core\"\n"
    );

    std::fs::remove_file(&path).expect("remove temp input");
}

#[test]
fn selects_output_shape_with_raw_pretty_and_toon_modes() {
    // Raw output unwraps strings only; other values keep their encoding.
    let raw = run_tq(&["-p", "json", "-o", "json", "-r", ".phrase"], SAMPLE);
    assert_eq!(
        String::from_utf8(raw.stdout).expect("stdout is utf-8"),
        "Ada-Lovelace\n"
    );
    let raw_number = run_tq(&["-p", "json", "-o", "json", "-r", "-c", ".n"], SAMPLE);
    assert_eq!(
        String::from_utf8(raw_number.stdout).expect("stdout is utf-8"),
        "-3.5\n"
    );

    // Without -c, JSON is pretty-printed.
    let pretty = run_tq(&["-p", "json", "-o", "json", ".meta"], SAMPLE);
    assert_eq!(
        String::from_utf8(pretty.stdout).expect("stdout is utf-8"),
        "{\n  \"team\": \"core\",\n  \"level\": 2\n}\n"
    );

    // TOON in, TOON out is the default on both sides.
    let toon = run_tq(&["."], "a: 1\nb[2]: x,y\n");
    assert_eq!(
        String::from_utf8(toon.stdout).expect("stdout is utf-8"),
        "a: 1\nb[2]: x,y\n"
    );

    // A scalar result still gets its terminating newline.
    let scalar = run_tq(&[".a"], "a: 1\n");
    assert_eq!(
        String::from_utf8(scalar.stdout).expect("stdout is utf-8"),
        "1\n"
    );
}

#[test]
fn supports_toonl_row_streams_slurp_output_and_file_detection() {
    let input = "[]{id,name}:\n1,Ada\n2,Linus\n[=2]\n";

    let rows = run_tq(&["-p", "toonl", "-o", "json", "-c", ".name"], input);
    assert_eq!(rows.status.code(), Some(0), "toonl rows exit cleanly");
    assert_eq!(
        String::from_utf8(rows.stdout).expect("stdout is utf-8"),
        "\"Ada\"\n\"Linus\"\n"
    );

    let slurped = run_tq(
        &["-p", "toonl", "-o", "json", "-c", "-s", "map(.name)"],
        input,
    );
    assert_eq!(slurped.status.code(), Some(0), "toonl slurp exits cleanly");
    assert_eq!(
        String::from_utf8(slurped.stdout).expect("stdout is utf-8"),
        "[\"Ada\",\"Linus\"]\n"
    );

    let toonl = run_tq(
        &["-p", "json", "-o", "toonl", "."],
        r#"{"id":1,"name":"Ada"}"#,
    );
    assert_eq!(toonl.status.code(), Some(0), "toonl output exits cleanly");
    assert_eq!(
        String::from_utf8(toonl.stdout).expect("stdout is utf-8"),
        "[]{id,name}:\n1,Ada\n[=1]\n"
    );

    let path = std::env::temp_dir().join("tq-behavior-input.toonl");
    std::fs::write(&path, input).expect("write toonl temp input");
    let detected = run_tq(&["-o", "json", "-c", ".id", path.to_str().unwrap()], "");
    assert_eq!(
        detected.status.code(),
        Some(0),
        "detects .toonl input: {}",
        String::from_utf8_lossy(&detected.stderr)
    );
    assert_eq!(
        String::from_utf8(detected.stdout).expect("stdout is utf-8"),
        "1\n2\n"
    );
    std::fs::remove_file(&path).expect("remove toonl temp input");
}

#[test]
fn trims_toonl_keep_last_to_stdout_and_preserves_noop_bytes() {
    let input = "[]{ts,level,msg}:\n\
2026-07-14T03:00:00Z,info,boot\n\
2026-07-14T03:00:01Z,info,ready\n\
2026-07-14T03:00:02Z,error,\"disk full\"\n\
2026-07-14T03:00:03Z,info,recovered\n\
[=4]\n";
    let path = temp_file("tq-trim-stdout.toonl");
    std::fs::write(&path, input).expect("write toonl input");

    let trimmed = run_tq(&["trim", "--keep-last", "2", path.to_str().unwrap()], "");
    assert_eq!(
        trimmed.status.code(),
        Some(0),
        "trim exits cleanly: {}",
        String::from_utf8_lossy(&trimmed.stderr)
    );
    assert_eq!(
        String::from_utf8(trimmed.stdout).expect("stdout is utf-8"),
        "[]{ts,level,msg}:\n\
2026-07-14T03:00:02Z,error,\"disk full\"\n\
2026-07-14T03:00:03Z,info,recovered\n\
[=2]\n"
    );

    let noop = run_tq(&["trim", "--keep-last", "99", path.to_str().unwrap()], "");
    assert_eq!(noop.status.code(), Some(0), "noop trim exits cleanly");
    assert_eq!(
        String::from_utf8(noop.stdout).expect("stdout is utf-8"),
        input
    );
    std::fs::remove_file(&path).expect("remove toonl input");
}

#[test]
fn trims_toonl_in_place_with_same_result_on_repeat() {
    let input = "[]{id,name}:\n1,Ada\n2,Bob\n3,Cy\n[=3]\n";
    let expected = "[]{id,name}:\n2,Bob\n3,Cy\n[=2]\n";
    let path = temp_file("tq-trim-in-place.toonl");
    std::fs::write(&path, input).expect("write toonl input");

    let first = run_tq(
        &[
            "trim",
            "--keep-last",
            "2",
            "--in-place",
            path.to_str().unwrap(),
        ],
        "",
    );
    assert_eq!(
        first.status.code(),
        Some(0),
        "in-place trim exits cleanly: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(first.stdout.is_empty(), "in-place trim writes no stdout");
    assert_eq!(
        std::fs::read_to_string(&path).expect("read trimmed"),
        expected
    );

    let second = run_tq(
        &[
            "trim",
            "--keep-last",
            "2",
            "--in-place",
            path.to_str().unwrap(),
        ],
        "",
    );
    assert_eq!(
        second.status.code(),
        Some(0),
        "repeat in-place trim exits cleanly"
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("read repeated trim"),
        expected
    );
    std::fs::remove_file(&path).expect("remove toonl input");
}

#[test]
fn trims_toonl_across_schema_rotation_and_zero_rows() {
    let input = "[]{id,name}:\n1,Ada\n2,Bob\n[]{id,name,role}:\n3,Cy,dev\n4,Di,ops\n[=2]\n";

    let spanning = run_tq(&["trim", "--keep-last", "3"], input);
    assert_eq!(
        spanning.status.code(),
        Some(0),
        "rotation trim exits cleanly: {}",
        String::from_utf8_lossy(&spanning.stderr)
    );
    assert_eq!(
        String::from_utf8(spanning.stdout).expect("stdout is utf-8"),
        "[]{id,name}:\n2,Bob\n[]{id,name,role}:\n3,Cy,dev\n4,Di,ops\n[=2]\n"
    );

    let empty = run_tq(&["trim", "--keep-last", "0"], input);
    assert_eq!(
        empty.status.code(),
        Some(0),
        "zero-row trim exits cleanly: {}",
        String::from_utf8_lossy(&empty.stderr)
    );
    assert_eq!(
        String::from_utf8(empty.stdout).expect("stdout is utf-8"),
        "[]{id,name,role}:\n[=0]\n"
    );
}

#[test]
fn trim_property_keeps_last_rows_and_updates_trailer_around_cut() {
    for total in 1..=6 {
        for keep in 0..=total + 2 {
            for trailer in [false, true] {
                let input = numbered_toonl(total, trailer);
                let trim = run_tq(&["trim", "--keep-last", &keep.to_string()], &input);
                assert_eq!(
                    trim.status.code(),
                    Some(0),
                    "trim total={total} keep={keep} trailer={trailer}: {}",
                    String::from_utf8_lossy(&trim.stderr)
                );
                let output = String::from_utf8(trim.stdout).expect("stdout is utf-8");
                if keep >= total {
                    assert_eq!(output, input, "oversized keep is byte-for-byte noop");
                }

                let ids = run_tq(&["-p", "toonl", "-o", "json", "-c", ".id"], &output);
                assert_eq!(
                    ids.status.code(),
                    Some(0),
                    "trimmed output decodes: {}",
                    String::from_utf8_lossy(&ids.stderr)
                );
                let expected_ids = (total.saturating_sub(keep)..total)
                    .map(|index| format!("{}\n", index + 1))
                    .collect::<String>();
                assert_eq!(
                    String::from_utf8(ids.stdout).expect("ids stdout is utf-8"),
                    expected_ids
                );
                if keep < total {
                    if trailer {
                        assert!(
                            output.ends_with(&format!("[={keep}]\n")),
                            "trim recounts the first retained trailer"
                        );
                    } else {
                        assert!(
                            !output.contains("[="),
                            "trim does not invent a trailer for an open segment"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn supports_v0_2_toonl_query_trim_and_close_variants() {
    let input = "[]<req>{method,path,status}:\n\
[]<metric>{name,value}:\n\
req:GET,/health,200\n\
metric:cpu,0.42\n\
[]{event}:\n\
[~]{event}:\n\
started\n\
req:POST,/login,401\n\
metric:mem,0.70\n";

    let queried = run_tq(&["-p", "toonl", "-o", "json", "-c", ".method"], input);
    assert_eq!(
        queried.status.code(),
        Some(0),
        "v0.2 query exits cleanly: {}",
        String::from_utf8_lossy(&queried.stderr)
    );
    assert_eq!(
        String::from_utf8(queried.stdout).expect("stdout is utf-8"),
        "\"GET\"\nnull\nnull\n\"POST\"\nnull\n"
    );

    let trimmed = run_tq(&["trim", "--keep-last", "3"], input);
    assert_eq!(
        trimmed.status.code(),
        Some(0),
        "tagged trim exits cleanly: {}",
        String::from_utf8_lossy(&trimmed.stderr)
    );
    let trimmed = String::from_utf8(trimmed.stdout).expect("stdout is utf-8");
    assert_eq!(
        trimmed,
        "[]<req>{method,path,status}:\n\
[]<metric>{name,value}:\n\
[]{event}:\n\
started\n\
req:POST,/login,401\n\
metric:mem,0.70\n"
    );

    let trimmed_methods = run_tq(&["-p", "toonl", "-o", "json", "-c", ".method"], &trimmed);
    assert_eq!(
        trimmed_methods.status.code(),
        Some(0),
        "trimmed tagged output decodes: {}",
        String::from_utf8_lossy(&trimmed_methods.stderr)
    );
    assert_eq!(
        String::from_utf8(trimmed_methods.stdout).expect("stdout is utf-8"),
        "null\n\"POST\"\nnull\n"
    );

    let per_lane = run_tq(&["close"], input);
    assert_eq!(
        per_lane.status.code(),
        Some(0),
        "per-lane close exits cleanly: {}",
        String::from_utf8_lossy(&per_lane.stderr)
    );
    assert_eq!(
        String::from_utf8(per_lane.stdout).expect("stdout is utf-8"),
        "[2]{method,path,status}:\n  GET,/health,200\n  POST,/login,401\n[2]{name,value}:\n  cpu,0.42\n  mem,0.70\n[1]{event}:\n  started\n"
    );

    let interleaved = run_tq(&["close", "--interleaved"], input);
    assert_eq!(
        interleaved.status.code(),
        Some(0),
        "interleaved close exits cleanly: {}",
        String::from_utf8_lossy(&interleaved.stderr)
    );
    assert_eq!(
        String::from_utf8(interleaved.stdout).expect("stdout is utf-8"),
        "[1]{method,path,status}:\n  GET,/health,200\n[1]{name,value}:\n  cpu,0.42\n[1]{event}:\n  started\n[1]{method,path,status}:\n  POST,/login,401\n[1]{name,value}:\n  mem,0.70\n"
    );
}

fn assert_error(args: &[&str], stdin: &str, message: &str) {
    let output = run_tq(args, stdin);
    let stderr = String::from_utf8(output.stderr).expect("stderr is utf-8");

    assert_eq!(
        output.status.code(),
        Some(1),
        "{args:?} exits with a failure code, got stderr: {stderr}"
    );
    assert!(
        stderr.contains(message),
        "{args:?} reports `{message}`, got: {stderr}"
    );
}

fn numbered_toonl(total: usize, trailer: bool) -> String {
    let mut input = "[]{id,name}:\n".to_owned();
    for index in 1..=total {
        input.push_str(&format!("{index},name{index}\n"));
    }
    if trailer {
        input.push_str(&format!("[={total}]\n"));
    }
    input
}

fn temp_file(name: &str) -> std::path::PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    std::env::temp_dir().join(format!(
        "{name}.{}.{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
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
        // A tq that fails fast (usage error) may exit before reading stdin;
        // the resulting broken pipe is not a test failure.
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe, "write stdin");
    }

    child.wait_with_output().expect("wait for tq")
}

fn run_jq(filter: &str, stdin: &str) -> std::process::Output {
    let mut child = Command::new("jq")
        .args(["-c", filter])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn jq");

    if let Err(error) = child
        .stdin
        .as_mut()
        .expect("stdin is piped")
        .write_all(stdin.as_bytes())
    {
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe, "write jq stdin");
    }

    child.wait_with_output().expect("wait for jq")
}

/// The remaining evaluator and lexer corners.
#[test]
fn jq_oracle_unary_minus_empty_literals_and_container_ordering() {
    let cases = [
        ("-.n", r#"{"n":2}"#),
        ("[]", r#"{"n":2}"#),
        ("{}", r#"{"n":2}"#),
        // null and booleans sort ahead of everything else.
        (
            "sort_by(.k)|map(.k)",
            r#"[{"k":null},{"k":false},{"k":true},{"k":null}]"#,
        ),
        // Arrays compare element-wise, then by length.
        (
            "sort_by(.k)|map(.k)",
            r#"[{"k":[1,2]},{"k":[1]},{"k":[2]}]"#,
        ),
        // Objects compare by key sequence, then by value, then by size.
        (
            "sort_by(.k)|map(.k)",
            r#"[{"k":{"b":1}},{"k":{"a":1}},{"k":{"a":1,"c":2}}]"#,
        ),
        // A multi-output sort key becomes an array key.
        ("sort_by(.a,.b)|map(.a)", r#"[{"a":2,"b":1},{"a":1,"b":2}]"#),
        // A regex escape has to survive the string lexer.
        (r#"test("a\\.b")"#, r#""a.b""#),
    ];

    for (filter, input) in cases {
        let tq = run_tq(&["-p", "json", "-o", "json", "-c", "--", filter], input);
        let jq = run_jq(filter, input);

        assert_eq!(
            tq.status.code(),
            Some(0),
            "tq exits cleanly for {filter}: {}",
            String::from_utf8_lossy(&tq.stderr)
        );
        assert_eq!(
            String::from_utf8(tq.stdout).expect("tq stdout is utf-8"),
            String::from_utf8(jq.stdout).expect("jq stdout is utf-8"),
            "oracle match for {filter} over {input}"
        );
    }
}

#[test]
fn slicing_a_non_array_yields_null() {
    let output = run_tq(&["-p", "json", "-o", "json", "-c", ".phrase[1:2]"], SAMPLE);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout is utf-8"),
        "null\n"
    );
}

#[test]
fn reports_the_remaining_evaluator_and_parser_errors() {
    let cases = [
        (".n * .phrase", r#"{"n":2,"phrase":"x"}"#, "expected number"),
        ("join(\",\")", "[[1],2]", "join cannot stringify this value"),
        (
            "split((\"a\",\"b\"))",
            r#""abc""#,
            "split argument must produce one value",
        ),
        (".a b", "{}", "unexpected trailing filter input"),
        ("{1:2}", "{}", "expected object key"),
        ("map(.", "{}", "expected `RParen`"),
        (". as $x", "{}", "unsupported character `$`"),
        // A lone `=` or `!` that is not doubled up is an unfinished operator.
        (".a=1", "{}", "expected `=`"),
        (".a!1", "{}", "expected `=`"),
    ];

    for (filter, input, message) in cases {
        assert_error(
            &["-p", "json", "-o", "json", "-c", "--", filter],
            input,
            message,
        );
    }
}
