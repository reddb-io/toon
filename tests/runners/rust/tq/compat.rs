//! Contract checks for `tq jq-check`, the jq-compatibility decision.
//!
//! Four pairings are pinned here:
//!
//! - every fixture in `tests/corpus/tq/compat/` decides the way it declares;
//! - every positive fixture names a parity corpus case, which is replayed so
//!   the yes is backed by the pinned jq-1.7.1 result;
//! - every divergence the ledger records is either refused outright or is a
//!   case where jq 1.7.1 itself fails, which is the one thing a positive
//!   decision does not promise;
//! - every builtin the evaluator's registry dispatches is a builtin the
//!   decision knows about, so the two cannot drift apart.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

#[path = "jq_corpus.rs"]
mod jq_corpus;

use jq_corpus::{
    assert_expected, field, fields, location, read_cases, run_tq, stderr, stdout, tq_binary, Case,
};

/// One declared compatibility decision.
#[derive(Debug)]
struct Fixture {
    location: String,
    name: String,
    filter: String,
    options: Vec<String>,
    compatible: bool,
    /// The parity case backing a positive decision.
    parity: Option<String>,
    /// The reason kind a negative decision must carry.
    reason: Option<String>,
}

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_fixtures() -> Vec<Fixture> {
    let directory = repository().join("tests/corpus/tq/compat");
    let mut paths = fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
        .map(|entry| entry.expect("read compat corpus entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "cases")
        })
        .collect::<Vec<_>>();
    paths.sort();

    let mut fixtures = Vec::new();
    for path in paths {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        for raw in source.split("\n\n") {
            let block = raw.trim();
            if block.is_empty() || block.lines().all(|line| line.starts_with('#')) {
                continue;
            }
            fixtures.push(parse_fixture(&path, block));
        }
    }

    assert!(
        !fixtures.is_empty(),
        "the compatibility corpus must not be empty"
    );
    fixtures
}

fn parse_fixture(path: &Path, block: &str) -> Fixture {
    let location = location(path, block);
    let fields = fields(
        &location,
        block,
        &["case", "filter", "options", "compatible", "parity", "reason"],
    );

    let compatible = match field(&fields, "compatible").as_deref() {
        Some("yes") => true,
        Some("no") => false,
        _ => panic!("{location}: `compatible` is `yes` or `no`"),
    };
    let parity = field(&fields, "parity");
    let reason = field(&fields, "reason");
    assert_eq!(
        parity.is_some(),
        compatible,
        "{location}: a positive fixture names its parity case, a negative one does not"
    );
    assert_eq!(
        reason.is_some(),
        !compatible,
        "{location}: a negative fixture names its reason kind, a positive one does not"
    );

    Fixture {
        name: field(&fields, "case").unwrap_or_else(|| panic!("{location}: missing case name")),
        filter: field(&fields, "filter").unwrap_or_else(|| panic!("{location}: missing filter")),
        options: field(&fields, "options")
            .unwrap_or_default()
            .split_whitespace()
            .map(str::to_owned)
            .collect(),
        compatible,
        parity,
        reason,
        location,
    }
}

/// The decision `tq jq-check` reaches for one invocation.
#[derive(Debug)]
struct Decision {
    compatible: bool,
    kinds: Vec<String>,
    output: Output,
}

fn decide(options: &[String], filter: &str) -> Decision {
    let output = Command::new(tq_binary())
        .arg("jq-check")
        .args(options)
        .args(["--", filter])
        .stdin(Stdio::null())
        .output()
        .expect("run tq jq-check");

    let report: serde_json::Value = serde_json::from_str(&stdout(&output)).unwrap_or_else(|error| {
        panic!(
            "jq-check must print one JSON object ({error}): {:?} {:?}",
            stdout(&output),
            stderr(&output)
        )
    });
    let compatible = report["compatible"]
        .as_bool()
        .expect("the report carries a boolean decision");
    let kinds = report["reasons"]
        .as_array()
        .expect("the report carries a reason array")
        .iter()
        .map(|reason| {
            reason["kind"]
                .as_str()
                .expect("a reason carries a kind")
                .to_owned()
        })
        .collect();

    assert_eq!(
        compatible,
        output.status.success(),
        "the exit status must repeat the decision"
    );
    assert_eq!(
        report["jq_version"], "1.7.1",
        "the decision speaks for the pinned jq"
    );
    assert_eq!(
        report["filter"], filter,
        "the report echoes the filter it decided"
    );

    Decision {
        compatible,
        kinds,
        output,
    }
}

fn parity_cases() -> BTreeMap<String, Case> {
    read_cases(&repository().join("tests/corpus/tq/parity"))
        .into_iter()
        .map(|case| (case.name.clone(), case))
        .collect()
}

#[test]
fn every_fixture_decides_the_way_it_declares() {
    for fixture in read_fixtures() {
        let decision = decide(&fixture.options, &fixture.filter);
        assert_eq!(
            decision.compatible, fixture.compatible,
            "{}: {} decided {:?}",
            fixture.location, fixture.name, decision.kinds
        );
        if let Some(reason) = &fixture.reason {
            assert!(
                decision.kinds.iter().any(|kind| kind == reason),
                "{}: expected a `{reason}` reason, got {:?}",
                fixture.location,
                decision.kinds
            );
        }
    }
}

/// Deciding is not evaluating: the report is the whole of stdout, nothing
/// reaches stderr, and no filter — not even a halting or erroring one — runs.
#[test]
fn a_decision_never_produces_query_output() {
    const KEYS: [&str; 5] = ["jq_version", "filter", "options", "compatible", "reasons"];

    let mut filters = read_fixtures()
        .into_iter()
        .map(|fixture| (fixture.options, fixture.filter))
        .collect::<Vec<_>>();
    filters.push((Vec::new(), "1,2,halt".to_owned()));
    filters.push((Vec::new(), "error(\"boom\")".to_owned()));
    filters.push((Vec::new(), "debug".to_owned()));

    for (options, filter) in filters {
        let decision = decide(&options, &filter);
        let report: serde_json::Value = serde_json::from_str(&stdout(&decision.output))
            .expect("the decision is one JSON object");
        let object = report.as_object().expect("the decision is an object");
        assert_eq!(
            object.keys().map(String::as_str).collect::<Vec<_>>(),
            KEYS,
            "{filter}: the report carries exactly its documented keys"
        );
        assert_eq!(
            stderr(&decision.output),
            "",
            "{filter}: deciding writes nothing to stderr"
        );
    }
}

#[test]
fn every_positive_fixture_replays_through_the_parity_oracle() {
    let cases = parity_cases();
    let mut replayed = 0;

    for fixture in read_fixtures() {
        let Some(name) = &fixture.parity else {
            continue;
        };
        let case = cases.get(name).unwrap_or_else(|| {
            panic!(
                "{}: `{name}` is not a parity corpus case",
                fixture.location
            )
        });
        assert!(
            case.divergence.is_none(),
            "{}: `{name}` records a divergence, so it cannot back a positive decision",
            fixture.location
        );
        assert_expected(case, &run_tq(case));
        replayed += 1;
    }

    assert!(replayed > 0, "positive fixtures must name parity cases");
}

/// A positive decision promises that tq reproduces jq 1.7.1 wherever jq
/// succeeds. Every ledgered divergence must therefore either be refused, or be
/// a case the corpus records jq itself failing on.
#[test]
fn every_ledgered_divergence_is_refused_or_is_one_jq_fails() {
    let mut checked = 0;

    for case in parity_cases().into_values() {
        let Some(jq_output) = &case.jq_output else {
            continue;
        };
        checked += 1;
        if !decide(&[], &case.filter).compatible {
            continue;
        }
        assert!(
            jq_output.starts_with("error: "),
            "{}: jq 1.7.1 succeeds here, so the divergence must be refused",
            case.location
        );
    }

    assert!(checked > 0, "the corpus must record divergences");
}

/// Every `Builtin::new("name", arity, …)` the registry declares. The registry
/// is private to the crate, so it is read from its source, exactly as the
/// language-reference contract does.
fn registry() -> Vec<(String, usize)> {
    let directory = repository().join("crates/tq/src/query/builtins");
    let mut signatures = Vec::new();

    let entries = fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()));
    for entry in entries {
        let path = entry.expect("read builtins directory entry").path();
        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }

        let source = fs::read_to_string(&path).expect("read builtin module");
        for declaration in source.split("Builtin::new(\"").skip(1) {
            let (name, rest) = declaration
                .split_once('"')
                .expect("terminated builtin name");
            let arity = rest
                .trim_start_matches(',')
                .split(',')
                .next()
                .expect("builtin arity follows the name")
                .trim()
                .parse()
                .expect("builtin arity is a number");
            signatures.push((name.to_owned(), arity));
        }
    }

    assert!(
        !signatures.is_empty(),
        "the builtin registry must not be empty"
    );
    signatures
}

#[test]
fn the_decision_knows_every_builtin_the_registry_dispatches() {
    for (name, arity) in registry() {
        let arguments = vec!["."; arity].join(";");
        let filter = if arity == 0 {
            name.clone()
        } else {
            format!("{name}({arguments})")
        };
        let decision = decide(&[], &filter);
        assert!(
            !decision.kinds.iter().any(|kind| kind == "unsupported-builtin"),
            "`{name}/{arity}` is registered but the decision calls it unsupported"
        );
    }
}

#[test]
fn the_decision_reports_every_reason_it_finds() {
    let decision = decide(&["--stream".to_owned()], "sin, trim");
    assert!(!decision.compatible);
    assert_eq!(
        decision.kinds,
        ["unsupported-option", "unsupported-builtin", "divergent-builtin"]
    );
}

#[test]
fn a_filter_is_taken_from_the_last_argument_when_no_separator_is_given() {
    let output = Command::new(tq_binary())
        .args(["jq-check", "-c", ".a"])
        .stdin(Stdio::null())
        .output()
        .expect("run tq jq-check");
    assert!(output.status.success(), "{}", stderr(&output));
    let report: serde_json::Value =
        serde_json::from_str(&stdout(&output)).expect("one JSON object");
    assert_eq!(report["filter"], ".a");
    assert_eq!(report["options"], serde_json::json!(["-c"]));
}

#[test]
fn a_misshapen_invocation_is_a_usage_error() {
    for arguments in [
        vec!["jq-check"],
        vec!["jq-check", "--", ".a", ".b"],
        vec!["jq-check", "--arg", "only", ".a"],
        vec!["jq-check", "stray", "-c", ".a"],
        vec!["jq-check", "-p", "json"],
    ] {
        let output = Command::new(tq_binary())
            .args(&arguments)
            .stdin(Stdio::null())
            .output()
            .expect("run tq jq-check");
        assert!(!output.status.success(), "{arguments:?} should be rejected");
        assert!(
            stderr(&output).contains("usage: tq jq-check"),
            "{arguments:?}: {}",
            stderr(&output)
        );
    }
}
