//! Contract checks for the tq language reference.
//!
//! The reference in `docs/tq-language.md` is only worth reading if it cannot
//! drift from what tq actually ships. Three pairings are pinned here:
//!
//! - the builtin catalog against the builtin registry, by name *and* arity;
//! - the "Where tq differs from jq" table against the divergence ledger in
//!   `docs/tq-jq-parity.md`, in the same order;
//! - that ledger against the parity corpus cases carrying a `divergence`
//!   field.
//!
//! The registry is private to the crate, so it is read from its source. That
//! keeps the check honest without widening the crate's public surface.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// A builtin as the catalog and the registry both spell it.
type Signature = (String, usize);

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(relative: &str) -> String {
    let path = repository().join(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// Splits one markdown table row into its cells. `\|` is an escaped pipe
/// inside a cell, not a separator, so it is hidden before the split.
fn cells(row: &str) -> Vec<String> {
    const ESCAPE: char = '\u{0}';
    row.replace("\\|", &ESCAPE.to_string())
        .trim()
        .trim_matches('|')
        .split('|')
        .map(|cell| cell.replace(ESCAPE, "|").trim().to_owned())
        .collect()
}

/// The rows of the first markdown table under `heading`, header and alignment
/// rows dropped.
fn table_rows(document: &str, heading: &str) -> Vec<String> {
    let mut rows = Vec::new();
    let mut inside_section = false;

    for line in document.lines() {
        if line.starts_with("## ") {
            if inside_section && !rows.is_empty() {
                break;
            }
            inside_section = line.trim() == heading;
            continue;
        }
        if !inside_section {
            continue;
        }
        if line.starts_with('|') {
            rows.push(line.to_owned());
        } else if !rows.is_empty() {
            break;
        }
    }

    assert!(
        rows.len() > 2,
        "{heading}: expected a markdown table with rows"
    );
    rows.drain(..2);
    rows
}

/// The backticked spans of a markdown cell.
fn quoted(cell: &str) -> Vec<String> {
    let mut spans = Vec::new();
    let mut rest = cell;
    while let Some(open) = rest.find('`') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('`') else {
            break;
        };
        spans.push(after[..close].to_owned());
        rest = &after[close + 1..];
    }
    spans
}

/// Reads a catalog signature such as `range(lo; hi)` as `("range", 2)`.
///
/// Prose spans that are not a builtin — a `strftime` directive such as `%z`,
/// or a variable such as `$__loc__` — carry no signature and are skipped.
fn signature(span: &str) -> Option<Signature> {
    let (name, arguments) = match span.split_once('(') {
        Some((name, arguments)) => (
            name,
            arguments.trim_end_matches(')').split(';').count().max(1),
        ),
        None => (span, 0),
    };

    let name = name.trim();
    let mut characters = name.chars();
    let first = characters.next()?;
    let named = (first == '@' || first.is_ascii_alphabetic() || first == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_');

    named.then(|| (name.to_owned(), arguments))
}

/// Every `Builtin::new("name", arity, …)` the registry declares.
fn registry() -> BTreeSet<Signature> {
    let directory = repository().join("crates/tq/src/query/builtins");
    let mut signatures = BTreeSet::new();

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
            signatures.insert((name.to_owned(), arity));
        }
    }

    assert!(
        !signatures.is_empty(),
        "the builtin registry must not be empty"
    );
    signatures
}

/// The catalog, split by the status each row carries.
fn catalog() -> (BTreeSet<Signature>, BTreeSet<Signature>) {
    let reference = read("docs/tq-language.md");
    let (_, catalog) = reference
        .split_once("## Builtin catalog")
        .expect("the reference has a builtin catalog");
    let (catalog, _) = catalog
        .split_once("## Where tq differs from jq")
        .expect("the catalog is followed by the divergence table");

    let mut supported = BTreeSet::new();
    let mut absent = BTreeSet::new();

    for line in catalog.lines().filter(|line| line.starts_with("| `")) {
        let cells = cells(line);
        assert_eq!(cells.len(), 3, "catalog row has three cells: {line}");
        let signatures = quoted(&cells[0])
            .into_iter()
            .filter_map(|span| signature(&span));

        match cells[1].as_str() {
            "supported" => supported.extend(signatures),
            "deferred" | "never" => absent.extend(signatures),
            status => panic!("unknown catalog status `{status}`: {line}"),
        }
    }

    (supported, absent)
}

/// The ledger ids, in the order the ledger lists them.
fn ledger() -> Vec<String> {
    table_rows(&read("docs/tq-jq-parity.md"), "## Divergence ledger")
        .iter()
        .map(|row| {
            quoted(&cells(row)[0])
                .first()
                .cloned()
                .expect("a ledger row names its corpus case")
        })
        .collect()
}

#[test]
fn the_catalog_names_every_registered_builtin() {
    let (supported, _) = catalog();
    let registry = registry();

    let missing: Vec<_> = registry.difference(&supported).collect();
    assert!(
        missing.is_empty(),
        "builtins are registered but absent from the catalog: {missing:?}"
    );
}

#[test]
fn the_catalog_claims_no_builtin_the_registry_lacks() {
    let (supported, _) = catalog();
    let registry = registry();

    let invented: Vec<_> = supported.difference(&registry).collect();
    assert!(
        invented.is_empty(),
        "the catalog marks these supported but nothing dispatches them: {invented:?}"
    );
}

#[test]
fn deferred_and_never_builtins_are_really_absent() {
    let (_, absent) = catalog();
    let registry = registry();

    let shipped: Vec<_> = absent.intersection(&registry).collect();
    assert!(
        shipped.is_empty(),
        "the catalog defers these but the registry dispatches them: {shipped:?}"
    );
}

#[test]
fn the_divergence_table_repeats_the_ledger() {
    let table: Vec<_> = table_rows(&read("docs/tq-language.md"), "## Where tq differs from jq")
        .iter()
        .map(|row| {
            quoted(&cells(row)[0])
                .first()
                .cloned()
                .expect("a divergence row names its ledger id")
        })
        .collect();

    assert_eq!(
        table,
        ledger(),
        "the reference's divergence table and the ledger must name the same \
         ids in the same order"
    );
}

#[test]
fn the_ledger_names_every_diverging_corpus_case() {
    let directory = repository().join("tests/corpus/tq/parity");
    let mut cases = BTreeSet::new();

    let entries = fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()));
    for entry in entries {
        let path = entry.expect("read parity corpus entry").path();
        if path
            .extension()
            .is_none_or(|extension| extension != "cases")
        {
            continue;
        }

        let corpus = fs::read_to_string(&path).expect("read parity corpus file");
        for case in corpus.split("\n\n") {
            if !case.lines().any(|line| line.starts_with("divergence:")) {
                continue;
            }
            let name = case
                .lines()
                .find_map(|line| line.strip_prefix("case: "))
                .expect("a case declares its name")
                .trim()
                .trim_matches('"');
            cases.insert(name.to_owned());
        }
    }

    assert!(!cases.is_empty(), "the corpus must record divergences");
    let ledgered: BTreeSet<_> = ledger().into_iter().collect();
    assert_eq!(
        ledgered, cases,
        "every corpus case carrying a `divergence` field needs a ledger row, \
         and every ledger row needs its case"
    );
}

#[test]
fn the_readme_points_at_the_reference() {
    let readme = read("crates/tq/README.md");
    assert!(
        readme.contains("## Query language"),
        "the tq README needs a query language section"
    );
    assert!(
        readme.contains("docs/tq-language.md"),
        "the tq README must link the language reference"
    );
    assert!(
        readme.contains("docs/tq-jq-parity.md"),
        "the tq README must link the divergence ledger"
    );
}

#[test]
fn the_changelog_records_the_object_iteration_break() {
    let changelog = read("CHANGELOG.md");
    let (_, unreleased) = changelog
        .split_once("## [Unreleased]")
        .expect("the changelog has an unreleased section");

    assert!(
        unreleased.contains("divergence-iteration-on-object"),
        "the retired ledger row belongs in the changelog entry"
    );
    assert!(
        unreleased.contains("`.[]?`"),
        "the object-iteration entry must name the suppressing form"
    );
}
