// `tq upgrade` — self-update against the GitHub releases of this repository.
//
// The release resolution, channel/pin/token knobs, and asset naming mirror
// `install.sh`, so a binary installed by the script and a binary that
// upgraded itself always land on the same asset.

use std::cmp::Ordering;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{self, ExitCode};

const UPGRADE_USAGE: &str = "usage: tq upgrade [--check] [VERSION]";

const REPO: &str = "reddb-io/toon";
const DEFAULT_API_BASE: &str = "https://api.github.com/repos/reddb-io/toon";
const DEFAULT_DOWNLOAD_BASE: &str = "https://github.com/reddb-io/toon/releases/download";

/// Release assets are a few megabytes; the ceiling only exists so a hostile
/// or broken endpoint cannot stream into memory forever.
const MAX_DOWNLOAD_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Channel {
    Stable,
    Next,
}

impl Channel {
    fn as_str(self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Next => "next",
        }
    }
}

#[derive(Debug)]
pub(super) struct UpgradeOptions {
    check_only: bool,
    pin: Option<String>,
}

#[derive(Debug)]
struct UpgradeEnv {
    channel: Channel,
    token: Option<String>,
    api_base: String,
    download_base: String,
}

#[derive(Debug, PartialEq, Eq)]
struct ReleaseAsset {
    name: String,
    id: u64,
}

#[derive(Debug, PartialEq, Eq)]
struct Release {
    tag: String,
    assets: Vec<ReleaseAsset>,
}

impl Release {
    fn version(&self) -> &str {
        self.tag.strip_prefix('v').unwrap_or(&self.tag)
    }
}

pub(super) fn parse_upgrade_args(
    args: impl Iterator<Item = String>,
) -> Result<UpgradeOptions, String> {
    let mut check_only = false;
    let mut positional = Vec::new();
    let mut args = args.peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--check" => check_only = true,
            "--" => {
                positional.extend(args);
                break;
            }
            value if value.starts_with('-') => return Err(UPGRADE_USAGE.to_owned()),
            value => positional.push(value.to_owned()),
        }
    }

    if positional.len() > 1 {
        return Err(UPGRADE_USAGE.to_owned());
    }

    Ok(UpgradeOptions {
        check_only,
        pin: positional.pop(),
    })
}

fn upgrade_env() -> Result<UpgradeEnv, String> {
    let channel = match env::var("TQ_CHANNEL").ok().as_deref() {
        None | Some("") | Some("stable") => Channel::Stable,
        Some("next") => Channel::Next,
        Some(other) => {
            return Err(format!(
                "unsupported TQ_CHANNEL `{other}`; expected stable or next"
            ))
        }
    };

    Ok(UpgradeEnv {
        channel,
        token: non_empty_env("GITHUB_TOKEN"),
        // The two base URLs exist so the integration suite can point the whole
        // flow at a local server instead of GitHub.
        api_base: non_empty_env("TQ_API_BASE").unwrap_or_else(|| DEFAULT_API_BASE.to_owned()),
        download_base: non_empty_env("TQ_DOWNLOAD_BASE")
            .unwrap_or_else(|| DEFAULT_DOWNLOAD_BASE.to_owned()),
    })
}

fn non_empty_env(key: &str) -> Option<String> {
    env::var(key).ok().filter(|value| !value.is_empty())
}

pub(super) fn run_upgrade(options: UpgradeOptions) -> Result<(String, ExitCode), String> {
    let environment = upgrade_env()?;
    // An explicit argument wins over TQ_VERSION, the same precedence the
    // installer gives its command line.
    let pin = options.pin.clone().or_else(|| non_empty_env("TQ_VERSION"));
    let release = resolve_release(&environment, pin.as_deref())?;

    let current = env!("CARGO_PKG_VERSION");
    let latest = release.version();
    let up_to_date = if pin.is_some() {
        // A pin is a request for one exact version, so anything else — older
        // or newer — is a change worth making.
        latest == current
    } else {
        compare_versions(latest, current) != Ordering::Greater
    };

    if options.check_only {
        let status = if up_to_date {
            "up to date"
        } else {
            "update available"
        };
        return Ok((
            format!(
                "tq {current} (channel {}) — latest {latest}: {status}\n",
                environment.channel.as_str()
            ),
            if up_to_date {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            },
        ));
    }

    if up_to_date {
        return Ok((
            format!("tq {current} is already up to date.\n"),
            ExitCode::SUCCESS,
        ));
    }

    let asset = select_asset(&release, env::consts::OS, env::consts::ARCH)?;
    let binary = fetch_asset(&environment, &release, asset)?;
    let sums = fetch_asset(
        &environment,
        &release,
        release_asset(&release, "SHA256SUMS")?,
    )?;
    let sums = String::from_utf8(sums)
        .map_err(|_| format!("SHA256SUMS for {} is not valid UTF-8", release.tag))?;
    verify_checksum(&binary, &sums, &asset.name)?;

    let installed = install_binary(&binary)?;
    Ok((
        format!(
            "Updated tq {current} -> {latest} at {}\n",
            installed.display()
        ),
        ExitCode::SUCCESS,
    ))
}

// --- release resolution ------------------------------------------------------

fn resolve_release(environment: &UpgradeEnv, pin: Option<&str>) -> Result<Release, String> {
    let base = &environment.api_base;
    if let Some(pin) = pin {
        let tag = if pin.starts_with('v') {
            pin.to_owned()
        } else {
            format!("v{pin}")
        };
        let body = http_get(environment, &format!("{base}/releases/tags/{tag}"), None)
            .map_err(|error| format!("release {tag} not found: {error}"))?;
        return parse_release(&decode_body(body)?);
    }

    if environment.channel == Channel::Next {
        let body = http_get(environment, &format!("{base}/releases?per_page=1"), None)
            .map_err(|error| format!("could not list releases: {error}"))?;
        return parse_release(&decode_body(body)?);
    }

    // Latest stable; while only prereleases exist, fall back to the newest one
    // — exactly what install.sh does.
    match http_get(environment, &format!("{base}/releases/latest"), None) {
        Ok(body) => parse_release(&decode_body(body)?),
        Err(latest_error) => {
            let body = http_get(environment, &format!("{base}/releases?per_page=1"), None)
                .map_err(|_| format!("could not resolve the latest release: {latest_error}"))?;
            parse_release(&decode_body(body)?)
        }
    }
}

fn decode_body(body: Vec<u8>) -> Result<String, String> {
    String::from_utf8(body).map_err(|_| "the release response is not valid UTF-8".to_owned())
}

fn parse_release(body: &str) -> Result<Release, String> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|error| format!("could not parse the release response: {error}"))?;
    // `?per_page=1` answers with a one-element list; every other endpoint
    // answers with the release object itself.
    let release = match value {
        serde_json::Value::Array(items) => items
            .into_iter()
            .next()
            .ok_or_else(|| format!("no releases published for {REPO}"))?,
        object => object,
    };

    let tag = release
        .get("tag_name")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            "could not parse the release tag (private repo without GITHUB_TOKEN?)".to_owned()
        })?
        .to_owned();

    let assets = release
        .get("assets")
        .and_then(serde_json::Value::as_array)
        .map(|assets| {
            assets
                .iter()
                .filter_map(|asset| {
                    Some(ReleaseAsset {
                        name: asset.get("name")?.as_str()?.to_owned(),
                        id: asset.get("id").and_then(serde_json::Value::as_u64)?,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(Release { tag, assets })
}

// --- platform mapping --------------------------------------------------------

/// Asset names in preference order, mirroring `asset_name` in `release.yml`.
///
/// On Linux the static musl asset comes first — it is what install.sh picks,
/// and it runs regardless of the host glibc. The gnu asset is the fallback for
/// releases where the musl build (optional in the matrix) did not publish.
fn asset_candidates(os: &str, arch: &str) -> Result<Vec<String>, String> {
    let names: &[&str] = match (os, arch) {
        ("linux", "x86_64") => &["tq-linux-x86_64-static", "tq-linux-x86_64"],
        ("linux", "aarch64") => &["tq-linux-aarch64-static", "tq-linux-aarch64"],
        ("macos", "x86_64") => &["tq-macos-x86_64"],
        ("macos", "aarch64") => &["tq-macos-aarch64"],
        ("windows", "x86_64") => &["tq-windows-x86_64.exe"],
        _ => {
            return Err(format!(
                "unsupported platform {os}/{arch}; see https://github.com/{REPO}/releases"
            ))
        }
    };
    Ok(names.iter().map(|name| (*name).to_owned()).collect())
}

fn select_asset<'a>(
    release: &'a Release,
    os: &str,
    arch: &str,
) -> Result<&'a ReleaseAsset, String> {
    let candidates = asset_candidates(os, arch)?;
    candidates
        .iter()
        .find_map(|name| release.assets.iter().find(|asset| &asset.name == name))
        .ok_or_else(|| {
            format!(
                "no tq asset for {os}/{arch} in release {} (looked for {})",
                release.tag,
                candidates.join(", ")
            )
        })
}

fn release_asset<'a>(release: &'a Release, name: &str) -> Result<&'a ReleaseAsset, String> {
    release
        .assets
        .iter()
        .find(|asset| asset.name == name)
        .ok_or_else(|| format!("release {} publishes no {name}", release.tag))
}

// --- download + verify -------------------------------------------------------

fn fetch_asset(
    environment: &UpgradeEnv,
    release: &Release,
    asset: &ReleaseAsset,
) -> Result<Vec<u8>, String> {
    // Private repos serve assets only through the API asset endpoint, so a
    // token switches the download over to it — as install.sh does.
    let (url, accept) = match environment.token {
        Some(_) => (
            format!("{}/releases/assets/{}", environment.api_base, asset.id),
            Some("application/octet-stream"),
        ),
        None => (
            format!(
                "{}/{}/{}",
                environment.download_base, release.tag, asset.name
            ),
            None,
        ),
    };
    http_get(environment, &url, accept)
        .map_err(|error| format!("could not download {}: {error}", asset.name))
}

fn verify_checksum(bytes: &[u8], sums: &str, asset_name: &str) -> Result<(), String> {
    let expected = sums
        .lines()
        .find_map(|line| {
            let (hash, name) = line.split_once("  ")?;
            (name.trim() == asset_name).then(|| hash.trim().to_lowercase())
        })
        .ok_or_else(|| format!("no checksum for {asset_name} in SHA256SUMS"))?;

    let actual = sha256_hex(bytes);
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "checksum verification failed for {asset_name}: expected {expected}, got {actual}"
        ))
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn http_get(environment: &UpgradeEnv, url: &str, accept: Option<&str>) -> Result<Vec<u8>, String> {
    let mut request = ureq::get(url).header(
        "User-Agent",
        concat!(
            "tq/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/reddb-io/toon)"
        ),
    );
    if let Some(token) = &environment.token {
        request = request.header("Authorization", format!("Bearer {token}"));
    }
    if let Some(accept) = accept {
        request = request.header("Accept", accept);
    }

    let mut response = request.call().map_err(|error| error.to_string())?;
    response
        .body_mut()
        .with_config()
        .limit(MAX_DOWNLOAD_BYTES)
        .read_to_vec()
        .map_err(|error| error.to_string())
}

// --- install -----------------------------------------------------------------

fn install_binary(bytes: &[u8]) -> Result<PathBuf, String> {
    let target = env::current_exe()
        .map_err(|error| format!("could not locate the running tq binary: {error}"))?;
    // A symlinked tq must replace the binary it points at, not the link.
    let target = fs::canonicalize(&target).unwrap_or(target);
    let directory = target
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", target.display()))?
        .to_path_buf();

    // Same directory as the target, so the final rename stays on one
    // filesystem and is therefore atomic.
    let staged = directory.join(format!(".tq-upgrade.{}.tmp", process::id()));
    write_staged_binary(&staged, bytes).map_err(|error| unwritable(&directory, &error))?;

    match swap_in_place(&staged, &target) {
        Ok(()) => Ok(target),
        Err(error) => {
            let _ = fs::remove_file(&staged);
            Err(unwritable(&directory, &error))
        }
    }
}

fn write_staged_binary(staged: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(staged)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(staged, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn swap_in_place(staged: &Path, target: &Path) -> io::Result<()> {
    // Unix unlinks the old inode while the running process keeps executing it,
    // so replacing the live binary is a plain rename.
    fs::rename(staged, target)
}

#[cfg(windows)]
fn swap_in_place(staged: &Path, target: &Path) -> io::Result<()> {
    // Windows refuses to overwrite a running image, but it does allow renaming
    // it: move the live binary aside, put the new one in its place, then try to
    // delete the leftover. The delete fails while this process still runs, so
    // the stale file is cleaned up by the next upgrade instead.
    let parked = target.with_extension(format!("old-{}", process::id()));
    let _ = fs::remove_file(&parked);
    fs::rename(target, &parked)?;
    if let Err(error) = fs::rename(staged, target) {
        let _ = fs::rename(&parked, target);
        return Err(error);
    }
    let _ = fs::remove_file(&parked);
    Ok(())
}

fn unwritable(directory: &Path, error: &io::Error) -> String {
    if error.kind() == io::ErrorKind::PermissionDenied {
        format!(
            "cannot write to {}: {error}\n\
             re-run with sudo, or reinstall elsewhere: \
             curl -fsSL https://raw.githubusercontent.com/{REPO}/main/install.sh | TQ_INSTALL_DIR=~/.local/bin sh",
            directory.display()
        )
    } else {
        format!("cannot replace tq in {}: {error}", directory.display())
    }
}

// --- version comparison ------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
enum PreField {
    Number(u64),
    Text(String),
}

impl Ord for PreField {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (PreField::Number(left), PreField::Number(right)) => left.cmp(right),
            (PreField::Text(left), PreField::Text(right)) => left.cmp(right),
            // Semver: numeric identifiers always rank below alphanumeric ones.
            (PreField::Number(_), PreField::Text(_)) => Ordering::Less,
            (PreField::Text(_), PreField::Number(_)) => Ordering::Greater,
        }
    }
}

impl PartialOrd for PreField {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Version {
    core: [u64; 3],
    pre: Vec<PreField>,
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.core.cmp(&other.core).then_with(|| {
            match (self.pre.is_empty(), other.pre.is_empty()) {
                // Semver: a release outranks any of its prereleases.
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => self.pre.cmp(&other.pre),
            }
        })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Lenient on purpose: an unreadable component reads as 0 rather than failing
/// the upgrade, because the tag is GitHub's to shape, not ours.
fn parse_version(text: &str) -> Version {
    let text = text.trim().strip_prefix('v').unwrap_or(text.trim());
    let text = text.split('+').next().unwrap_or(text);
    let (core, pre) = text.split_once('-').unwrap_or((text, ""));

    let mut parsed = [0u64; 3];
    for (slot, part) in parsed.iter_mut().zip(core.split('.')) {
        *slot = part.parse().unwrap_or(0);
    }

    let pre = if pre.is_empty() {
        Vec::new()
    } else {
        pre.split('.')
            .map(|field| match field.parse::<u64>() {
                Ok(number) => PreField::Number(number),
                Err(_) => PreField::Text(field.to_owned()),
            })
            .collect()
    };

    Version { core: parsed, pre }
}

fn compare_versions(left: &str, right: &str) -> Ordering {
    parse_version(left).cmp(&parse_version(right))
}

#[cfg(test)]
mod upgrade_unit_tests {
    use super::*;

    fn release(tag: &str, assets: &[(&str, u64)]) -> Release {
        Release {
            tag: tag.to_owned(),
            assets: assets
                .iter()
                .map(|(name, id)| ReleaseAsset {
                    name: (*name).to_owned(),
                    id: *id,
                })
                .collect(),
        }
    }

    #[test]
    fn parses_a_release_object_with_its_assets() {
        let parsed = parse_release(
            r#"{"tag_name":"v1.2.3","assets":[{"id":7,"name":"tq-linux-x86_64-static"},
               {"id":8,"name":"SHA256SUMS"}]}"#,
        )
        .expect("parse the release");

        assert_eq!(
            parsed,
            release(
                "v1.2.3",
                &[("tq-linux-x86_64-static", 7), ("SHA256SUMS", 8)]
            )
        );
        assert_eq!(parsed.version(), "1.2.3");
    }

    #[test]
    fn parses_the_first_element_of_a_release_list() {
        let parsed =
            parse_release(r#"[{"tag_name":"v0.2.0-next.4","assets":[]},{"tag_name":"v0.1.0"}]"#)
                .expect("parse the release list");

        assert_eq!(parsed, release("v0.2.0-next.4", &[]));
        assert_eq!(parsed.version(), "0.2.0-next.4");
    }

    #[test]
    fn a_release_without_assets_parses_to_an_empty_asset_list() {
        assert_eq!(
            parse_release(r#"{"tag_name":"v1.0.0"}"#).expect("parse"),
            release("v1.0.0", &[])
        );
    }

    #[test]
    fn assets_missing_a_name_or_an_id_are_skipped_rather_than_failing_the_parse() {
        let parsed = parse_release(
            r#"{"tag_name":"v1.0.0","assets":[{"id":1},{"name":"nameless"},{"id":2,"name":"real"}]}"#,
        )
        .expect("parse");

        assert_eq!(parsed, release("v1.0.0", &[("real", 2)]));
    }

    #[test]
    fn parsing_reports_a_missing_tag_an_empty_list_and_malformed_json() {
        assert!(parse_release(r#"{"message":"Not Found"}"#)
            .expect_err("no tag")
            .contains("could not parse the release tag"));
        assert!(parse_release("[]")
            .expect_err("empty list")
            .contains("no releases published for reddb-io/toon"));
        assert!(parse_release("{oops")
            .expect_err("malformed")
            .contains("could not parse the release response"));
    }

    #[test]
    fn a_tag_without_the_v_prefix_still_reads_as_a_version() {
        assert_eq!(release("1.2.3", &[]).version(), "1.2.3");
    }

    #[test]
    fn asset_candidates_mirror_the_release_matrix() {
        assert_eq!(
            asset_candidates("linux", "x86_64").expect("linux x86_64"),
            ["tq-linux-x86_64-static", "tq-linux-x86_64"]
        );
        assert_eq!(
            asset_candidates("linux", "aarch64").expect("linux aarch64"),
            ["tq-linux-aarch64-static", "tq-linux-aarch64"]
        );
        assert_eq!(
            asset_candidates("macos", "x86_64").expect("macos intel"),
            ["tq-macos-x86_64"]
        );
        assert_eq!(
            asset_candidates("macos", "aarch64").expect("macos arm"),
            ["tq-macos-aarch64"]
        );
        assert_eq!(
            asset_candidates("windows", "x86_64").expect("windows"),
            ["tq-windows-x86_64.exe"]
        );

        for (os, arch) in [
            ("linux", "riscv64"),
            ("freebsd", "x86_64"),
            ("windows", "aarch64"),
        ] {
            assert!(asset_candidates(os, arch)
                .expect_err("unsupported")
                .contains(&format!("unsupported platform {os}/{arch}")));
        }
    }

    #[test]
    fn asset_selection_prefers_static_then_falls_back_to_gnu() {
        let both = release(
            "v1.0.0",
            &[("tq-linux-x86_64", 1), ("tq-linux-x86_64-static", 2)],
        );
        assert_eq!(
            select_asset(&both, "linux", "x86_64")
                .expect("static wins")
                .name,
            "tq-linux-x86_64-static"
        );

        let gnu_only = release("v1.0.0", &[("tq-linux-x86_64", 1)]);
        assert_eq!(
            select_asset(&gnu_only, "linux", "x86_64")
                .expect("gnu fallback")
                .name,
            "tq-linux-x86_64"
        );
    }

    #[test]
    fn asset_selection_reports_every_name_it_looked_for() {
        let error = select_asset(&release("v1.0.0", &[("SHA256SUMS", 1)]), "linux", "aarch64")
            .expect_err("no asset");

        assert!(
            error.contains("no tq asset for linux/aarch64 in release v1.0.0"),
            "{error}"
        );
        assert!(
            error.contains("tq-linux-aarch64-static, tq-linux-aarch64"),
            "{error}"
        );
    }

    #[test]
    fn release_asset_looks_a_name_up_by_hand() {
        let published = release("v1.0.0", &[("SHA256SUMS", 3)]);

        assert_eq!(
            release_asset(&published, "SHA256SUMS").expect("found").id,
            3
        );
        assert!(release_asset(&published, "checksums.txt")
            .expect_err("absent")
            .contains("release v1.0.0 publishes no checksums.txt"));
    }

    #[test]
    fn version_comparison_orders_cores_then_prereleases() {
        for (left, right) in [
            ("1.0.1", "1.0.0"),
            ("1.1.0", "1.0.9"),
            ("2.0.0", "1.99.99"),
            // A release outranks its own prereleases.
            ("0.12.0", "0.12.0-next.7"),
            // Numeric prerelease fields compare as numbers, not strings.
            ("0.12.0-next.10", "0.12.0-next.9"),
            // A longer prerelease outranks the prefix it extends.
            ("0.12.0-next.1.1", "0.12.0-next.1"),
            // Semver: alphanumeric fields outrank numeric ones.
            ("0.12.0-rc", "0.12.0-1"),
            ("0.12.0-rc.2", "0.12.0-rc.1"),
        ] {
            assert_eq!(
                compare_versions(left, right),
                Ordering::Greater,
                "{left} > {right}"
            );
            assert_eq!(
                compare_versions(right, left),
                Ordering::Less,
                "{right} < {left}"
            );
        }

        for (left, right) in [
            ("1.2.3", "1.2.3"),
            // The leading v is decoration, and so is build metadata.
            ("v1.2.3", "1.2.3"),
            ("1.2.3+build.9", "1.2.3"),
            (" 1.2.3 ", "1.2.3"),
            // Missing components read as zero.
            ("1.2", "1.2.0"),
            ("1", "1.0.0"),
            ("0.12.0-next.3", "0.12.0-next.3"),
        ] {
            assert_eq!(
                compare_versions(left, right),
                Ordering::Equal,
                "{left} == {right}"
            );
        }
    }

    #[test]
    fn an_unreadable_version_component_reads_as_zero_rather_than_failing() {
        assert_eq!(compare_versions("1.x.3", "1.0.3"), Ordering::Equal);
        assert_eq!(compare_versions("", "0.0.0"), Ordering::Equal);
    }

    #[test]
    fn checksum_verification_accepts_the_matching_line_only() {
        let sums = format!(
            "{}  tq-linux-x86_64-static\n{}  tq-macos-aarch64\n",
            sha256_hex(b"payload"),
            sha256_hex(b"other")
        );

        verify_checksum(b"payload", &sums, "tq-linux-x86_64-static").expect("matching checksum");

        assert!(
            verify_checksum(b"tampered", &sums, "tq-linux-x86_64-static")
                .expect_err("mismatch")
                .contains("checksum verification failed for tq-linux-x86_64-static")
        );
        assert!(verify_checksum(b"payload", &sums, "tq-windows-x86_64.exe")
            .expect_err("absent")
            .contains("no checksum for tq-windows-x86_64.exe in SHA256SUMS"));
    }

    #[test]
    fn checksum_verification_ignores_case_and_lines_it_cannot_split() {
        let sums = format!(
            "# a comment with no separator\n{}  tq-linux-x86_64-static\n",
            sha256_hex(b"payload").to_uppercase()
        );

        verify_checksum(b"payload", &sums, "tq-linux-x86_64-static").expect("uppercase checksum");
    }

    #[test]
    fn sha256_matches_the_known_digest_of_the_empty_input() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn upgrade_arguments_parse_the_check_flag_and_the_pin() {
        let parsed = parse_upgrade_args(["--check".to_owned(), "1.2.3".to_owned()].into_iter())
            .expect("parse");
        assert!(parsed.check_only);
        assert_eq!(parsed.pin.as_deref(), Some("1.2.3"));

        let bare = parse_upgrade_args(std::iter::empty()).expect("parse");
        assert!(!bare.check_only);
        assert_eq!(bare.pin, None);
    }

    #[test]
    fn upgrade_arguments_reject_unknown_flags_and_extra_positionals() {
        for args in [vec!["-z"], vec!["--nope"], vec!["1.0.0", "2.0.0"]] {
            let args = args.into_iter().map(str::to_owned);
            assert_eq!(
                parse_upgrade_args(args).expect_err("rejected"),
                UPGRADE_USAGE
            );
        }
    }

    #[test]
    fn the_channel_names_itself() {
        assert_eq!(Channel::Stable.as_str(), "stable");
        assert_eq!(Channel::Next.as_str(), "next");
    }

    #[test]
    fn a_permission_denied_write_points_at_sudo_and_the_installer() {
        let denied = unwritable(
            Path::new("/usr/local/bin"),
            &io::Error::from(io::ErrorKind::PermissionDenied),
        );
        assert!(
            denied.contains("cannot write to /usr/local/bin"),
            "{denied}"
        );
        assert!(denied.contains("re-run with sudo"), "{denied}");
        assert!(denied.contains("TQ_INSTALL_DIR"), "{denied}");

        let other = unwritable(Path::new("/tmp"), &io::Error::from(io::ErrorKind::NotFound));
        assert!(other.contains("cannot replace tq in /tmp"), "{other}");
        assert!(!other.contains("sudo"), "{other}");
    }
}
