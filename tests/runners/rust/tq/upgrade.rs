//! `tq upgrade` end to end, against a release served by a local HTTP server.
//!
//! Nothing here touches github.com: `TQ_API_BASE` and `TQ_DOWNLOAD_BASE` point
//! the whole flow at a throwaway listener, and the binary under test is a copy
//! in a scratch directory, so it upgrades itself without disturbing the build.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::Arc;
use std::thread;

/// Comfortably above any version this crate will ever carry, so the release the
/// server serves always reads as an update.
const NEWER: &str = "999.1.0";
const NEW_BINARY: &[u8] = b"#!/bin/sh\necho 'tq 999.1.0'\n";

// --- the flows the acceptance criteria name ---------------------------------

#[test]
fn check_reports_an_available_update_and_exits_nonzero() {
    let server = Server::start(routes(NEWER, NEW_BINARY, Sums::Correct));
    let scratch = Scratch::new("check-outdated");

    let output = scratch.upgrade(&["--check"], &server, &[]);

    assert_eq!(output.status.code(), Some(1), "{}", output.describe());
    let stdout = output.stdout_utf8();
    assert!(stdout.contains(current_version()), "{stdout}");
    assert!(stdout.contains(NEWER), "{stdout}");
    assert!(stdout.contains("channel stable"), "{stdout}");
    assert!(stdout.contains("update available"), "{stdout}");
}

#[test]
fn check_reports_up_to_date_and_exits_zero() {
    let server = Server::start(routes(current_version(), NEW_BINARY, Sums::Correct));
    let scratch = Scratch::new("check-current");

    let output = scratch.upgrade(&["--check"], &server, &[]);

    assert_eq!(output.status.code(), Some(0), "{}", output.describe());
    assert!(
        output.stdout_utf8().contains("up to date"),
        "{}",
        output.describe()
    );
    // --check never writes, however far behind the binary is.
    assert_eq!(scratch.binary_bytes(), original_binary_bytes());
}

#[test]
fn upgrade_replaces_the_running_binary_after_verifying_the_checksum() {
    let server = Server::start(routes(NEWER, NEW_BINARY, Sums::Correct));
    let scratch = Scratch::new("replace");

    let output = scratch.upgrade(&[], &server, &[]);

    assert_eq!(output.status.code(), Some(0), "{}", output.describe());
    let stdout = output.stdout_utf8();
    assert!(
        stdout.contains(&format!("Updated tq {} -> {NEWER}", current_version())),
        "{stdout}"
    );
    assert_eq!(scratch.binary_bytes(), NEW_BINARY);
    // Replaced in place, and still executable.
    assert!(scratch.binary_mode() & 0o111 != 0);
    // The staging file never survives a successful upgrade.
    assert!(scratch.leftovers().is_empty(), "{:?}", scratch.leftovers());
}

#[test]
fn upgrade_is_a_no_op_when_the_latest_release_is_the_running_version() {
    let server = Server::start(routes(current_version(), NEW_BINARY, Sums::Correct));
    let scratch = Scratch::new("noop");

    let output = scratch.upgrade(&[], &server, &[]);

    assert_eq!(output.status.code(), Some(0), "{}", output.describe());
    assert!(
        output.stdout_utf8().contains("already up to date"),
        "{}",
        output.describe()
    );
    assert_eq!(scratch.binary_bytes(), original_binary_bytes());
    // An up-to-date tq resolves the release and stops: it never downloads.
    assert_eq!(
        server.hits(&format!(
            "/dl/v{}/tq-linux-x86_64-static",
            current_version()
        )),
        0
    );
}

#[test]
fn upgrade_refuses_a_binary_whose_checksum_does_not_match() {
    let server = Server::start(routes(NEWER, NEW_BINARY, Sums::Wrong));
    let scratch = Scratch::new("bad-checksum");

    let output = scratch.upgrade(&[], &server, &[]);

    assert_eq!(output.status.code(), Some(1), "{}", output.describe());
    assert!(
        output
            .stderr_utf8()
            .contains("checksum verification failed"),
        "{}",
        output.describe()
    );
    assert_eq!(scratch.binary_bytes(), original_binary_bytes());
}

#[test]
fn upgrade_fails_hard_when_the_release_has_no_checksum_for_the_asset() {
    let server = Server::start(routes(NEWER, NEW_BINARY, Sums::MissingEntry));
    let scratch = Scratch::new("no-checksum-entry");

    let output = scratch.upgrade(&[], &server, &[]);

    assert_eq!(output.status.code(), Some(1), "{}", output.describe());
    assert!(
        output
            .stderr_utf8()
            .contains("no checksum for tq-linux-x86_64-static in SHA256SUMS"),
        "{}",
        output.describe()
    );
    assert_eq!(scratch.binary_bytes(), original_binary_bytes());
}

#[test]
fn upgrade_fails_hard_when_the_release_publishes_no_checksums_at_all() {
    let server = Server::start(routes(NEWER, NEW_BINARY, Sums::Absent));
    let scratch = Scratch::new("no-sums-asset");

    let output = scratch.upgrade(&[], &server, &[]);

    assert_eq!(output.status.code(), Some(1), "{}", output.describe());
    assert!(
        output.stderr_utf8().contains("publishes no SHA256SUMS"),
        "{}",
        output.describe()
    );
    assert_eq!(scratch.binary_bytes(), original_binary_bytes());
}

// --- knob parity with the installer -----------------------------------------

#[test]
fn next_channel_reads_the_release_list_instead_of_latest() {
    let server = Server::start(routes(NEWER, NEW_BINARY, Sums::Correct));
    let scratch = Scratch::new("channel-next");

    let output = scratch.upgrade(&["--check"], &server, &[("TQ_CHANNEL", "next")]);

    assert_eq!(output.status.code(), Some(1), "{}", output.describe());
    assert!(
        output.stdout_utf8().contains("channel next"),
        "{}",
        output.describe()
    );
    assert_eq!(server.hits("/api/releases?per_page=1"), 1);
    assert_eq!(server.hits("/api/releases/latest"), 0);
}

#[test]
fn stable_falls_back_to_the_release_list_when_no_stable_release_exists() {
    // `/releases/latest` 404s while a repository has only prereleases, exactly
    // as it does for install.sh.
    let mut routes = routes(NEWER, NEW_BINARY, Sums::Correct);
    routes.insert("/api/releases/latest".to_owned(), Route::status(404));
    let server = Server::start(routes);
    let scratch = Scratch::new("stable-fallback");

    let output = scratch.upgrade(&["--check"], &server, &[]);

    assert_eq!(output.status.code(), Some(1), "{}", output.describe());
    assert!(
        output.stdout_utf8().contains(NEWER),
        "{}",
        output.describe()
    );
    assert_eq!(server.hits("/api/releases?per_page=1"), 1);
}

#[test]
fn an_unknown_channel_is_rejected_before_any_request() {
    let server = Server::start(routes(NEWER, NEW_BINARY, Sums::Correct));
    let scratch = Scratch::new("channel-bogus");

    let output = scratch.upgrade(&["--check"], &server, &[("TQ_CHANNEL", "beta")]);

    assert_eq!(output.status.code(), Some(1), "{}", output.describe());
    assert!(
        output
            .stderr_utf8()
            .contains("unsupported TQ_CHANNEL `beta`"),
        "{}",
        output.describe()
    );
    assert_eq!(server.hits("/api/releases/latest"), 0);
}

#[test]
fn a_version_argument_pins_the_release_by_tag() {
    let server = Server::start(routes(NEWER, NEW_BINARY, Sums::Correct));
    let scratch = Scratch::new("pin-argument");

    let output = scratch.upgrade(&[NEWER], &server, &[]);

    assert_eq!(output.status.code(), Some(0), "{}", output.describe());
    assert_eq!(scratch.binary_bytes(), NEW_BINARY);
    assert_eq!(server.hits(&format!("/api/releases/tags/v{NEWER}")), 1);
    assert_eq!(server.hits("/api/releases/latest"), 0);
}

#[test]
fn tq_version_pins_the_release_and_the_argument_wins_over_it() {
    let mut table = routes(NEWER, NEW_BINARY, Sums::Correct);
    // The release the argument pins to, so the override is observable.
    table.insert(
        format!("/api/releases/tags/v{}", current_version()),
        Route::json(release_json(
            current_version(),
            &["tq-linux-x86_64-static", "SHA256SUMS"],
        )),
    );
    let server = Server::start(table);
    let scratch = Scratch::new("pin-env");

    // The tag form of the pin is accepted too, matching TQ_VERSION=v0.1.0.
    let output = scratch.upgrade(
        &["--check"],
        &server,
        &[("TQ_VERSION", &format!("v{NEWER}"))],
    );
    assert_eq!(output.status.code(), Some(1), "{}", output.describe());
    assert_eq!(server.hits(&format!("/api/releases/tags/v{NEWER}")), 1);

    // An explicit argument overrides the environment.
    let output = scratch.upgrade(
        &["--check", current_version()],
        &server,
        &[("TQ_VERSION", &format!("v{NEWER}"))],
    );
    assert_eq!(output.status.code(), Some(0), "{}", output.describe());
    assert!(
        output.stdout_utf8().contains("up to date"),
        "{}",
        output.describe()
    );
    assert_eq!(
        server.hits(&format!("/api/releases/tags/v{}", current_version())),
        1
    );
}

#[test]
fn a_pin_to_an_older_release_downgrades_instead_of_reporting_up_to_date() {
    let server = Server::start(routes("0.0.1", NEW_BINARY, Sums::Correct));
    let scratch = Scratch::new("pin-downgrade");

    let output = scratch.upgrade(&["0.0.1"], &server, &[]);

    assert_eq!(output.status.code(), Some(0), "{}", output.describe());
    assert!(
        output
            .stdout_utf8()
            .contains(&format!("Updated tq {} -> 0.0.1", current_version())),
        "{}",
        output.describe()
    );
    assert_eq!(scratch.binary_bytes(), NEW_BINARY);
}

#[test]
fn a_missing_pinned_release_reports_the_tag_it_looked_for() {
    let server = Server::start(routes(NEWER, NEW_BINARY, Sums::Correct));
    let scratch = Scratch::new("pin-missing");

    let output = scratch.upgrade(&["1.2.3"], &server, &[]);

    assert_eq!(output.status.code(), Some(1), "{}", output.describe());
    assert!(
        output.stderr_utf8().contains("release v1.2.3 not found"),
        "{}",
        output.describe()
    );
}

#[test]
fn github_token_authorizes_the_api_and_downloads_through_the_asset_endpoint() {
    let mut routes = routes(NEWER, NEW_BINARY, Sums::Correct);
    // The token-only paths: with GITHUB_TOKEN set, assets come from the API by
    // id rather than from the public download host.
    routes.insert(
        "/api/releases/assets/11".to_owned(),
        Route::bytes(NEW_BINARY.to_vec()),
    );
    routes.insert(
        "/api/releases/assets/12".to_owned(),
        Route::bytes(sums_body(NEW_BINARY, Sums::Correct).into_bytes()),
    );
    let server = Server::start(routes);
    let scratch = Scratch::new("token");

    let output = scratch.upgrade(&[], &server, &[("GITHUB_TOKEN", "s3cret")]);

    assert_eq!(output.status.code(), Some(0), "{}", output.describe());
    assert_eq!(scratch.binary_bytes(), NEW_BINARY);
    assert_eq!(server.hits("/api/releases/assets/11"), 1);
    assert_eq!(
        server.hits(&format!("/dl/v{NEWER}/tq-linux-x86_64-static")),
        0
    );
    // Every request carried the bearer token.
    assert!(server
        .authorizations()
        .iter()
        .all(|value| value == "Bearer s3cret"));
    assert!(!server.authorizations().is_empty());
}

// --- asset selection ---------------------------------------------------------

#[test]
fn linux_falls_back_to_the_gnu_asset_when_the_static_one_is_absent() {
    let mut table = routes(NEWER, NEW_BINARY, Sums::Correct);
    table.insert(
        format!("/api/releases/tags/v{NEWER}"),
        Route::json(release_json(NEWER, &["tq-linux-x86_64", "SHA256SUMS"])),
    );
    table.insert(
        format!("/dl/v{NEWER}/tq-linux-x86_64"),
        Route::bytes(NEW_BINARY.to_vec()),
    );
    table.insert(
        format!("/dl/v{NEWER}/SHA256SUMS"),
        Route::bytes(format!("{}  tq-linux-x86_64\n", sha256_hex(NEW_BINARY)).into_bytes()),
    );
    let server = Server::start(table);
    let scratch = Scratch::new("gnu-fallback");

    let output = scratch.upgrade(&[NEWER], &server, &[]);

    assert_eq!(output.status.code(), Some(0), "{}", output.describe());
    assert_eq!(scratch.binary_bytes(), NEW_BINARY);
    assert_eq!(server.hits(&format!("/dl/v{NEWER}/tq-linux-x86_64")), 1);
}

#[test]
fn a_release_without_an_asset_for_this_platform_names_what_it_looked_for() {
    let mut table = routes(NEWER, NEW_BINARY, Sums::Correct);
    table.insert(
        format!("/api/releases/tags/v{NEWER}"),
        Route::json(release_json(NEWER, &["tq-macos-aarch64", "SHA256SUMS"])),
    );
    let server = Server::start(table);
    let scratch = Scratch::new("no-asset");

    let output = scratch.upgrade(&[NEWER], &server, &[]);

    assert_eq!(output.status.code(), Some(1), "{}", output.describe());
    let stderr = output.stderr_utf8();
    assert!(stderr.contains("no tq asset for"), "{stderr}");
    assert!(
        stderr.contains("tq-linux-x86_64-static, tq-linux-x86_64"),
        "{stderr}"
    );
}

// --- diagnostics -------------------------------------------------------------

#[test]
fn a_release_response_without_a_tag_is_reported_as_unparseable() {
    let mut table = routes(NEWER, NEW_BINARY, Sums::Correct);
    table.insert(
        "/api/releases/latest".to_owned(),
        Route::json("{\"message\":\"Not Found\"}".to_owned()),
    );
    let server = Server::start(table);
    let scratch = Scratch::new("no-tag");

    let output = scratch.upgrade(&["--check"], &server, &[]);

    assert_eq!(output.status.code(), Some(1), "{}", output.describe());
    assert!(
        output
            .stderr_utf8()
            .contains("could not parse the release tag"),
        "{}",
        output.describe()
    );
}

#[test]
fn an_empty_release_list_is_reported_rather_than_indexed() {
    let mut table = routes(NEWER, NEW_BINARY, Sums::Correct);
    table.insert(
        "/api/releases?per_page=1".to_owned(),
        Route::json("[]".to_owned()),
    );
    let server = Server::start(table);
    let scratch = Scratch::new("empty-list");

    let output = scratch.upgrade(&["--check"], &server, &[("TQ_CHANNEL", "next")]);

    assert_eq!(output.status.code(), Some(1), "{}", output.describe());
    assert!(
        output
            .stderr_utf8()
            .contains("no releases published for reddb-io/toon"),
        "{}",
        output.describe()
    );
}

#[test]
fn a_malformed_release_response_is_reported_as_unparseable() {
    let mut table = routes(NEWER, NEW_BINARY, Sums::Correct);
    table.insert(
        "/api/releases/latest".to_owned(),
        Route::json("not json".to_owned()),
    );
    let server = Server::start(table);
    let scratch = Scratch::new("bad-json");

    let output = scratch.upgrade(&["--check"], &server, &[]);

    assert_eq!(output.status.code(), Some(1), "{}", output.describe());
    assert!(
        output
            .stderr_utf8()
            .contains("could not parse the release response"),
        "{}",
        output.describe()
    );
}

#[test]
fn a_download_that_fails_is_reported_with_the_asset_name() {
    let mut table = routes(NEWER, NEW_BINARY, Sums::Correct);
    table.insert(
        format!("/dl/v{NEWER}/tq-linux-x86_64-static"),
        Route::status(500),
    );
    let server = Server::start(table);
    let scratch = Scratch::new("download-fails");

    let output = scratch.upgrade(&[], &server, &[]);

    assert_eq!(output.status.code(), Some(1), "{}", output.describe());
    assert!(
        output
            .stderr_utf8()
            .contains("could not download tq-linux-x86_64-static"),
        "{}",
        output.describe()
    );
    assert_eq!(scratch.binary_bytes(), original_binary_bytes());
}

#[test]
fn an_unreachable_host_reports_the_network_error() {
    let scratch = Scratch::new("unreachable");
    // Port 1 on loopback: nothing listens, and the connection refusal is
    // immediate, so this stays a network-error test rather than a timeout.
    let output = scratch.run_upgrade(
        &["--check"],
        &[
            ("TQ_API_BASE", "http://127.0.0.1:1/api"),
            ("TQ_DOWNLOAD_BASE", "http://127.0.0.1:1/dl"),
        ],
    );

    assert_eq!(output.status.code(), Some(1), "{}", output.describe());
    assert!(
        output
            .stderr_utf8()
            .contains("could not resolve the latest release"),
        "{}",
        output.describe()
    );
}

#[test]
fn a_read_only_directory_reports_how_to_install_elsewhere() {
    let server = Server::start(routes(NEWER, NEW_BINARY, Sums::Correct));
    let scratch = Scratch::new("readonly");

    std::fs::set_permissions(&scratch.directory, std::fs::Permissions::from_mode(0o555))
        .expect("make directory read-only");
    // Root ignores directory write permissions, so this guard keeps the test
    // meaningful when it runs as root instead of failing for the wrong reason.
    let probe = scratch.directory.join(".tq-upgrade-write-probe");
    let root_bypasses_permissions = std::fs::write(&probe, b"x").is_ok();
    let _ = std::fs::remove_file(&probe);

    let output = scratch.upgrade(&[], &server, &[]);

    if root_bypasses_permissions {
        assert_eq!(output.status.code(), Some(0), "{}", output.describe());
    } else {
        assert_eq!(output.status.code(), Some(1), "{}", output.describe());
        let stderr = output.stderr_utf8();
        assert!(stderr.contains("cannot write to"), "{stderr}");
        assert!(stderr.contains("re-run with sudo"), "{stderr}");
        assert!(stderr.contains("TQ_INSTALL_DIR"), "{stderr}");
    }

    let _ = std::fs::set_permissions(&scratch.directory, std::fs::Permissions::from_mode(0o755));
}

#[test]
fn upgrade_rejects_unknown_flags_and_extra_positionals() {
    let scratch = Scratch::new("usage");
    let usage = "usage: tq upgrade [--check] [VERSION]";

    for args in [vec!["-z"], vec!["--dry-run"], vec!["1.0.0", "2.0.0"]] {
        let output = scratch.run_upgrade(&args, &[]);
        assert_eq!(output.status.code(), Some(1), "{}", output.describe());
        assert!(
            output.stderr_utf8().contains(usage),
            "{args:?}: {}",
            output.stderr_utf8()
        );
    }
}

#[test]
fn a_double_dash_ends_flag_parsing_for_upgrade() {
    let server = Server::start(routes(NEWER, NEW_BINARY, Sums::Correct));
    let scratch = Scratch::new("double-dash");

    let output = scratch.upgrade(&["--check", "--", NEWER], &server, &[]);

    assert_eq!(output.status.code(), Some(1), "{}", output.describe());
    assert_eq!(server.hits(&format!("/api/releases/tags/v{NEWER}")), 1);
}

// --- fixtures ----------------------------------------------------------------

fn current_version() -> &'static str {
    // The test binary and tq are built from the same manifest.
    env!("CARGO_PKG_VERSION")
}

#[derive(Clone, Copy)]
enum Sums {
    Correct,
    Wrong,
    /// A SHA256SUMS that lists other assets but not this platform's.
    MissingEntry,
    /// A release that publishes no SHA256SUMS asset at all.
    Absent,
}

fn sums_body(binary: &[u8], sums: Sums) -> String {
    match sums {
        Sums::Correct => format!(
            "{}  tq-linux-x86_64-static\n{}  tq-linux-aarch64-static\n",
            sha256_hex(binary),
            sha256_hex(b"other")
        ),
        Sums::Wrong => format!("{}  tq-linux-x86_64-static\n", sha256_hex(b"tampered")),
        Sums::MissingEntry | Sums::Absent => {
            format!("{}  tq-macos-aarch64\n", sha256_hex(binary))
        }
    }
}

fn release_json(version: &str, assets: &[&str]) -> String {
    let assets = assets
        .iter()
        .enumerate()
        .map(|(index, name)| format!("{{\"id\":{},\"name\":\"{name}\"}}", 11 + index))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{\"tag_name\":\"v{version}\",\"assets\":[{assets}]}}")
}

/// The default routing table: a release at `version` carrying this platform's
/// static asset plus checksums, reachable through every resolution path.
fn routes(version: &str, binary: &[u8], sums: Sums) -> HashMap<String, Route> {
    let names: &[&str] = match sums {
        Sums::Absent => &["tq-linux-x86_64-static"],
        _ => &["tq-linux-x86_64-static", "SHA256SUMS"],
    };
    let json = release_json(version, names);

    let mut table = HashMap::new();
    table.insert("/api/releases/latest".to_owned(), Route::json(json.clone()));
    table.insert(
        format!("/api/releases/tags/v{version}"),
        Route::json(json.clone()),
    );
    table.insert(
        "/api/releases?per_page=1".to_owned(),
        Route::json(format!("[{json}]")),
    );
    table.insert(
        format!("/dl/v{version}/tq-linux-x86_64-static"),
        Route::bytes(binary.to_vec()),
    );
    table.insert(
        format!("/dl/v{version}/SHA256SUMS"),
        Route::bytes(sums_body(binary, sums).into_bytes()),
    );
    table
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = <sha2::Sha256 as sha2::Digest>::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

// --- a scratch copy of tq that may overwrite itself ---------------------------

struct Scratch {
    directory: PathBuf,
    binary: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let directory =
            std::env::temp_dir().join(format!("tq-upgrade.{}.{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("create scratch dir");

        let binary = directory.join("tq");
        std::fs::copy(env!("CARGO_BIN_EXE_tq"), &binary).expect("copy tq into the scratch dir");
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755))
            .expect("make the copy executable");

        Self { directory, binary }
    }

    fn upgrade(&self, args: &[&str], server: &Server, env: &[(&str, &str)]) -> Output {
        let api = format!("http://{}/api", server.address);
        let download = format!("http://{}/dl", server.address);
        let mut all = vec![
            ("TQ_API_BASE", api.as_str()),
            ("TQ_DOWNLOAD_BASE", download.as_str()),
        ];
        all.extend_from_slice(env);
        self.run_upgrade(args, &all)
    }

    fn run_upgrade(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        let mut command = Command::new(&self.binary);
        command
            .arg("upgrade")
            .args(args)
            // Inherited values would leak a developer's real token or channel
            // into the run.
            .env_remove("GITHUB_TOKEN")
            .env_remove("TQ_CHANNEL")
            .env_remove("TQ_VERSION")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in env {
            command.env(key, value);
        }
        command.output().expect("run tq upgrade")
    }

    fn binary_bytes(&self) -> Vec<u8> {
        std::fs::read(&self.binary).expect("read the scratch binary")
    }

    fn binary_mode(&self) -> u32 {
        std::fs::metadata(&self.binary)
            .expect("stat the scratch binary")
            .permissions()
            .mode()
    }

    /// Anything left in the scratch directory other than `tq` itself.
    fn leftovers(&self) -> Vec<String> {
        std::fs::read_dir(&self.directory)
            .expect("list the scratch dir")
            .filter_map(|entry| {
                let name = entry.ok()?.file_name().to_string_lossy().into_owned();
                (name != "tq").then_some(name)
            })
            .collect()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.directory, std::fs::Permissions::from_mode(0o755));
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

fn original_binary_bytes() -> Vec<u8> {
    std::fs::read(env!("CARGO_BIN_EXE_tq")).expect("read the built tq")
}

trait Describe {
    fn describe(&self) -> String;
    fn stdout_utf8(&self) -> String;
    fn stderr_utf8(&self) -> String;
}

impl Describe for Output {
    fn describe(&self) -> String {
        format!(
            "exit {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            self.status.code(),
            self.stdout_utf8(),
            self.stderr_utf8()
        )
    }

    fn stdout_utf8(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    fn stderr_utf8(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

// --- the stand-in for GitHub --------------------------------------------------

#[derive(Clone)]
struct Route {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

impl Route {
    fn json(body: String) -> Self {
        Self {
            status: 200,
            content_type: "application/json",
            body: body.into_bytes(),
        }
    }

    fn bytes(body: Vec<u8>) -> Self {
        Self {
            status: 200,
            content_type: "application/octet-stream",
            body,
        }
    }

    fn status(status: u16) -> Self {
        Self {
            status,
            content_type: "application/json",
            body: b"{}".to_vec(),
        }
    }
}

struct Server {
    address: String,
    log: Arc<Log>,
}

#[derive(Default)]
struct Log {
    hits: std::sync::Mutex<HashMap<String, usize>>,
    authorizations: std::sync::Mutex<Vec<String>>,
}

impl Server {
    fn start(routes: HashMap<String, Route>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let address = listener
            .local_addr()
            .expect("read the bound port")
            .to_string();
        let log = Arc::new(Log::default());
        let routes = Arc::new(routes);

        let thread_log = Arc::clone(&log);
        thread::spawn(move || {
            // The listener dies with the test process; each connection is
            // one request, which is all ureq needs here.
            for stream in listener.incoming().flatten() {
                let routes = Arc::clone(&routes);
                let log = Arc::clone(&thread_log);
                thread::spawn(move || serve(stream, &routes, &log));
            }
        });

        Self { address, log }
    }

    fn hits(&self, path: &str) -> usize {
        *self
            .log
            .hits
            .lock()
            .expect("hit log")
            .get(path)
            .unwrap_or(&0)
    }

    fn authorizations(&self) -> Vec<String> {
        self.log.authorizations.lock().expect("auth log").clone()
    }
}

fn serve(stream: TcpStream, routes: &HashMap<String, Route>, log: &Log) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone the stream"));
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() || request_line.is_empty() {
        return;
    }
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_owned();

    loop {
        let mut header = String::new();
        match reader.read_line(&mut header) {
            Ok(0) => break,
            Ok(_) => {
                if header.trim().is_empty() {
                    break;
                }
                if let Some(value) = header.strip_prefix("authorization: ") {
                    log.authorizations
                        .lock()
                        .expect("auth log")
                        .push(value.trim().to_owned());
                }
            }
            Err(_) => return,
        }
    }

    *log.hits
        .lock()
        .expect("hit log")
        .entry(path.clone())
        .or_insert(0) += 1;

    let missing = Route::status(404);
    let route = routes.get(&path).unwrap_or(&missing);
    let mut stream = stream;
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        route.status,
        reason(route.status),
        route.content_type,
        route.body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&route.body);
    let _ = stream.flush();
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        404 => "Not Found",
        _ => "Internal Server Error",
    }
}
