use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Debug)]
struct Case {
    args: Vec<String>,
    stdin: String,
    stdout: String,
    stderr: String,
    exit_code: i32,
}

#[test]
fn golden_cli_cases() {
    let root = env!("CARGO_MANIFEST_DIR");
    let cases_dir = std::path::Path::new(root).join("tests/golden");

    for entry in fs::read_dir(cases_dir).expect("golden cases directory exists") {
        let entry = entry.expect("golden case directory entry is readable");
        if !entry.file_type().expect("golden case file type").is_dir() {
            continue;
        }

        let case = read_case(&entry.path());
        let output = run_case(&case);

        assert_eq!(
            output.status.code(),
            Some(case.exit_code),
            "{} exit code",
            entry.file_name().to_string_lossy()
        );
        assert_eq!(
            String::from_utf8(output.stdout).expect("stdout is utf-8"),
            case.stdout,
            "{} stdout",
            entry.file_name().to_string_lossy()
        );
        assert_eq!(
            String::from_utf8(output.stderr).expect("stderr is utf-8"),
            case.stderr,
            "{} stderr",
            entry.file_name().to_string_lossy()
        );
    }
}

#[test]
fn toon_json_toon_round_trips_to_same_canonical_form() {
    let input = "name: Ada\nusers[2]{id,name}:\n  1,Ada\n  2,Bob\n";
    let to_json = run_tq(&["-o", "json", "."], input);
    assert_eq!(to_json.status.code(), Some(0), "TOON to JSON exits cleanly");

    let json = String::from_utf8(to_json.stdout).expect("json stdout is utf-8");
    let back_to_toon = run_tq(&["-p", "json", "-o", "toon", "."], &json);
    assert_eq!(
        back_to_toon.status.code(),
        Some(0),
        "JSON to TOON exits cleanly"
    );
    assert_eq!(
        String::from_utf8(back_to_toon.stdout).expect("toon stdout is utf-8"),
        input
    );
}

#[test]
fn jq_oracle_core_filters() {
    let input = r#"{"users":[{"name":"Ada","score":41,"active":true},{"name":"Bob","score":7,"active":false}],"meta":{"team":"core"},"empty":null}"#;
    let filters = [
        ".users[]|select(.active)|{name:.name,next:(.score+1)}",
        ".users|map(.score)|length",
        ".meta|keys",
        ".meta|has(\"team\")",
        ".empty==null,(.users|length>1)",
        "[.users[].name]",
    ];

    for filter in filters {
        let tq = run_tq(&["-p", "json", "-o", "json", "-c", filter], input);
        assert_eq!(
            tq.status.code(),
            Some(0),
            "tq exits cleanly for {filter}: {}",
            String::from_utf8_lossy(&tq.stderr)
        );

        let jq = run_jq(filter, input);
        assert_eq!(
            jq.status.code(),
            Some(0),
            "jq exits cleanly for {filter}: {}",
            String::from_utf8_lossy(&jq.stderr)
        );

        assert_eq!(
            String::from_utf8(tq.stdout).expect("tq stdout is utf-8"),
            String::from_utf8(jq.stdout).expect("jq stdout is utf-8"),
            "oracle match for {filter}"
        );
    }
}

#[test]
fn jq_oracle_aggregation_ordering_and_strings() {
    let input = r#"{"users":[{"name":"Ada","score":41,"team":"research"},{"name":"Bob","score":7,"team":"ops"},{"name":"Cid","score":19,"team":"research"}],"tags":["ops","core","ops","dev"],"meta":{"team":"core","level":2},"phrase":"Ada-Lovelace"}"#;
    let filters = [
        ".users|sort_by(.score)|map(.name)",
        ".users|group_by(.team)|map({team:.[0].team,names:map(.name)})",
        ".tags|unique",
        ".users|map(.score)|add",
        ".users|min_by(.score)|.name",
        ".users|max_by(.score)|.name",
        ".meta|to_entries|sort_by(.key)",
        ".meta|to_entries|from_entries",
        ".phrase|split(\"-\")",
        "[.users[].name]|join(\",\")",
        ".phrase|test(\"^Ada\")",
    ];

    for filter in filters {
        let tq = run_tq(&["-p", "json", "-o", "json", "-c", filter], input);
        assert_eq!(
            tq.status.code(),
            Some(0),
            "tq exits cleanly for {filter}: {}",
            String::from_utf8_lossy(&tq.stderr)
        );

        let jq = run_jq(filter, input);
        assert_eq!(
            jq.status.code(),
            Some(0),
            "jq exits cleanly for {filter}: {}",
            String::from_utf8_lossy(&jq.stderr)
        );

        assert_eq!(
            String::from_utf8(tq.stdout).expect("tq stdout is utf-8"),
            String::from_utf8(jq.stdout).expect("jq stdout is utf-8"),
            "oracle match for {filter}"
        );
    }
}

fn read_case(path: &std::path::Path) -> Case {
    let args = fs::read_to_string(path.join("args.txt"))
        .expect("args fixture exists")
        .split_whitespace()
        .map(ToOwned::to_owned)
        .collect();

    let exit_code = fs::read_to_string(path.join("exit.txt"))
        .expect("exit fixture exists")
        .trim()
        .parse()
        .expect("exit fixture is an integer");

    Case {
        args,
        stdin: fs::read_to_string(path.join("stdin.toon")).expect("stdin fixture exists"),
        stdout: fs::read_to_string(path.join("stdout.toon")).expect("stdout fixture exists"),
        stderr: fs::read_to_string(path.join("stderr.txt")).expect("stderr fixture exists"),
        exit_code,
    }
}

fn run_case(case: &Case) -> std::process::Output {
    run_tq(
        &case.args.iter().map(String::as_str).collect::<Vec<_>>(),
        &case.stdin,
    )
}

fn run_tq(args: &[&str], stdin: &str) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_tq"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn tq");

    child
        .stdin
        .as_mut()
        .expect("stdin is piped")
        .write_all(stdin.as_bytes())
        .expect("write stdin");

    child.wait_with_output().expect("wait for tq")
}

fn run_jq(filter: &str, stdin: &str) -> std::process::Output {
    let mut child = Command::new("jq")
        .args(["-c", filter])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn jq");

    child
        .stdin
        .as_mut()
        .expect("stdin is piped")
        .write_all(stdin.as_bytes())
        .expect("write jq stdin");

    child.wait_with_output().expect("wait for jq")
}
