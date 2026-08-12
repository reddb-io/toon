//! Data-driven compatibility checks against jq 1.7.1.
//!
//! Vendored expectations are the primary, hermetic contract. The optional jq
//! validator is a maintenance aid: it runs only when the exact reference
//! version is available.

use std::io;
use std::path::PathBuf;
use std::process::{Command, Output};

#[path = "jq_corpus.rs"]
mod jq_corpus;

use jq_corpus::{
    assert_expected, read_corpus, run_filter, run_tq, stderr, stdout, Case, Expected,
};

const PINNED_JQ_VERSION: &str = "jq-1.7.1";

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
