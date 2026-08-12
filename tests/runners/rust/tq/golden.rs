use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug)]
struct Case {
    args: Vec<String>,
    stdin: String,
    stdout: String,
    stderr: ExpectedStderr,
    exit_code: i32,
}

/// A case pins stderr one of two ways, and which one it picks says what the
/// bytes mean. `stderr.txt` is an exact pin, for stderr that *is* the behaviour
/// under test: what `debug`, `stderr` and `halt_error` write is program output,
/// so a byte of it changing is a real regression. `stderr-contains.txt` lists
/// one required fragment per line, for tq's own `error: …` diagnostics: a case
/// pins which construct failed, not how the sentence reads, so a parser slice
/// is free to reword its own diagnostic without parking on this suite.
#[derive(Debug)]
enum ExpectedStderr {
    Exact(String),
    Contains(Vec<String>),
}

#[test]
fn golden_cli_cases() {
    for path in case_dirs() {
        let name = case_name(&path);
        let case = read_case(&path);
        let output = run_case(&case);

        assert_eq!(
            output.status.code(),
            Some(case.exit_code),
            "{name} exit code"
        );
        assert_eq!(
            String::from_utf8(output.stdout).expect("stdout is utf-8"),
            case.stdout,
            "{name} stdout"
        );
        assert_stderr(
            &case.stderr,
            &String::from_utf8(output.stderr).expect("stderr is utf-8"),
            &name,
        );
    }
}

/// The guard behind #313. Pinning a diagnostic byte-for-byte hands every later
/// parser slice a failing suite for wording it changed on purpose — three
/// attempts parked that way on #270 alone. A diagnostic goes in
/// `stderr-contains.txt`, where the fragments name the construct that failed.
#[test]
fn no_golden_case_pins_exact_diagnostic_wording() {
    for path in case_dirs() {
        let Ok(text) = fs::read_to_string(path.join("stderr.txt")) else {
            continue;
        };

        assert!(
            !text.lines().any(|line| line.starts_with("error: ")),
            "{}: stderr.txt pins tq's diagnostic wording byte-for-byte; move the \
             fragments that identify the failure to stderr-contains.txt",
            case_name(&path)
        );
    }
}

/// De-pinning wording only stays honest while the fragments left behind still
/// tell the diagnostics apart. `error:` alone would pass every error case and
/// assert nothing, so each fragment set has to reject every other case's
/// stderr — the cheapest mechanical proof that a case pins its own failure.
#[test]
fn stderr_fragments_identify_one_case_each() {
    let observed: Vec<(String, ExpectedStderr, String)> = case_dirs()
        .into_iter()
        .map(|path| {
            let case = read_case(&path);
            let stderr = String::from_utf8(run_case(&case).stderr).expect("stderr is utf-8");
            (case_name(&path), case.stderr, stderr)
        })
        .collect();

    for (name, expected, _) in &observed {
        let ExpectedStderr::Contains(fragments) = expected else {
            continue;
        };

        for (other, _, stderr) in &observed {
            if other == name {
                continue;
            }
            assert!(
                !fragments.iter().all(|fragment| stderr.contains(fragment)),
                "{name}'s fragments also match {other}, so they do not identify a failure"
            );
        }
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
fn retired_nested_and_keyed_switches_are_usage_errors() {
    for flag in ["--nested-tabular-headers", "--keyed-map-collapse"] {
        let output = run_tq(&[flag, "-o", "toon", "."], "name: Ada\n");
        let stderr = String::from_utf8(output.stderr).expect("stderr is utf-8");

        assert_eq!(output.status.code(), Some(1), "{flag} is rejected");
        assert_eq!(output.stdout, b"", "{flag} produces no output");
        assert!(stderr.starts_with("error: usage: tq"), "{stderr}");
        assert!(!stderr.contains(flag), "usage must not advertise {flag}: {stderr}");
    }
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

fn case_dirs() -> Vec<PathBuf> {
    let root = env!("CARGO_MANIFEST_DIR");
    let cases_dir = Path::new(root).join("../../tests/golden/tq");

    let mut dirs: Vec<PathBuf> = fs::read_dir(cases_dir)
        .expect("golden cases directory exists")
        .map(|entry| entry.expect("golden case directory entry is readable"))
        .filter(|entry| entry.file_type().expect("golden case file type").is_dir())
        .map(|entry| entry.path())
        .collect();
    dirs.sort();

    assert!(!dirs.is_empty(), "the golden case corpus should not be empty");
    dirs
}

fn case_name(path: &Path) -> String {
    path.file_name()
        .expect("golden case directory has a name")
        .to_string_lossy()
        .into_owned()
}

fn assert_stderr(expected: &ExpectedStderr, actual: &str, name: &str) {
    match expected {
        ExpectedStderr::Exact(text) => assert_eq!(actual, text, "{name} stderr"),
        ExpectedStderr::Contains(fragments) => {
            for fragment in fragments {
                assert!(
                    actual.contains(fragment.as_str()),
                    "{name} stderr does not report {fragment:?}, got: {actual}"
                );
            }
        }
    }
}

fn read_case(path: &Path) -> Case {
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
        stderr: read_expected_stderr(path),
        exit_code,
    }
}

fn read_expected_stderr(path: &Path) -> ExpectedStderr {
    let exact = fs::read_to_string(path.join("stderr.txt")).ok();
    let contains = fs::read_to_string(path.join("stderr-contains.txt")).ok();

    match (exact, contains) {
        (Some(text), None) => ExpectedStderr::Exact(text),
        (None, Some(text)) => {
            let fragments: Vec<String> = text
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(ToOwned::to_owned)
                .collect();
            assert!(
                !fragments.is_empty(),
                "{}: stderr-contains.txt lists no fragment, so it asserts nothing",
                case_name(path)
            );
            ExpectedStderr::Contains(fragments)
        }
        (Some(_), Some(_)) => panic!(
            "{}: a case pins stderr exactly or by fragment, not both",
            case_name(path)
        ),
        (None, None) => panic!(
            "{}: a case needs stderr.txt or stderr-contains.txt",
            case_name(path)
        ),
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
