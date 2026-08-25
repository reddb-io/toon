//! End-to-end stdio transport: a scripted client transcript driven through
//! `serve_stdio_with`, byte for byte, exactly as an MCP client would.

mod fixture_service;

use fixture_service::Fixture;
use reddb_io_toon_rpc_mcp::{serve_stdio_with, McpDispatcher};
use serde_json::Value;
use std::io::BufReader;
use std::sync::Arc;

/// Feed a transcript of request lines through the transport and collect the
/// response lines it writes.
fn run(transcript: &str) -> Vec<Value> {
    let dispatcher = McpDispatcher::new(Arc::new(Fixture));
    let mut input = BufReader::new(transcript.as_bytes());
    let mut output: Vec<u8> = Vec::new();

    serve_stdio_with(&dispatcher, &mut input, &mut output).expect("transport must not fail");

    let text = String::from_utf8(output).expect("stdout must be UTF-8");
    if text.is_empty() {
        return vec![];
    }

    assert!(
        text.ends_with('\n'),
        "every message must be newline-terminated"
    );
    text.lines()
        .map(|line| {
            serde_json::from_str(line).expect("each line must be one complete JSON message")
        })
        .collect()
}

/// The transcript a modern MCP client produces on a fresh connection:
/// discover, then list, then call. There is no handshake in this revision.
const MODERN_TRANSCRIPT: &str = concat!(
    r#"{"jsonrpc":"2.0","id":1,"method":"server/discover","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientInfo":{"name":"ExampleClient","version":"1.0.0"},"io.modelcontextprotocol/clientCapabilities":{}}}}"#,
    "\n",
    r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}}"#,
    "\n",
    r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"echo","arguments":{"text":"hello"},"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}}"#,
    "\n",
);

#[test]
fn a_modern_client_transcript_is_served_end_to_end() {
    let responses = run(MODERN_TRANSCRIPT);
    assert_eq!(responses.len(), 3, "one response per request");

    // Responses arrive in order, each correlated by id.
    assert_eq!(responses[0]["id"], 1);
    assert_eq!(responses[0]["result"]["supportedVersions"][0], "2026-07-28");

    assert_eq!(responses[1]["id"], 2);
    assert_eq!(responses[1]["result"]["tools"][0]["name"], "echo");
    assert!(responses[1]["result"]["tools"][0]["inputSchema"].is_object());

    assert_eq!(responses[2]["id"], 3);
    assert_eq!(responses[2]["result"]["content"][0]["text"], "hello");
    assert!(responses[2]["result"].get("isError").is_none());
}

#[test]
fn every_response_is_exactly_one_line() {
    let dispatcher = McpDispatcher::new(Arc::new(Fixture));
    let mut input = BufReader::new(MODERN_TRANSCRIPT.as_bytes());
    let mut output: Vec<u8> = Vec::new();
    serve_stdio_with(&dispatcher, &mut input, &mut output).unwrap();

    let text = String::from_utf8(output).unwrap();
    assert_eq!(
        text.matches('\n').count(),
        3,
        "three messages means exactly three newlines: {text}"
    );
}

#[test]
fn a_legacy_client_transcript_fails_deterministically_against_a_modern_server() {
    // Legacy client, modern server: the handshake is refused, and the error
    // names the versions the server does speak.
    let responses = run(concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"LegacyClient","version":"1.0.0"}}}"#,
        "\n",
    ));

    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0]["error"]["code"], -32601);
    assert_eq!(responses[0]["error"]["data"]["supported"][0], "2026-07-28");
}

#[test]
fn a_dual_era_transcript_completes_the_legacy_handshake() {
    let dispatcher = McpDispatcher::new(Arc::new(Fixture)).with_legacy_initialize(true);
    let transcript = concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"LegacyClient","version":"1.0.0"}}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"echo","arguments":{"text":"hi"}}}"#,
        "\n",
    );

    let mut input = BufReader::new(transcript.as_bytes());
    let mut output: Vec<u8> = Vec::new();
    serve_stdio_with(&dispatcher, &mut input, &mut output).unwrap();
    let text = String::from_utf8(output).unwrap();
    let responses: Vec<Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();

    // initialize, tools/list, tools/call are answered; the notification is not.
    assert_eq!(responses.len(), 3, "the notification must draw no reply");
    assert_eq!(responses[0]["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(responses[1]["id"], 2);
    assert_eq!(responses[2]["result"]["content"][0]["text"], "hi");
}

#[test]
fn notifications_produce_no_output_at_all() {
    let responses = run(concat!(
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        "\n",
        r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}"#,
        "\n",
    ));
    assert!(responses.is_empty());
}

#[test]
fn a_malformed_line_does_not_desynchronize_the_stream() {
    // One bad line yields one error and the following request still works.
    let responses = run(concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#,
        "\n",
        "{ this is not json",
        "\n",
        r#"{"jsonrpc":"2.0","id":3,"method":"ping"}"#,
        "\n",
    ));

    assert_eq!(responses.len(), 3);
    assert_eq!(responses[0]["id"], 1);
    assert_eq!(responses[1]["error"]["code"], -32700);
    assert_eq!(responses[2]["id"], 3);
    assert_eq!(responses[2]["result"], serde_json::json!({}));
}

#[test]
fn blank_lines_between_messages_are_tolerated() {
    let responses = run(concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#,
        "\n\n\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#,
        "\n",
    ));
    assert_eq!(responses.len(), 2, "blank lines are not messages");
}

#[test]
fn a_final_line_without_a_trailing_newline_is_still_processed() {
    let responses = run(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#);
    assert_eq!(responses.len(), 1);
}

#[test]
fn eof_on_input_ends_the_loop_cleanly() {
    // The loop returns Ok on EOF, which is the graceful shutdown signal.
    let dispatcher = McpDispatcher::new(Arc::new(Fixture));
    let mut input = BufReader::new(&b""[..]);
    let mut output: Vec<u8> = Vec::new();
    assert!(serve_stdio_with(&dispatcher, &mut input, &mut output).is_ok());
    assert!(output.is_empty());
}
