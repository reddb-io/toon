//! The `toon` bin against the shared cross-language CLI corpus.
//!
//! `tests/golden/toon-cli/` pins one invocation per directory: the argv, the
//! stdin, the files in the working directory, and the exact stdout, stderr,
//! exit code, and written files it must produce. `packages/toon/test/
//! cli-golden.test.mjs` drives the same corpus through the TypeScript bin, so
//! a case that passes on both sides is byte parity between the two
//! front-ends — the contract Spec #359 asks for.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

struct Case {
    name: String,
    args: Vec<String>,
    stdin: String,
    files: Vec<(String, Vec<u8>)>,
    stdout: String,
    stderr: String,
    exit_code: i32,
    outputs: Vec<(String, Vec<u8>)>,
}

#[test]
fn golden_cli_cases_match_byte_for_byte() {
    for path in case_dirs() {
        let case = read_case(&path);
        let workspace = TempWorkspace::new(&case);
        let output = run_toon(&case, workspace.path());
        let name = &case.name;

        assert_eq!(
            String::from_utf8(output.stdout).expect("stdout is utf-8"),
            case.stdout,
            "{name} stdout"
        );
        assert_eq!(
            String::from_utf8(output.stderr).expect("stderr is utf-8"),
            case.stderr,
            "{name} stderr"
        );
        assert_eq!(
            output.status.code(),
            Some(case.exit_code),
            "{name} exit code"
        );

        for (file, expected) in &case.outputs {
            let written = fs::read(workspace.path().join(file))
                .unwrap_or_else(|error| panic!("{name} did not write {file}: {error}"));
            assert_eq!(&written, expected, "{name} wrote {file}");
        }
    }
}

/// The corpus only earns its keep while both front-ends actually read it, so a
/// case the TypeScript runner cannot see is a case that proves nothing.
#[test]
fn every_case_is_visible_to_the_typescript_runner() {
    let runner = fs::read_to_string(repo_path("packages/toon/test/cli-golden.test.mjs"))
        .expect("the TypeScript golden runner exists");

    assert!(
        runner.contains("tests/golden/toon-cli"),
        "the TypeScript runner must read the shared corpus directory"
    );
}

/// `cargo install reddb-io-toon` has to yield a binary named `toon`, which is
/// a manifest claim rather than a runtime one — so the manifest is what the
/// test reads.
#[test]
fn the_publishing_crate_declares_the_toon_binary() {
    let manifest = fs::read_to_string(repo_path("crates/toon/Cargo.toml"))
        .expect("the crate manifest is readable");

    assert!(
        manifest.contains("[[bin]]") && manifest.contains("name = \"toon\""),
        "the publishing crate must declare the `toon` bin: {manifest}"
    );
    assert!(
        manifest.contains("name = \"reddb-io-toon\""),
        "the crate name stays reddb-io-*"
    );
}

#[test]
fn version_reports_the_crate_version() {
    let output = run_toon_in(&["--version".to_owned()], "", Path::new("."));

    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout is utf-8"),
        format!("{}\n", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(output.status.code(), Some(0));
}

/// A file the CLI cannot read is a failure the user can act on, not a panic.
#[test]
fn a_missing_input_file_fails_with_a_clean_message() {
    let output = run_toon_in(&["nowhere.json".to_owned()], "", Path::new("."));
    let stderr = String::from_utf8(output.stderr).expect("stderr is utf-8");

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty(), "{stderr}");
    assert!(stderr.starts_with("✖ Failed to read `"), "{stderr}");
    assert!(stderr.contains("nowhere.json"), "{stderr}");
}

/// `--verbose` is the one flag whose output cannot be pinned cross-language:
/// upstream appends a JavaScript stack. Rust carries the cause chain instead,
/// so the corpus leaves it out and this case pins the Rust half.
#[test]
fn verbose_appends_the_cause_chain() {
    let plain = run_toon_in(&[], "{oops", Path::new("."));
    let verbose = run_toon_in(&["--verbose".to_owned()], "{oops", Path::new("."));

    let plain_stderr = String::from_utf8(plain.stderr).expect("stderr is utf-8");
    let verbose_stderr = String::from_utf8(verbose.stderr).expect("stderr is utf-8");

    assert_eq!(plain.status.code(), Some(1));
    assert!(
        plain_stderr.starts_with("✖ Failed to parse JSON: "),
        "{plain_stderr}"
    );
    assert!(!plain_stderr.contains("Caused by:"), "{plain_stderr}");
    assert!(verbose_stderr.contains("Caused by: "), "{verbose_stderr}");
}

/// Strict decoding must not silently substitute U+FFFD, and it has to say so
/// in the sentence upstream uses.
#[test]
fn strict_decoding_rejects_ill_formed_bytes() {
    let workspace = TempWorkspace::empty("strict-utf8");
    fs::write(workspace.path().join("input.toon"), b"name: \xff\n").expect("write fixture");

    let strict = run_toon_in(&["input.toon".to_owned()], "", workspace.path());
    let lenient = run_toon_in(
        &["input.toon".to_owned(), "--no-strict".to_owned()],
        "",
        workspace.path(),
    );

    let stderr = String::from_utf8(strict.stderr).expect("stderr is utf-8");
    assert_eq!(strict.status.code(), Some(1));
    assert_eq!(
        stderr,
        "✖ Input is not valid UTF-8. Pass --no-strict to replace ill-formed bytes\n"
    );

    assert_eq!(lenient.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(lenient.stdout).expect("stdout is utf-8"),
        "{\n  \"name\": \"\u{FFFD}\"\n}\n"
    );
}

/// A document longer than the reporting window still fails cleanly: the header
/// stays, and only the quoted source line drops away.
#[test]
fn a_failure_past_the_line_window_reports_its_header_alone() {
    let mut document = String::from("rows[200]{id}:\n");
    for id in 0..199 {
        document.push_str(&format!("  {id}\n"));
    }
    document.push_str("  oops: 1\n");

    let output = run_toon_in(&["-d".to_owned()], &document, Path::new("."));
    let stderr = String::from_utf8(output.stderr).expect("stderr is utf-8");

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr.starts_with("✖ Failed to decode TOON at line "),
        "{stderr}"
    );
}

/// A document larger than one write batch still reaches stdout whole.
#[test]
fn a_document_past_the_write_batch_is_written_whole() {
    let rows: Vec<String> = (0..8_000).map(|id| format!("  {id},row-{id}")).collect();
    let document = format!("rows[8000]{{id,name}}:\n{}\n", rows.join("\n"));

    let output = run_toon_in(&["-d".to_owned()], &document, Path::new("."));
    let stdout = String::from_utf8(output.stdout).expect("stdout is utf-8");

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout.len() > 64 * 1024, "the case must exceed one batch");
    assert!(
        stdout.ends_with("\"name\": \"row-7999\"\n    }\n  ]\n}\n"),
        "{}",
        &stdout[stdout.len() - 80..]
    );
}

fn case_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(repo_path("tests/golden/toon-cli"))
        .expect("the shared CLI corpus exists")
        .map(|entry| entry.expect("corpus entry is readable").path())
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();

    assert!(
        !dirs.is_empty(),
        "the shared CLI corpus should not be empty"
    );
    dirs
}

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn read_case(path: &Path) -> Case {
    let name = path
        .file_name()
        .expect("corpus case has a name")
        .to_string_lossy()
        .into_owned();

    Case {
        args: read_args(&path.join("args.txt")),
        stdin: fs::read_to_string(path.join("stdin.txt")).unwrap_or_default(),
        files: read_directory(&path.join("files")),
        stdout: read_text(&path.join("stdout.txt"), &name, "stdout.txt"),
        stderr: read_text(&path.join("stderr.txt"), &name, "stderr.txt"),
        exit_code: read_text(&path.join("exit.txt"), &name, "exit.txt")
            .trim()
            .parse()
            .expect("exit fixture is an integer"),
        outputs: read_directory(&path.join("output")),
        name,
    }
}

/// One argument per line, so an argument may hold spaces. The trailing newline
/// the file ends with is a terminator, not an empty final argument.
fn read_args(path: &Path) -> Vec<String> {
    let text = fs::read_to_string(path).expect("args fixture exists");
    text.strip_suffix('\n')
        .unwrap_or(&text)
        .split('\n')
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn read_text(path: &Path, case: &str, fixture: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("{case} needs {fixture}: {error}"))
}

fn read_directory(path: &Path) -> Vec<(String, Vec<u8>)> {
    let Ok(entries) = fs::read_dir(path) else {
        return Vec::new();
    };
    let mut files: Vec<(String, Vec<u8>)> = entries
        .map(|entry| entry.expect("corpus file is readable").path())
        .filter(|path| path.is_file())
        .map(|path| {
            (
                path.file_name()
                    .expect("corpus file has a name")
                    .to_string_lossy()
                    .into_owned(),
                fs::read(&path).expect("corpus file is readable"),
            )
        })
        .collect();
    files.sort();
    files
}

/// A working directory of its own per case, so `--output` writes and relative
/// labels are real without a case reaching into the repository.
struct TempWorkspace {
    path: PathBuf,
}

impl TempWorkspace {
    fn new(case: &Case) -> Self {
        let workspace = Self::empty(&case.name);
        for (name, content) in &case.files {
            fs::write(workspace.path().join(name), content).expect("write case fixture");
        }
        workspace
    }

    fn empty(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("toon-cli-golden-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create case working directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn run_toon(case: &Case, cwd: &Path) -> Output {
    run_toon_in(&case.args, &case.stdin, cwd)
}

fn run_toon_in(args: &[String], stdin: &str, cwd: &Path) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_toon"))
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn toon");

    // Feeding stdin from its own thread keeps a document larger than a pipe
    // buffer from deadlocking against the stdout this side has not read yet.
    let mut pipe = child.stdin.take().expect("stdin is piped");
    let document = stdin.as_bytes().to_vec();
    let feeder = std::thread::spawn(move || {
        if let Err(error) = pipe.write_all(&document) {
            // A run that fails on argv alone exits before reading stdin; the
            // resulting broken pipe is not a test failure.
            assert_eq!(error.kind(), io::ErrorKind::BrokenPipe, "write stdin");
        }
    });

    let output = child.wait_with_output().expect("wait for toon");
    feeder.join().expect("stdin feeder finished");
    output
}
