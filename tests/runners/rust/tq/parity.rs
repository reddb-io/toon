//! Data-driven compatibility checks against jq 1.7.1.
//!
//! Vendored expectations are the primary, hermetic contract. The optional jq
//! validator is a maintenance aid: it runs only when the exact reference
//! version is available.

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const PINNED_JQ_VERSION: &str = "jq-1.7.1";

#[derive(Debug)]
struct Case {
    location: String,
    name: String,
    filter: String,
    input: String,
    expected: Expected,
    divergence: Option<String>,
    jq_output: Option<String>,
}

#[derive(Debug)]
enum Expected {
    Output(String),
    Error(String),
}

#[test]
fn vendored_jq_parity_corpus() {
    let cases = read_corpus();
    assert!(!cases.is_empty(), "jq parity corpus must not be empty");

    for case in &cases {
        let actual = run_tq(case);
        assert_expected(case, &actual);
    }
}

#[test]
fn jq_1_7_1_validates_corpus() {
    let Some(jq) = pinned_jq() else {
        return;
    };

    for case in read_corpus() {
        let actual = run_filter(&jq, &case.filter, &case.input);
        match (&case.divergence, &case.jq_output) {
            (None, None) => assert_jq_matches_vendored(&case, &actual),
            (Some(reason), Some(expected)) => {
                assert!(
                    !reason.trim().is_empty(),
                    "{}: empty divergence",
                    case.location
                );
                assert_jq_documented(&case, expected, &actual);
                assert_jq_differs_from_tq(&case, &actual);
            }
            _ => panic!(
                "{}: divergence and jq-output must appear together",
                case.location
            ),
        }
    }
}

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus/tq/parity")
}

fn read_corpus() -> Vec<Case> {
    let directory = corpus_dir();
    let mut paths = fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
        .map(|entry| entry.expect("read parity corpus entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "cases")
        })
        .collect::<Vec<_>>();
    paths.sort();

    let mut cases = Vec::new();
    let mut names = BTreeSet::new();
    for path in paths {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        for raw in source.split("\n\n") {
            let block = raw.trim();
            if block.is_empty() || block.lines().all(|line| line.starts_with('#')) {
                continue;
            }
            let case = parse_case(&path, block);
            assert!(
                names.insert(case.name.clone()),
                "{}: duplicate case name `{}`",
                case.location,
                case.name
            );
            cases.push(case);
        }
    }
    cases
}

fn parse_case(path: &Path, block: &str) -> Case {
    let mut name = None;
    let mut filter = None;
    let mut input = None;
    let mut output = None;
    let mut error = None;
    let mut divergence = None;
    let mut jq_output = None;
    let first_line = block.lines().next().expect("non-empty case block");
    let location = format!("{} ({first_line})", path.display());

    for line in block.lines() {
        if line.starts_with('#') {
            continue;
        }
        let (key, encoded) = line
            .split_once(": ")
            .unwrap_or_else(|| panic!("{location}: expected `key: JSON string`"));
        let value: String = serde_json::from_str(encoded)
            .unwrap_or_else(|error| panic!("{location}: invalid {key} value: {error}"));
        let slot = match key {
            "case" => &mut name,
            "filter" => &mut filter,
            "input" => &mut input,
            "output" => &mut output,
            "error" => &mut error,
            "divergence" => &mut divergence,
            "jq-output" => &mut jq_output,
            _ => panic!("{location}: unknown field `{key}`"),
        };
        assert!(
            slot.replace(value).is_none(),
            "{location}: duplicate `{key}`"
        );
    }

    let expected = match (output, error) {
        (Some(value), None) => Expected::Output(value),
        (None, Some(value)) => Expected::Error(value),
        _ => panic!("{location}: exactly one of output or error is required"),
    };

    Case {
        location,
        name: name.unwrap_or_else(|| panic!("missing case name")),
        filter: filter.unwrap_or_else(|| panic!("missing filter")),
        input: input.unwrap_or_else(|| panic!("missing input")),
        expected,
        divergence,
        jq_output,
    }
}

fn run_tq(case: &Case) -> Output {
    run_filter(
        Path::new(env!("CARGO_BIN_EXE_tq")),
        &case.filter,
        &case.input,
    )
}

fn run_filter(binary: &Path, filter: &str, stdin: &str) -> Output {
    let mut command = Command::new(binary);
    if binary.ends_with("tq") {
        command.args(["-p", "json", "-o", "json"]);
    }
    let mut child = command
        .args(["-c", "--", filter])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("spawn {}: {error}", binary.display()));

    if let Err(error) = child
        .stdin
        .as_mut()
        .expect("stdin is piped")
        .write_all(stdin.as_bytes())
    {
        assert_eq!(
            error.kind(),
            io::ErrorKind::BrokenPipe,
            "write filter stdin"
        );
    }
    child.wait_with_output().expect("wait for filter process")
}

fn assert_expected(case: &Case, actual: &Output) {
    match &case.expected {
        Expected::Output(expected) => {
            assert_eq!(
                actual.status.code(),
                Some(0),
                "{}: {}",
                case.location,
                stderr(actual)
            );
            assert_eq!(stdout(actual), *expected, "{}", case.location);
        }
        Expected::Error(expected) => {
            assert!(
                !actual.status.success(),
                "{}: expected an error",
                case.location
            );
            assert!(
                stderr(actual).contains(expected),
                "{}: expected error containing {expected:?}, got {:?}",
                case.location,
                stderr(actual)
            );
        }
    }
}

fn pinned_jq() -> Option<PathBuf> {
    let binary = PathBuf::from("jq");
    let version = match Command::new(&binary).arg("--version").output() {
        Ok(output) => output,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            eprintln!("warning: {PINNED_JQ_VERSION} is not on PATH; skipping jq corpus validation");
            return None;
        }
        Err(error) => panic!("probe jq version: {error}"),
    };
    let found = stdout(&version).trim().to_owned();
    if !version.status.success() || found != PINNED_JQ_VERSION {
        eprintln!(
            "warning: expected {PINNED_JQ_VERSION} on PATH, found {}; skipping jq corpus validation",
            if found.is_empty() { "an unreadable jq" } else { &found }
        );
        return None;
    }
    Some(binary)
}

fn assert_jq_matches_vendored(case: &Case, actual: &Output) {
    match &case.expected {
        Expected::Output(expected) => {
            assert_eq!(
                actual.status.code(),
                Some(0),
                "{}: jq: {}",
                case.location,
                stderr(actual)
            );
            assert_eq!(stdout(actual), *expected, "{}: jq output", case.location);
        }
        Expected::Error(expected) => {
            assert!(
                !actual.status.success(),
                "{}: jq should fail",
                case.location
            );
            assert!(
                stderr(actual).contains(expected),
                "{}: jq error: {}",
                case.location,
                stderr(actual)
            );
        }
    }
}

fn assert_jq_documented(case: &Case, expected: &str, actual: &Output) {
    if let Some(message) = expected.strip_prefix("error: ") {
        assert!(
            !actual.status.success(),
            "{}: jq should fail",
            case.location
        );
        assert!(
            stderr(actual).contains(message),
            "{}: jq error: {}",
            case.location,
            stderr(actual)
        );
    } else {
        assert_eq!(
            actual.status.code(),
            Some(0),
            "{}: jq: {}",
            case.location,
            stderr(actual)
        );
        assert_eq!(
            stdout(actual),
            expected,
            "{}: documented jq output",
            case.location
        );
    }
}

fn assert_jq_differs_from_tq(case: &Case, jq: &Output) {
    let tq = run_tq(case);
    assert!(
        tq.status.code() != jq.status.code() || tq.stdout != jq.stdout || tq.stderr != jq.stderr,
        "{}: documented divergence is stale",
        case.location
    );
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is utf-8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr is utf-8")
}
