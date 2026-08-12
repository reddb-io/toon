//! The vendored jq-1.7.1 parity corpus, and the plumbing that replays it.
//!
//! Included by `parity.rs`, which checks tq against the corpus, and by
//! `compat.rs`, which checks that `tq jq-check` only says yes to filters the
//! corpus pins. Each target uses part of this surface, so the module carries a
//! crate-wide `dead_code` allowance rather than a note on every item.
#![allow(dead_code)]

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// One corpus case: a filter, its input, and the result jq 1.7.1 pins.
#[derive(Debug)]
pub struct Case {
    pub location: String,
    pub name: String,
    pub filter: String,
    pub input: String,
    pub expected: Expected,
    /// Set when the case records a deliberate tq-ism, with the ledger's reason.
    pub divergence: Option<String>,
    /// jq 1.7.1's own result for a divergent case: compact stdout, or `error: `
    /// followed by a required stderr substring.
    pub jq_output: Option<String>,
}

#[derive(Debug)]
pub enum Expected {
    Output(String),
    Error(String),
}

pub fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus/tq/parity")
}

/// Reads every `.cases` file in `directory`, in file then declaration order,
/// and asserts case names stay unique across the whole set.
pub fn read_cases(directory: &Path) -> Vec<Case> {
    let mut paths = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
        .map(|entry| entry.expect("read corpus entry").path())
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

pub fn read_corpus() -> Vec<Case> {
    read_cases(&corpus_dir())
}

/// Splits one case block into `key: JSON string` fields. Unknown keys are an
/// error, so a typo cannot quietly drop an expectation.
pub fn fields(location: &str, block: &str, known: &[&str]) -> Vec<(String, String)> {
    let mut fields: Vec<(String, String)> = Vec::new();
    for line in block.lines() {
        if line.starts_with('#') {
            continue;
        }
        let (key, encoded) = line
            .split_once(": ")
            .unwrap_or_else(|| panic!("{location}: expected `key: JSON string`"));
        assert!(
            known.contains(&key),
            "{location}: unknown field `{key}`"
        );
        assert!(
            !fields.iter().any(|(seen, _)| seen == key),
            "{location}: duplicate `{key}`"
        );
        let value: String = serde_json::from_str(encoded)
            .unwrap_or_else(|error| panic!("{location}: invalid {key} value: {error}"));
        fields.push((key.to_owned(), value));
    }
    fields
}

pub fn field(fields: &[(String, String)], key: &str) -> Option<String> {
    fields
        .iter()
        .find_map(|(name, value)| (name == key).then(|| value.clone()))
}

/// The location label a case block reports itself under.
pub fn location(path: &Path, block: &str) -> String {
    let first_line = block.lines().next().expect("non-empty case block");
    format!("{} ({first_line})", path.display())
}

fn parse_case(path: &Path, block: &str) -> Case {
    let location = location(path, block);
    let fields = fields(
        &location,
        block,
        &[
            "case",
            "filter",
            "input",
            "output",
            "error",
            "divergence",
            "jq-output",
        ],
    );

    let expected = match (field(&fields, "output"), field(&fields, "error")) {
        (Some(value), None) => Expected::Output(value),
        (None, Some(value)) => Expected::Error(value),
        _ => panic!("{location}: exactly one of output or error is required"),
    };

    Case {
        name: field(&fields, "case").unwrap_or_else(|| panic!("{location}: missing case name")),
        filter: field(&fields, "filter").unwrap_or_else(|| panic!("{location}: missing filter")),
        input: field(&fields, "input").unwrap_or_else(|| panic!("{location}: missing input")),
        expected,
        divergence: field(&fields, "divergence"),
        jq_output: field(&fields, "jq-output"),
        location,
    }
}

pub fn tq_binary() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_tq"))
}

pub fn run_tq(case: &Case) -> Output {
    run_filter(tq_binary(), &case.filter, &case.input)
}

pub fn run_filter(binary: &Path, filter: &str, stdin: &str) -> Output {
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

pub fn assert_expected(case: &Case, actual: &Output) {
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

pub fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is utf-8")
}

pub fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr is utf-8")
}
