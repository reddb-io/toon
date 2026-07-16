//! Corpus sweep: every test file the repo carries, fed through the `tq` binary.
//!
//! golden.rs pins exact stdout/stderr for hand-picked scenarios and behavior.rs
//! checks builtins against jq. Both ask "is the answer right?" about inputs
//! chosen by hand. This runner asks a cheaper question across a far wider net:
//! over every corpus file in the repo, does tq ever hang, panic, or die on a
//! signal — and do valid documents still survive a round trip?
//!
//! The distinction that keeps the sweep honest: a clean error is a perfectly
//! good outcome, because much of the corpus is malformed on purpose. A panic, a
//! signal death or a hang never is. So the universal assertion is "failed
//! cleanly or not at all", and exit codes are only pinned where the corpus says
//! what the answer should be.

use serde_json::Value as Json;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Long enough that a loaded CI runner never trips it, short enough that a real
/// hang is reported as a hang instead of stalling until the harness gives up.
const PER_FILE_TIMEOUT: Duration = Duration::from_secs(30);

/// Rust turns a panic into exit code 101; a process killed by a signal reports
/// no code at all. Both mean tq came apart rather than failing on purpose.
const PANIC_EXIT_CODE: i32 = 101;

static SCRATCH_ID: AtomicUsize = AtomicUsize::new(0);

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The spec fixtures live in a submodule. A checkout without it is a missing
/// signal, not a failure — this suite is also expected to run in shallow trees.
fn spec_fixture_root() -> Option<PathBuf> {
    let root = repo_root().join("vendor/toon-spec/tests/fixtures");
    if root.is_dir() {
        return Some(root);
    }
    eprintln!(
        "warning: {} is missing — skipping the official spec fixtures. \
         Run `git submodule update --init` to cover them.",
        root.display()
    );
    None
}

fn scratch_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tq-corpus.{}", std::process::id()));
    fs::create_dir_all(&dir).expect("create scratch dir");
    let id = SCRATCH_ID.fetch_add(1, Ordering::Relaxed);
    dir.join(format!("{id}.{name}"))
}

#[derive(Debug)]
struct Run {
    /// `None` when a signal killed tq before it could choose an exit code.
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Run {
    fn succeeded(&self) -> bool {
        self.code == Some(0)
    }

    /// The assertion every invocation in this file gets, whatever the input.
    /// Exiting non-zero with a diagnostic is fine; coming apart is not.
    fn assert_failed_cleanly_or_not_at_all(&self, context: &str) {
        assert!(
            !self.stderr.contains("panicked at"),
            "{context}: tq panicked\n{}",
            self.stderr
        );
        assert_ne!(
            self.code,
            Some(PANIC_EXIT_CODE),
            "{context}: tq exited {PANIC_EXIT_CODE} (panic)\n{}",
            self.stderr
        );
        assert!(
            self.code.is_some(),
            "{context}: tq died on a signal\n{}",
            self.stderr
        );
    }
}

/// Runs tq with a hard deadline, capturing output through files rather than
/// pipes: a pipe that fills up while nobody drains it is itself a hang, and
/// this runner must be able to tell tq's hangs from its own.
fn run_tq(args: &[&str]) -> Run {
    let stdout_path = scratch_path("stdout");
    let stderr_path = scratch_path("stderr");

    let mut child = Command::new(env!("CARGO_BIN_EXE_tq"))
        .args(args)
        // Every invocation here names its input file, so an empty stdin means a
        // stray read ends at EOF instead of blocking on the test runner's stdin.
        .stdin(Stdio::null())
        .stdout(File::create(&stdout_path).expect("create stdout capture"))
        .stderr(File::create(&stderr_path).expect("create stderr capture"))
        .spawn()
        .expect("spawn tq");

    let deadline = Instant::now() + PER_FILE_TIMEOUT;
    let status = loop {
        match child.try_wait().expect("poll tq") {
            Some(status) => break status,
            None => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!(
                        "tq did not finish within {PER_FILE_TIMEOUT:?}: tq {}",
                        args.join(" ")
                    );
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    };

    let run = Run {
        code: status.code(),
        stdout: fs::read_to_string(&stdout_path).unwrap_or_default(),
        stderr: fs::read_to_string(&stderr_path).unwrap_or_default(),
    };
    let _ = fs::remove_file(&stdout_path);
    let _ = fs::remove_file(&stderr_path);
    run
}

fn write_scratch(name: &str, contents: &str) -> PathBuf {
    let path = scratch_path(name);
    fs::write(&path, contents).expect("write scratch input");
    path
}

fn files_under(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
        for entry in entries {
            let path = entry.expect("directory entry").path();
            if path.is_dir() {
                walk(&path, out);
            } else {
                out.push(path);
            }
        }
    }

    let mut files = Vec::new();
    walk(root, &mut files);
    files.sort();
    files
}

fn files_with_extension(root: &Path, extension: &str) -> Vec<PathBuf> {
    files_under(root)
        .into_iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == extension))
        .collect()
}

/// Every JSON document the repo keeps as test data, wherever it lives.
fn json_corpus_files() -> Vec<PathBuf> {
    let mut files = files_with_extension(&repo_root().join("tests/corpus"), "json");
    if let Some(spec_root) = spec_fixture_root() {
        files.extend(files_with_extension(&spec_root, "json"));
    }
    files
}

fn label(path: &Path) -> String {
    let root = repo_root().canonicalize().expect("canonical repo root");
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    path.strip_prefix(&root)
        .unwrap_or(&path)
        .display()
        .to_string()
}

// ---------------------------------------------------------------------------
// Whole-file sweeps
// ---------------------------------------------------------------------------

/// Every JSON corpus file is a valid document, so tq must read all of them, and
/// a JSON → TOON → JSON trip must give the same value back.
///
/// Value, not bytes: TOON's tabular form writes each row in the header's field
/// order, so `{"c":30,"b":20,"a":10}` under a `{a,b,c}` header comes back as
/// `{"a":10,"b":20,"c":30}`. That reordering is what the spec asks for (see the
/// official encode/arrays-objects fixture), and JSON object key order carries no
/// meaning, so the contract is the decoded value.
#[test]
fn every_json_corpus_file_round_trips_through_toon() {
    let files = json_corpus_files();
    assert!(!files.is_empty(), "the JSON corpus should not be empty");

    for path in files {
        let name = label(&path);
        let file = path.to_str().expect("corpus path is utf-8");

        let to_json = run_tq(&["-p", "json", "-o", "json", "-c", ".", file]);
        to_json.assert_failed_cleanly_or_not_at_all(&name);
        assert!(
            to_json.succeeded(),
            "{name}: tq could not read a valid JSON corpus file\n{}",
            to_json.stderr
        );

        let to_toon = run_tq(&["-p", "json", "-o", "toon", ".", file]);
        to_toon.assert_failed_cleanly_or_not_at_all(&name);
        assert!(
            to_toon.succeeded(),
            "{name}: tq could not encode a valid JSON corpus file to TOON\n{}",
            to_toon.stderr
        );

        let encoded = write_scratch("round-trip.toon", &to_toon.stdout);
        let back = run_tq(&[
            "-p",
            "toon",
            "-o",
            "json",
            "-c",
            ".",
            encoded.to_str().expect("scratch path is utf-8"),
        ]);
        back.assert_failed_cleanly_or_not_at_all(&name);
        assert!(
            back.succeeded(),
            "{name}: tq could not read back the TOON it just wrote\n{}",
            back.stderr
        );
        let _ = fs::remove_file(&encoded);

        assert_eq!(
            parse_json(&back.stdout, &name),
            parse_json(&to_json.stdout, &name),
            "{name}: JSON → TOON → JSON changed the value"
        );
    }
}

/// Corpus files the YAML reader cannot read but the JSON reader can.
///
/// `tq -p yaml` rejects integers above u64::MAX — `{"v":18446744073709551616}`
/// fails with "JSON number out of range" while `-p json` reads it as 1e+20.
/// JSON is a subset of YAML, so a JSON document the JSON reader accepts should
/// never fail the YAML reader; this is a divergence in tq (serde_norway's number
/// handling), reported rather than fixed here, since #195 is a test slice and
/// changing tq's behaviour is out of its scope.
///
/// A ratchet, like tests/runners/rust/toon/expected-failures.txt: an entry that
/// starts passing is a stale entry, and the test says so instead of going quiet.
const YAML_READER_DIVERGENCES: &[&str] =
    &["vendor/toon-spec/tests/fixtures/encode/primitives.json"];

/// JSON is a subset of YAML, so the YAML reader must agree with the JSON reader
/// on every JSON corpus file — which is what makes the `-p yaml` path testable
/// without a YAML corpus of its own.
#[test]
fn the_yaml_reader_agrees_with_the_json_reader_on_the_json_corpus() {
    for path in json_corpus_files() {
        let name = label(&path);
        let file = path.to_str().expect("corpus path is utf-8");

        let as_yaml = run_tq(&["-p", "yaml", "-o", "json", "-c", ".", file]);
        as_yaml.assert_failed_cleanly_or_not_at_all(&name);

        if YAML_READER_DIVERGENCES.contains(&name.as_str()) {
            assert!(
                !as_yaml.succeeded(),
                "{name}: the YAML reader now reads this file — \
                 remove it from YAML_READER_DIVERGENCES"
            );
            continue;
        }

        assert!(
            as_yaml.succeeded(),
            "{name}: the YAML reader rejected a JSON document\n{}",
            as_yaml.stderr
        );

        let as_json = run_tq(&["-p", "json", "-o", "json", "-c", ".", file]);
        assert_eq!(
            parse_json(&as_yaml.stdout, &name),
            parse_json(&as_json.stdout, &name),
            "{name}: the YAML and JSON readers disagree"
        );
    }
}

/// The golden corpus is deliberately half-malformed — depth bombs, length
/// mismatches, truncated documents. golden.rs owns what each one should print;
/// this only asks that none of them take tq down, whichever way they exit.
#[test]
fn golden_case_files_never_take_the_cli_down() {
    let root = repo_root().join("tests/golden");
    let files = files_under(&root);
    assert!(!files.is_empty(), "the golden corpus should not be empty");

    for path in files {
        let name = label(&path);
        let file = path.to_str().expect("golden path is utf-8");
        // args.txt/exit.txt/stderr.txt are not documents. Feeding them to the
        // parser anyway is the point: arbitrary bytes must not panic either.
        let format = match path.extension().and_then(|ext| ext.to_str()) {
            Some("json") => "json",
            Some("toonl") => "toonl",
            _ => "toon",
        };

        for output in ["json", "toon", "toonl"] {
            let run = run_tq(&["-p", format, "-o", output, ".", file]);
            run.assert_failed_cleanly_or_not_at_all(&format!("{name} (-p {format} -o {output})"));
        }

        if format != "json" {
            let checked = run_tq(&["check", "-p", format, file]);
            checked.assert_failed_cleanly_or_not_at_all(&format!("{name} (check -p {format})"));
        }
    }
}

// ---------------------------------------------------------------------------
// Official spec fixture cases
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct FixtureCase {
    name: String,
    input: Json,
    should_error: bool,
    /// Cases naming decoder/encoder options the CLI does not expose (strict,
    /// indent, expandPaths, …). tq is never asked to honour them, so its exit
    /// code says nothing about the case and only the crash guard applies.
    uses_options: bool,
}

fn read_fixture_cases(path: &Path) -> Vec<FixtureCase> {
    let source = fs::read_to_string(path).expect("read fixture");
    let fixture: Json = serde_json::from_str(&source).expect("fixture is valid JSON");
    fixture
        .get("tests")
        .and_then(Json::as_array)
        .expect("fixture has tests")
        .iter()
        .map(|test| FixtureCase {
            name: test
                .get("name")
                .and_then(Json::as_str)
                .expect("case has a name")
                .to_string(),
            input: test.get("input").cloned().unwrap_or(Json::Null),
            should_error: test
                .get("shouldError")
                .and_then(Json::as_bool)
                .unwrap_or(false),
            uses_options: test.get("options").is_some(),
        })
        .collect()
}

/// The decode fixtures carry TOON snippets, and roughly a sixth of them are
/// malformed on purpose. Every one goes through the CLI twice — once to parse,
/// once through `check` — because those are separate entry points into the
/// decoder and either could be the one that panics.
#[test]
fn official_decode_fixture_cases_survive_the_cli() {
    let Some(spec_root) = spec_fixture_root() else {
        return;
    };
    let files = files_with_extension(&spec_root.join("decode"), "json");
    assert!(!files.is_empty(), "the decode fixtures should not be empty");

    for path in files {
        for case in read_fixture_cases(&path) {
            let Some(input) = case.input.as_str() else {
                continue;
            };
            let context = format!("{}::{}", label(&path), case.name);
            let file = write_scratch("case.toon", input);
            let file = file.to_str().expect("scratch path is utf-8");

            let parsed = run_tq(&["-p", "toon", "-o", "json", "-c", ".", file]);
            parsed.assert_failed_cleanly_or_not_at_all(&context);
            let checked = run_tq(&["check", "-p", "toon", file]);
            checked.assert_failed_cleanly_or_not_at_all(&context);

            if case.uses_options {
                continue;
            }
            if case.should_error {
                assert!(
                    !parsed.succeeded(),
                    "{context}: a malformed document decoded without error"
                );
                assert!(
                    !parsed.stderr.is_empty(),
                    "{context}: a malformed document failed without a diagnostic"
                );
                assert!(
                    !checked.succeeded(),
                    "{context}: `tq check` passed a malformed document"
                );
            } else {
                assert!(
                    parsed.succeeded(),
                    "{context}: a valid document failed to decode\n{}",
                    parsed.stderr
                );
                assert!(
                    checked.succeeded(),
                    "{context}: `tq check` rejected a valid document\n{}",
                    checked.stderr
                );
            }
        }
    }
}

/// The encode fixtures carry JSON values plus the TOON they should produce.
/// The CLI has to encode every one of them, and read its own output back.
#[test]
fn official_encode_fixture_cases_survive_the_cli() {
    let Some(spec_root) = spec_fixture_root() else {
        return;
    };
    let files = files_with_extension(&spec_root.join("encode"), "json");
    assert!(!files.is_empty(), "the encode fixtures should not be empty");

    for path in files {
        for case in read_fixture_cases(&path) {
            let context = format!("{}::{}", label(&path), case.name);
            let source = serde_json::to_string(&case.input).expect("case input is serialisable");
            let file = write_scratch("case.json", &source);
            let file = file.to_str().expect("scratch path is utf-8");

            let encoded = run_tq(&["-p", "json", "-o", "toon", ".", file]);
            encoded.assert_failed_cleanly_or_not_at_all(&context);
            if case.uses_options {
                continue;
            }
            assert!(
                encoded.succeeded(),
                "{context}: tq could not encode a valid JSON value\n{}",
                encoded.stderr
            );

            // The baseline is tq reading the case itself, not this test's own
            // JSON parse: TOON normalises some values on the way in (-0 becomes
            // 0, per the spec's own "encodes negative zero as zero" case), and
            // that normalisation is not round-trip damage. Comparing tq against
            // tq keeps the shared normalisations out of the assertion and still
            // catches a trip that actually loses or changes data.
            let baseline = run_tq(&["-p", "json", "-o", "json", "-c", ".", file]);
            assert!(
                baseline.succeeded(),
                "{context}: tq could not read a valid JSON value\n{}",
                baseline.stderr
            );

            // Whatever tq wrote, tq must be able to read — the round trip is
            // the only claim that holds without reimplementing the encoder.
            let written = write_scratch("encoded.toon", &encoded.stdout);
            let back = run_tq(&[
                "-p",
                "toon",
                "-o",
                "json",
                "-c",
                ".",
                written.to_str().expect("scratch path is utf-8"),
            ]);
            back.assert_failed_cleanly_or_not_at_all(&context);
            assert!(
                back.succeeded(),
                "{context}: tq could not read back the TOON it just wrote\n{}",
                back.stderr
            );
            assert_eq!(
                parse_json(&back.stdout, &context),
                parse_json(&baseline.stdout, &context),
                "{context}: JSON → TOON → JSON changed the value"
            );
        }
    }
}

fn parse_json(source: &str, context: &str) -> Json {
    serde_json::from_str(source)
        .unwrap_or_else(|error| panic!("{context}: tq emitted invalid JSON ({error}): {source}"))
}
