use std::fs;
use std::io::{self, Write};
use std::process::{Command, Stdio};

#[derive(Debug)]
struct Case {
    args: Vec<String>,
    stdin: String,
    stdout: String,
    stderr: String,
    exit_code: i32,
}

#[test]
fn golden_cli_cases() {
    let root = env!("CARGO_MANIFEST_DIR");
    let cases_dir = std::path::Path::new(root).join("../../tests/golden/tq");

    for entry in fs::read_dir(cases_dir).expect("golden cases directory exists") {
        let entry = entry.expect("golden case directory entry is readable");
        if !entry.file_type().expect("golden case file type").is_dir() {
            continue;
        }

        let case = read_case(&entry.path());
        let output = run_case(&case);

        assert_eq!(
            output.status.code(),
            Some(case.exit_code),
            "{} exit code",
            entry.file_name().to_string_lossy()
        );
        assert_eq!(
            String::from_utf8(output.stdout).expect("stdout is utf-8"),
            case.stdout,
            "{} stdout",
            entry.file_name().to_string_lossy()
        );
        assert_eq!(
            String::from_utf8(output.stderr).expect("stderr is utf-8"),
            case.stderr,
            "{} stderr",
            entry.file_name().to_string_lossy()
        );
    }
}

#[test]
fn join_output_golden_case() {
    let output = run_tq(
        &["-p", "json", "-o", "json", "-j", ".[]"],
        "[\"Ada\",\"Bob\"]",
    );

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"AdaBob");
    assert_eq!(output.stderr, b"");
}

#[test]
fn join_output_compact_json_golden_case() {
    let output = run_tq(
        &["-p", "json", "-o", "json", "-j", "-c", ".[]"],
        "[{\"b\":2},{\"a\":1}]",
    );

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, br#"{"b":2}{"a":1}"#);
    assert_eq!(output.stderr, b"");
}

#[test]
fn join_output_raw_toon_golden_case() {
    let output = run_tq(&["-p", "json", "-o", "toon", "-j", "-r", ".[]"], "[1,2]");

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"12");
    assert_eq!(output.stderr, b"");
}

#[test]
fn sort_keys_compact_json_golden_case() {
    let output = run_tq(
        &["-p", "json", "-o", "json", "-S", "-c", "."],
        r#"{"z":{"b":1,"a":2},"a":[{"d":4,"c":3}]}"#,
    );

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        output.stdout,
        br#"{"a":[{"c":3,"d":4}],"z":{"a":2,"b":1}}
"#
    );
    assert_eq!(output.stderr, b"");
}

#[test]
fn sort_keys_toon_golden_case() {
    let output = run_tq(
        &["-p", "json", "-o", "toon", "-S", "."],
        r#"{"z":{"b":1,"a":2},"a":[{"d":4,"c":3}]}"#,
    );

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"a[1]{c,d}:\n  3,4\nz:\n  a: 2\n  b: 1\n");
    assert_eq!(output.stderr, b"");
}

#[test]
fn exit_status_false_golden_case() {
    let output = run_tq(
        &["-p", "json", "-o", "json", "-e", "-r", "-c", "false"],
        "null",
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"false\n");
    assert_eq!(output.stderr, b"");
}

#[test]
fn exit_status_no_output_golden_case() {
    let output = run_tq(&["-p", "json", "-o", "toon", "-e", "select(false)"], "null");

    assert_eq!(output.status.code(), Some(4));
    assert_eq!(output.stdout, b"");
    assert_eq!(output.stderr, b"");
}

#[test]
fn exit_status_toonl_stream_golden_case() {
    let output = run_tq(
        &["-p", "toonl", "-o", "json", "-c", "-e", ".active"],
        "[]{id,active}:\n1,true\n2,false\n[=2]\n",
    );

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"true\nfalse\n");
    assert_eq!(output.stderr, b"");
}

#[test]
fn combined_output_flags_golden_case() {
    let output = run_tq(
        &[
            "-p", "json", "-o", "json", "-j", "-S", "-e", "-r", "-c", ".[]",
        ],
        r#"[{"b":2,"a":1},true]"#,
    );

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, br#"{"a":1,"b":2}true"#);
    assert_eq!(output.stderr, b"");
}

#[test]
fn output_flags_usage_golden_case() {
    let output = run_tq(&["--not-a-flag"], "");
    let stderr = String::from_utf8(output.stderr).expect("stderr is utf-8");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"");
    assert!(stderr.contains("[-j] [-S] [-e]"), "{stderr}");
}

#[test]
fn toon_json_toon_round_trips_to_same_canonical_form() {
    let input = "name: Ada\nusers[2]{id,name}:\n  1,Ada\n  2,Bob\n";
    let to_json = run_tq(&["-o", "json", "."], input);
    assert_eq!(to_json.status.code(), Some(0), "TOON to JSON exits cleanly");

    let json = String::from_utf8(to_json.stdout).expect("json stdout is utf-8");
    let back_to_toon = run_tq(&["-p", "json", "-o", "toon", "."], &json);
    assert_eq!(
        back_to_toon.status.code(),
        Some(0),
        "JSON to TOON exits cleanly"
    );
    assert_eq!(
        String::from_utf8(back_to_toon.stdout).expect("toon stdout is utf-8"),
        input
    );
}

fn read_case(path: &std::path::Path) -> Case {
    let args = fs::read_to_string(path.join("args.txt"))
        .expect("args fixture exists")
        .split_whitespace()
        .map(ToOwned::to_owned)
        .collect();

    let exit_code = fs::read_to_string(path.join("exit.txt"))
        .expect("exit fixture exists")
        .trim()
        .parse()
        .expect("exit fixture is an integer");

    Case {
        args,
        stdin: fs::read_to_string(path.join("stdin.toon")).expect("stdin fixture exists"),
        stdout: fs::read_to_string(path.join("stdout.toon")).expect("stdout fixture exists"),
        stderr: fs::read_to_string(path.join("stderr.txt")).expect("stderr fixture exists"),
        exit_code,
    }
}

fn run_case(case: &Case) -> std::process::Output {
    run_tq(
        &case.args.iter().map(String::as_str).collect::<Vec<_>>(),
        &case.stdin,
    )
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
