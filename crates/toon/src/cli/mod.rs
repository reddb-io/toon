//! The `toon` binary: a thin argv adapter over the canonical event-stream
//! codec, implementing the upstream `@toon-format/cli` contract so upstream
//! scripts run unmodified. `tq` keeps its jq-faithful flag vocabulary; this
//! front-end keeps upstream's.
//!
//! `crates/toon/src/bin/toon.rs` binds this module to the process. The
//! TypeScript twin is `packages/toon/src/cli/run.ts`, and the shared corpus
//! under `tests/golden/toon-cli/` asserts the two front-ends agree byte for
//! byte.

mod args;
mod conversion;
mod errors;
mod io;
mod json_from_events;
mod token_stats;

use std::process::ExitCode;

pub use args::{parse_cli_args, ParsedArgs, HELP_TEXT};
pub use errors::CliError;
pub use io::{CliIo, InputSource, ProcessIo};
pub use token_stats::{estimate_token_count, format_statistics};

use conversion::{decode_to_json, encode_to_toon, Conversion};

/// The version the `--version` flag reports.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Runs the CLI against the real process and returns its exit status.
pub fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut io = ProcessIo::new();
    ExitCode::from(run_cli(&argv, &mut io))
}

/// Runs one CLI invocation and returns the process exit code.
pub fn run_cli(argv: &[String], io: &mut dyn CliIo) -> u8 {
    let mut verbose = false;
    match run(argv, io, &mut verbose) {
        Ok(()) => 0,
        Err(error) => {
            io.stderr(&format!("✖ {}\n", error.report(verbose)));
            1
        }
    }
}

fn run(argv: &[String], io: &mut dyn CliIo, verbose: &mut bool) -> Result<(), CliError> {
    let args = parse_cli_args(argv)?;
    *verbose = args.verbose;

    if args.help {
        io.stdout(HELP_TEXT);
        return Ok(());
    }
    if args.version {
        io.stdout(&format!("{VERSION}\n"));
        return Ok(());
    }

    let input = match args.input.as_deref() {
        None | Some("-") | Some("") => InputSource::Stdin,
        Some(path) => InputSource::File(io.cwd().join(path)),
    };
    let output = args
        .output
        .as_deref()
        .filter(|path| !path.is_empty())
        .map(|path| io.cwd().join(path));

    let indent_size = parse_indent(&args.indent)
        .ok_or_else(|| CliError::new(format!("Invalid indent value: {}", args.indent)))?;
    let delimiter = resolve_delimiter(&args.delimiter)?;

    let config = Conversion {
        input,
        output,
        indent_size,
    };

    match detect_mode(&config.input, args.encode, args.decode) {
        Mode::Encode => encode_to_toon(&config, delimiter, args.stats, io),
        Mode::Decode => decode_to_json(&config, args.strict, io),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Encode,
    Decode,
}

/// Upstream detects the mode from the file extension and defaults to encode,
/// which is what a bare `cat data.json | toon` relies on.
fn detect_mode(input: &InputSource, encode_flag: bool, decode_flag: bool) -> Mode {
    if encode_flag {
        return Mode::Encode;
    }
    if decode_flag {
        return Mode::Decode;
    }
    if let InputSource::File(path) = input {
        let name = path.to_string_lossy();
        if name.ends_with(".json") {
            return Mode::Encode;
        }
        if name.ends_with(".toon") {
            return Mode::Decode;
        }
    }
    Mode::Encode
}

/// `Number.parseInt(value || '2', 10)` with upstream's `< 0` rejection: a
/// leading integer wins, trailing text is ignored, and text with no leading
/// integer at all is the `NaN` upstream refuses.
fn parse_indent(value: &str) -> Option<usize> {
    if value.is_empty() {
        return Some(2);
    }

    let text = value.trim_start();
    let (negative, digits) = match text.strip_prefix(['-', '+']) {
        Some(rest) => (text.starts_with('-'), rest),
        None => (false, text),
    };
    let end = digits
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(digits.len());
    if end == 0 {
        return None;
    }
    let magnitude: usize = digits[..end].parse().ok()?;
    (!negative || magnitude == 0).then_some(magnitude)
}

/// Accepts the upstream literals plus the readable names `tq` already takes.
fn resolve_delimiter(value: &str) -> Result<char, CliError> {
    match value {
        "" | "comma" | "," => Ok(','),
        "tab" | "\\t" | "\t" => Ok('\t'),
        "pipe" | "|" => Ok('|'),
        _ => Err(CliError::new(format!(
            "Invalid delimiter {}. Valid delimiters are: comma (,), tab (\\t), pipe (|)",
            serde_json::to_string(value).unwrap_or_else(|_| format!("\"{value}\"")),
        ))),
    }
}
