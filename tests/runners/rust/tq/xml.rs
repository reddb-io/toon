//! Lossless XML conversion and defensive parsing behavior.

use std::io::Write;
use std::process::{Command, Stdio};

const COMPLETE_XML: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
    "<!--before & after--><?build release?>",
    "<catalog xmlns=\"urn:catalog\" xmlns:x=\"urn:item\" x:mode=\"full\" id=\"7\">",
    "lead<![CDATA[<raw>&data]]><x:item code=\"a&amp;b\"/>",
    "<x:item></x:item><!--inside--><?step done?>tail",
    "</catalog>",
);

#[test]
fn xml_read_has_one_canonical_ordered_tree() {
    let output = run_tq(&["-p", "xml", "-o", "json", "-c", "."], COMPLETE_XML);
    assert_success(&output);

    let actual: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("canonical tree is JSON");
    let expected = serde_json::json!({
        "xml": {
            "declaration": {
                "version": "1.0",
                "encoding": "UTF-8",
                "standalone": "yes"
            },
            "children": [
                {"type": "comment", "value": "before & after"},
                {"type": "processing_instruction", "target": "build", "value": "release"},
                {
                    "type": "element",
                    "name": "catalog",
                    "attributes": [
                        {"name": "xmlns", "value": "urn:catalog"},
                        {"name": "xmlns:x", "value": "urn:item"},
                        {"name": "x:mode", "value": "full"},
                        {"name": "id", "value": "7"}
                    ],
                    "children": [
                        {"type": "text", "value": "lead"},
                        {"type": "cdata", "value": "<raw>&data"},
                        {"type": "element", "name": "x:item", "attributes": [
                            {"name": "code", "value": "a&b"}
                        ], "children": [], "empty": true},
                        {"type": "element", "name": "x:item", "attributes": [], "children": [], "empty": false},
                        {"type": "comment", "value": "inside"},
                        {"type": "processing_instruction", "target": "step", "value": "done"},
                        {"type": "text", "value": "tail"}
                    ],
                    "empty": false
                }
            ]
        }
    });
    assert_eq!(actual, expected);
}

#[test]
fn xml_toon_xml_round_trip_preserves_the_canonical_tree() {
    let toon = run_tq(&["-p", "xml", "-o", "toon", "."], COMPLETE_XML);
    assert_success(&toon);

    let xml = run_tq(
        &["-p", "toon", "-o", "xml", "."],
        std::str::from_utf8(&toon.stdout).expect("TOON output is UTF-8"),
    );
    assert_success(&xml);

    let reparsed = run_tq(
        &["-p", "xml", "-o", "json", "-c", "."],
        std::str::from_utf8(&xml.stdout).expect("XML output is UTF-8"),
    );
    assert_success(&reparsed);

    let original = run_tq(&["-p", "xml", "-o", "json", "-c", "."], COMPLETE_XML);
    assert_eq!(reparsed.stdout, original.stdout);
}

#[test]
fn xml_output_requires_the_canonical_tree_instead_of_guessing() {
    let output = run_tq(
        &["-p", "json", "-o", "xml", "."],
        r#"{"item":[{"id":1},{"id":2}]}"#,
    );
    assert_failure(&output, "expected canonical XML document");
}

#[test]
fn xml_is_detected_from_extension_and_unambiguous_stdin() {
    let mut path = std::env::temp_dir().join(format!("tq-xml-{}", std::process::id()));
    path.set_extension("xml");
    std::fs::write(&path, "<root/>").expect("write XML fixture");

    let file = run_tq(&["-o", "json", "-c", ".", path.to_str().unwrap()], "");
    assert_success(&file);
    let stdin = run_tq(&["-o", "json", "-c", "."], "<?xml version=\"1.0\"?><root/>");
    assert_success(&stdin);
    assert!(String::from_utf8_lossy(&file.stdout).contains("\"xml\""));
    assert!(String::from_utf8_lossy(&stdin.stdout).contains("\"declaration\""));

    std::fs::remove_file(path).expect("remove XML fixture");
}

#[test]
fn malformed_and_adversarial_xml_fails_with_bounded_diagnostics() {
    for input in [
        "<root><open></root>",
        "<root>&unknown;</root>",
        "<!DOCTYPE root [<!ENTITY x \"boom\">]><root>&x;</root>",
    ] {
        let output = run_tq(&["-p", "xml", "."], input);
        assert!(
            !output.status.success(),
            "malformed XML unexpectedly passed"
        );
        assert!(output.stderr.len() <= 512, "diagnostic was not bounded");
        assert!(String::from_utf8_lossy(&output.stderr).contains("XML"));
    }

    let deep = format!("{}x{}", "<a>".repeat(300), "</a>".repeat(300));
    let output = run_tq(&["-p", "xml", "."], &deep);
    assert_failure(&output, "maximum depth");
}

#[test]
fn large_xml_input_is_parsed_without_recursion_or_panic() {
    let mut input = String::from("<items>");
    for index in 0..10_000 {
        input.push_str(&format!("<item id=\"{index}\"/>"));
    }
    input.push_str("</items>");

    let output = run_tq(
        &[
            "-p",
            "xml",
            "-o",
            "json",
            "-c",
            ".xml.children[0].children|length",
        ],
        &input,
    );
    assert_success(&output);
    assert_eq!(output.stdout, b"10000\n");
}

#[test]
fn malformed_xml_document_shapes_fail_cleanly() {
    let parse_cases = [
        ("", "document element is missing"),
        ("text<root/>", "text is not allowed outside"),
        ("<a/><b/>", "multiple document elements"),
        ("<![CDATA[x]]><root/>", "CDATA is not allowed outside"),
        (
            "<?xml version=\"1.0\"?><?xml version=\"1.0\"?><root/>",
            "misplaced declaration",
        ),
        ("<?xml encoding=\"UTF-8\"?><root/>", "invalid declaration"),
        ("<root a=\"&unknown;\"/>", "invalid attribute value"),
        ("<root a=\"1\" a=\"2\"/>", "invalid attribute"),
        ("<!--a--b--><root/>", "invalid comment"),
    ];
    for (input, expected) in parse_cases {
        assert_failure(&run_tq(&["-p", "xml", "."], input), expected);
    }

    let output_cases = [
        ("null", "expected canonical XML document"),
        (r#"{"xml":null}"#, "`xml` must be an object"),
        (
            r#"{"xml":{"declaration":null,"children":[],"extra":0}}"#,
            "unsupported field `extra`",
        ),
        (r#"{"xml":{"children":[]}}"#, "missing `declaration`"),
        (
            r#"{"xml":{"declaration":null,"children":{}}}"#,
            "XML children",
        ),
        (
            r#"{"xml":{"declaration":"1.0","children":[]}}"#,
            "declaration must be an object",
        ),
        (
            r#"{"xml":{"declaration":{"version":"1.0","extra":0},"children":[]}}"#,
            "declaration has unsupported field",
        ),
        (
            r#"{"xml":{"declaration":{},"children":[]}}"#,
            "missing `version`",
        ),
        (
            r#"{"xml":{"declaration":{"version":1},"children":[]}}"#,
            "version must be a string",
        ),
        (
            r#"{"xml":{"declaration":{"version":"2.0"},"children":[]}}"#,
            "version must be `1.0` or `1.1`",
        ),
        (
            r#"{"xml":{"declaration":{"version":"1.0","encoding":1},"children":[]}}"#,
            "encoding must be a string",
        ),
        (
            r#"{"xml":{"declaration":{"version":"1.0","standalone":"maybe"},"children":[]}}"#,
            "standalone declaration must be `yes` or `no`",
        ),
        (
            r#"{"xml":{"declaration":null,"children":[null]}}"#,
            "child node must be an object",
        ),
        (
            r#"{"xml":{"declaration":null,"children":[{"type":"unknown"}]}}"#,
            "unsupported XML node type",
        ),
        (
            r#"{"xml":{"declaration":null,"children":[{"type":"element","name":"r","attributes":[],"children":[{"type":"text","value":"x"}],"empty":true}]}}"#,
            "empty XML element cannot contain children",
        ),
        (
            r#"{"xml":{"declaration":null,"children":[{"type":"element","name":"r","attributes":[],"children":[],"empty":true,"extra":0}]}}"#,
            "XML element has unsupported field",
        ),
        (
            r#"{"xml":{"declaration":null,"children":[{"type":"element","name":"r","attributes":{},"children":[],"empty":true}]}}"#,
            "XML element attributes",
        ),
        (
            r#"{"xml":{"declaration":null,"children":[{"type":"element","name":"r","attributes":[],"children":{},"empty":true}]}}"#,
            "XML element children",
        ),
        (
            r#"{"xml":{"declaration":null,"children":[{"type":"element","name":"r","attributes":[],"children":[],"empty":"yes"}]}}"#,
            "empty flag must be a boolean",
        ),
        (
            r#"{"xml":{"declaration":null,"children":[{"type":"element","name":"r","attributes":[null],"children":[],"empty":true}]}}"#,
            "attribute must be an object",
        ),
        (
            r#"{"xml":{"declaration":null,"children":[{"type":"element","name":"r","attributes":[{"name":1,"value":"x"}],"children":[],"empty":true}]}}"#,
            "attribute name must be a string",
        ),
        (
            r#"{"xml":{"declaration":null,"children":[{"type":"element","name":"r","attributes":[{"name":"id","value":1}],"children":[],"empty":true}]}}"#,
            "attribute value must be a string",
        ),
        (
            r#"{"xml":{"declaration":null,"children":[{"type":"text"},{"type":"element","name":"r","attributes":[],"children":[],"empty":true}]}}"#,
            "leaf node is missing `value`",
        ),
        (
            r#"{"xml":{"declaration":null,"children":[{"type":"comment","value":"a--b"},{"type":"element","name":"r","attributes":[],"children":[],"empty":true}]}}"#,
            "invalid comment",
        ),
        (
            r#"{"xml":{"declaration":null,"children":[{"type":"processing_instruction","target":"build"},{"type":"element","name":"r","attributes":[],"children":[],"empty":true}]}}"#,
            "processing instruction is missing `value`",
        ),
    ];
    for (input, expected) in output_cases {
        assert_failure(&run_tq(&["-p", "json", "-o", "xml", "."], input), expected);
    }
}

#[test]
fn canonical_xml_output_supports_null_declaration_and_empty_instruction_value() {
    let input = r#"{"xml":{"declaration":null,"children":[{"type":"processing_instruction","target":"build","value":""},{"type":"element","name":"root","attributes":[],"children":[],"empty":true}]}}"#;
    let output = run_tq(&["-p", "json", "-o", "xml", "."], input);
    assert_success(&output);
    assert_eq!(output.stdout, b"<?build?><root/>\n");
}

fn assert_success(output: &std::process::Output) {
    assert_eq!(
        output.status.code(),
        Some(0),
        "tq failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_failure(output: &std::process::Output, expected: &str) {
    assert!(!output.status.success(), "tq unexpectedly succeeded");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(expected),
        "expected `{expected}` in: {stderr}"
    );
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
