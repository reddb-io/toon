//! Conformance of the wire shapes against MCP revision 2026-07-28.
//!
//! Every expected value here is transcribed from the official specification
//! pages for the pinned revision. See `docs/mcp-conformance.md` for the
//! per-assertion citations.

mod fixture_service;

use fixture_service::Fixture;
use reddb_io_toon_rpc_mcp::{McpDispatcher, MCP_PROTOCOL_VERSION};
use serde_json::{json, Value};
use std::sync::Arc;

fn dispatcher() -> McpDispatcher<Fixture> {
    McpDispatcher::new(Arc::new(Fixture))
}

/// Send one line and require a response.
fn call(line: &str) -> Value {
    let raw = dispatcher()
        .handle_line(line)
        .expect("a request must produce a response");
    assert!(
        !raw.contains('\n'),
        "a stdio message must not contain an embedded newline: {raw}"
    );
    serde_json::from_str(&raw).expect("response must be valid JSON")
}

fn result_of(line: &str) -> Value {
    let response = call(line);
    assert_eq!(response["jsonrpc"], "2.0");
    assert!(
        response.get("error").is_none(),
        "expected a result, got error: {response}"
    );
    response["result"].clone()
}

fn error_of(line: &str) -> Value {
    let response = call(line);
    assert_eq!(response["jsonrpc"], "2.0");
    assert!(
        response.get("result").is_none(),
        "expected an error, got result: {response}"
    );
    response["error"].clone()
}

#[test]
fn pinned_protocol_version_is_the_revision_this_crate_implements() {
    assert_eq!(MCP_PROTOCOL_VERSION, "2026-07-28");
}

#[test]
fn response_carries_exactly_one_of_result_or_error() {
    let ok = call(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#);
    assert!(ok.get("result").is_some() && ok.get("error").is_none());

    let err = call(r#"{"jsonrpc":"2.0","id":1,"method":"no/such"}"#);
    assert!(err.get("error").is_some() && err.get("result").is_none());
}

// --- server/discover -------------------------------------------------------

#[test]
fn discover_matches_the_schema_shape() {
    let result = result_of(
        r#"{"jsonrpc":"2.0","id":"discover-1","method":"server/discover","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientInfo":{"name":"ExampleClient","version":"1.0.0"},"io.modelcontextprotocol/clientCapabilities":{}}}}"#,
    );

    assert_eq!(result["resultType"], "complete");
    assert_eq!(result["supportedVersions"], json!(["2026-07-28"]));
    assert_eq!(
        result["_meta"]["io.modelcontextprotocol/serverInfo"],
        json!({ "name": "fixture-server", "version": "1.0.0" })
    );
    // Capabilities are advertised for each primitive the fixture exposes.
    assert!(result["capabilities"]["tools"].is_object());
    assert!(result["capabilities"]["resources"].is_object());
    assert!(result["capabilities"]["prompts"].is_object());
    assert!(result["instructions"].is_string());
}

#[test]
fn discover_is_answered_without_any_prior_handshake() {
    // The pinned revision has no handshake: the very first message may be any
    // request, and it must be served on its own merits.
    let result = result_of(r#"{"jsonrpc":"2.0","id":1,"method":"server/discover"}"#);
    assert_eq!(result["resultType"], "complete");
}

// --- tools -----------------------------------------------------------------

#[test]
fn tools_list_uses_the_tools_key_and_input_schema() {
    let result = result_of(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);

    assert_eq!(result["resultType"], "complete");
    assert!(
        result.get("items").is_none(),
        "the list key is \"tools\", never \"items\""
    );

    let tools = result["tools"].as_array().expect("tools must be an array");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["name"], "echo");
    assert_eq!(tools[0]["title"], "Echo");
    // inputSchema is required, camelCase, and a JSON Schema object (not null).
    assert!(tools[0]["inputSchema"].is_object());
    assert_eq!(tools[0]["inputSchema"]["type"], "object");
    assert!(tools[0].get("input_schema").is_none());
}

#[test]
fn tools_call_returns_content_blocks() {
    let result = result_of(
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"echo","arguments":{"text":"hi"}}}"#,
    );

    assert_eq!(result["resultType"], "complete");
    assert_eq!(result["content"], json!([{ "type": "text", "text": "hi" }]));
    // isError is omitted rather than serialized as null on success.
    assert!(result.get("isError").is_none());
}

#[test]
fn tool_execution_failure_is_a_result_with_is_error_not_a_jsonrpc_error() {
    // Actionable failures reach the model as a normal result so it can retry.
    let result = result_of(
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"echo","arguments":{}}}"#,
    );
    assert_eq!(result["isError"], true);
    assert_eq!(result["content"][0]["type"], "text");
}

#[test]
fn unknown_tool_is_a_protocol_error_with_invalid_params() {
    let error = error_of(
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"nope","arguments":{}}}"#,
    );
    assert_eq!(error["code"], -32602);
    assert!(error["message"].as_str().unwrap().contains("Unknown tool"));
}

#[test]
fn tools_call_without_a_name_is_invalid_params() {
    let error = error_of(r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{}}"#);
    assert_eq!(error["code"], -32602);
}

// --- resources -------------------------------------------------------------

#[test]
fn resources_list_uses_the_resources_key_and_mime_type_camel_case() {
    let result = result_of(r#"{"jsonrpc":"2.0","id":1,"method":"resources/list"}"#);

    assert!(result.get("items").is_none());
    let resources = result["resources"].as_array().expect("array");
    assert_eq!(resources[0]["uri"], "file:///fixture/readme.md");
    // The schema spells this mimeType; mime_type would be unreadable to clients.
    assert_eq!(resources[0]["mimeType"], "text/markdown");
    assert!(
        resources[0].get("mime_type").is_none(),
        "mime_type is not a schema key"
    );
}

#[test]
fn resources_read_wraps_entries_in_contents() {
    let result = result_of(
        r#"{"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":"file:///fixture/readme.md"}}"#,
    );

    assert_eq!(result["resultType"], "complete");
    let contents = result["contents"].as_array().expect("contents array");
    assert_eq!(contents[0]["uri"], "file:///fixture/readme.md");
    assert_eq!(contents[0]["mimeType"], "text/markdown");
    assert_eq!(contents[0]["text"], "# Fixture");
}

#[test]
fn missing_resource_is_invalid_params_and_never_an_empty_contents_array() {
    let error = error_of(
        r#"{"jsonrpc":"2.0","id":5,"method":"resources/read","params":{"uri":"file:///nonexistent.txt"}}"#,
    );
    assert_eq!(error["code"], -32602);
    assert_eq!(error["message"], "Resource not found");
    assert_eq!(error["data"]["uri"], "file:///nonexistent.txt");
}

// --- prompts ---------------------------------------------------------------

#[test]
fn prompts_list_uses_the_prompts_key() {
    let result = result_of(r#"{"jsonrpc":"2.0","id":1,"method":"prompts/list"}"#);
    assert!(result.get("items").is_none());
    assert_eq!(result["prompts"][0]["name"], "greet");
}

#[test]
fn prompts_get_returns_messages() {
    let result = result_of(
        r#"{"jsonrpc":"2.0","id":2,"method":"prompts/get","params":{"name":"greet","arguments":{"who":"Ada"}}}"#,
    );
    assert_eq!(
        result["messages"],
        json!([{ "role": "user", "content": { "type": "text", "text": "Hello, Ada!" } }])
    );
}

#[test]
fn unknown_prompt_is_invalid_params() {
    let error =
        error_of(r#"{"jsonrpc":"2.0","id":2,"method":"prompts/get","params":{"name":"nope"}}"#);
    assert_eq!(error["code"], -32602);
}

// --- lifecycle and errors --------------------------------------------------

#[test]
fn ping_returns_an_empty_result() {
    assert_eq!(
        result_of(r#"{"jsonrpc":"2.0","id":9,"method":"ping"}"#),
        json!({})
    );
}

#[test]
fn unknown_method_is_method_not_found() {
    let error = error_of(r#"{"jsonrpc":"2.0","id":1,"method":"server/nonsense"}"#);
    assert_eq!(error["code"], -32601);
}

#[test]
fn invented_legacy_methods_are_not_served() {
    // These belong to no MCP revision this crate implements.
    for method in ["mcp/listTools", "tools/invoke", "server/capabilities"] {
        let line = format!(r#"{{"jsonrpc":"2.0","id":1,"method":"{method}"}}"#);
        assert_eq!(
            error_of(&line)["code"],
            -32601,
            "{method} must not be served"
        );
    }
}

#[test]
fn initialize_is_rejected_by_default_but_names_the_supported_versions() {
    // A modern-only server SHOULD name its versions in the error, because a
    // legacy client has no fall-forward mechanism.
    let error = error_of(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}"#,
    );
    assert_eq!(error["code"], -32601);
    assert_eq!(error["data"]["supported"], json!(["2026-07-28"]));
}

#[test]
fn dual_era_mode_answers_initialize_and_advertises_both_versions() {
    let dual = dispatcher().with_legacy_initialize(true);

    let raw = dual
        .handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)
        .unwrap();
    let response: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(response["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(response["result"]["serverInfo"]["name"], "fixture-server");

    let raw = dual
        .handle_line(r#"{"jsonrpc":"2.0","id":2,"method":"server/discover"}"#)
        .unwrap();
    let response: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        response["result"]["supportedVersions"],
        json!(["2026-07-28", "2025-11-25"])
    );
}

#[test]
fn unsupported_protocol_version_reports_code_32022_with_the_supported_list() {
    let error = error_of(
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"1900-01-01"}}}"#,
    );
    assert_eq!(error["code"], -32022);
    assert_eq!(error["message"], "Unsupported protocol version");
    assert_eq!(error["data"]["supported"], json!(["2026-07-28"]));
    assert_eq!(error["data"]["requested"], "1900-01-01");
}

#[test]
fn a_matching_protocol_version_in_meta_is_accepted() {
    let result = result_of(
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}}"#,
    );
    assert!(result["tools"].is_array());
}

// --- notifications ---------------------------------------------------------

#[test]
fn notifications_receive_no_response() {
    for line in [
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/unknown"}"#,
    ] {
        assert!(
            dispatcher().handle_line(line).is_none(),
            "a notification must not be answered: {line}"
        );
    }
}

#[test]
fn an_explicit_null_id_is_a_request_and_is_answered() {
    // Only an absent id denotes a notification.
    let response = call(r#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#);
    assert!(response["id"].is_null());
    assert_eq!(response["result"], json!({}));
}

// --- malformed input -------------------------------------------------------

#[test]
fn invalid_json_is_a_parse_error() {
    let error = error_of(r#"{"jsonrpc":"2.0","id":1,"method":"#);
    assert_eq!(error["code"], -32700);
    assert_eq!(
        call(r#"{"jsonrpc":"2.0","id":1,"method":"#)["id"],
        Value::Null
    );
}

#[test]
fn a_missing_method_is_an_invalid_request() {
    let error = error_of(r#"{"jsonrpc":"2.0","id":7}"#);
    assert_eq!(error["code"], -32600);
    // The id is echoed so the client can correlate the failure.
    assert_eq!(call(r#"{"jsonrpc":"2.0","id":7}"#)["id"], 7);
}

#[test]
fn a_wrong_jsonrpc_version_is_an_invalid_request() {
    for line in [
        r#"{"jsonrpc":"1.0","id":1,"method":"ping"}"#,
        r#"{"jsonrpc":"2.1","id":1,"method":"ping"}"#,
    ] {
        assert_eq!(error_of(line)["code"], -32600, "{line}");
    }
}

#[test]
fn the_toon_rpc_dialect_is_not_accepted_as_mcp_wire() {
    // Spec #389 §9: TOON-RPC extensions must never pass as standard MCP wire.
    let error = error_of(r#"{"toonrpc":"1.0","id":1,"method":"tools/list"}"#);
    assert_eq!(error["code"], -32600);
}

#[test]
fn blank_and_whitespace_lines_are_ignored() {
    for line in ["", "   ", "\t"] {
        assert!(dispatcher().handle_line(line).is_none());
    }
}

#[test]
fn a_non_object_message_is_an_invalid_request() {
    for line in ["[]", "42", r#""hello""#, "null"] {
        assert_eq!(error_of(line)["code"], -32600, "{line}");
    }
}

#[test]
fn params_of_the_wrong_type_do_not_panic() {
    // params must be an object or array; a scalar must fail cleanly.
    let error = error_of(r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":"nope"}"#);
    assert_eq!(error["code"], -32602);
}

#[test]
fn a_string_id_is_echoed_unchanged() {
    let response = call(r#"{"jsonrpc":"2.0","id":"abc-123","method":"ping"}"#);
    assert_eq!(response["id"], "abc-123");
}

#[test]
fn control_characters_in_arguments_stay_escaped_on_one_line() {
    let raw = dispatcher()
        .handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"echo","arguments":{"text":"a\nb"}}}"#,
        )
        .unwrap();
    assert!(
        !raw.contains('\n'),
        "an embedded newline would corrupt the stdio framing: {raw}"
    );
    let response: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(response["result"]["content"][0]["text"], "a\nb");
}
