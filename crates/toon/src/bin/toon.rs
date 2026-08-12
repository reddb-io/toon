//! The `toon` binary. Upstream `@toon-format/cli` scripts run against it
//! unmodified; every byte of its behaviour lives in `reddb_io_toon::cli`, so
//! a test can drive the same contract in-process.

fn main() -> std::process::ExitCode {
    reddb_io_toon::cli::main()
}
