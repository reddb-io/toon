//! The shared MCP transcript (`tests/corpus/mcp/transcript.json`), replayed
//! against the Rust server. `packages/toon-rpc-mcp/test/transcript.test.mjs`
//! replays the same file against the TypeScript one.

use std::path::Path;

use reddb_io_toon_rpc_mcp::{
    serve_lines, text_content, CallToolResult, GetPromptResult, Implementation, JsonObject,
    McpError, McpServer, McpService, Prompt, PromptMessage, ReadResourceResult, Resource,
    ResourceContents, Role, Tool,
};
use serde_json::{Map, Value};

const STEP_COUNT: usize = 26;

struct Fixture(Value);

impl McpService for Fixture {
    fn server_info(&self) -> Implementation {
        Implementation {
            name: self.0["server"]["name"].as_str().unwrap().into(),
            version: self.0["server"]["version"].as_str().unwrap().into(),
            title: None,
        }
    }
    fn instructions(&self) -> Option<String> {
        self.0["server"]["instructions"].as_str().map(Into::into)
    }

    fn tools_enabled(&self) -> bool {
        true
    }
    fn list_tools(&self) -> Vec<Tool> {
        serde_json::from_value(self.0["tools"].clone()).unwrap()
    }
    fn call_tool(&self, name: &str, arguments: &JsonObject) -> Result<CallToolResult, McpError> {
        match name {
            "echo" => Ok(CallToolResult::text(
                arguments
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )),
            "fail" => Ok(CallToolResult::error("boom")),
            _ => Err(McpError::unknown_tool(name)),
        }
    }

    fn resources_enabled(&self) -> bool {
        true
    }
    fn list_resources(&self) -> Vec<Resource> {
        serde_json::from_value(self.0["resources"].clone()).unwrap()
    }
    fn read_resource(&self, uri: &str) -> Result<ReadResourceResult, McpError> {
        if uri != "memo://hello" {
            return Err(McpError::resource_not_found(uri));
        }
        Ok(ReadResourceResult {
            contents: vec![ResourceContents {
                uri: uri.into(),
                mime_type: Some("text/plain".into()),
                text: Some("Hello".into()),
                blob: None,
            }],
        })
    }

    fn prompts_enabled(&self) -> bool {
        true
    }
    fn list_prompts(&self) -> Vec<Prompt> {
        serde_json::from_value(self.0["prompts"].clone()).unwrap()
    }
    fn get_prompt(
        &self,
        name: &str,
        arguments: &Map<String, Value>,
    ) -> Result<GetPromptResult, McpError> {
        if name != "greet" {
            return Err(McpError::invalid_params(format!("Unknown prompt: {name}")));
        }
        let who = arguments
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| McpError::invalid_params("Missing argument: name"))?;
        Ok(GetPromptResult {
            description: Some("Greet someone.".into()),
            messages: vec![PromptMessage {
                role: Role::User,
                content: text_content(format!("Hello, {who}!")),
            }],
        })
    }
}

fn fixture() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus/mcp/transcript.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// Exact match, except an error message is only compared when expected.
fn assert_matches(actual: &Value, expected: &Value, path: &str) {
    match expected {
        Value::Array(expected) => {
            let actual = actual
                .as_array()
                .unwrap_or_else(|| panic!("{path}: expected an array"));
            assert_eq!(actual.len(), expected.len(), "{path}: length");
            for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                assert_matches(actual, expected, &format!("{path}[{index}]"));
            }
        }
        Value::Object(expected) => {
            let actual = actual
                .as_object()
                .unwrap_or_else(|| panic!("{path}: expected an object"));
            let mut keys = actual
                .keys()
                .filter(|key| {
                    !(path.ends_with(".error")
                        && *key == "message"
                        && !expected.contains_key("message"))
                })
                .collect::<Vec<_>>();
            keys.sort();
            let mut wanted = expected.keys().collect::<Vec<_>>();
            wanted.sort();
            assert_eq!(keys, wanted, "{path}: members");
            for (key, expected) in expected {
                assert_matches(&actual[key], expected, &format!("{path}.{key}"));
            }
        }
        expected => assert_eq!(actual, expected, "{path}"),
    }
}

fn step_line(step: &Value) -> String {
    match step.get("sendRaw") {
        Some(raw) => raw.as_str().unwrap().to_owned(),
        None => step["send"].to_string(),
    }
}

#[test]
fn shared_mcp_transcript() {
    let fixture = fixture();
    assert_eq!(fixture["schemaVersion"], "mcp-transcript-v1");
    assert_eq!(
        fixture["protocolVersion"],
        reddb_io_toon_rpc_mcp::MCP_PROTOCOL_VERSION
    );
    let mut steps = 0;
    for session in fixture["sessions"].as_array().unwrap() {
        let server = McpServer::new(Fixture(fixture.clone()));
        for (index, step) in session["steps"].as_array().unwrap().iter().enumerate() {
            steps += 1;
            let answer = server.handle_line(&step_line(step));
            let at = format!("{} step {index}", session["name"].as_str().unwrap());
            if step["expect"].is_null() {
                assert_eq!(answer, None, "{at}: no response");
                continue;
            }
            let answer = answer.unwrap_or_else(|| panic!("{at}: no answer"));
            assert!(!answer.contains('\n'), "{at}: one line");
            assert_matches(
                &serde_json::from_str(&answer).unwrap(),
                &step["expect"],
                &at,
            );
        }
    }
    assert_eq!(steps, STEP_COUNT);
}

#[tokio::test]
async fn stdio_carries_one_json_message_per_line() {
    let fixture = fixture();
    let first = &fixture["sessions"][0]["steps"];
    let input = first
        .as_array()
        .unwrap()
        .iter()
        .map(|step| format!("{}\n", step_line(step)))
        .collect::<String>();
    let server = McpServer::new(Fixture(fixture.clone()));
    let mut output = Vec::new();
    serve_lines(&server, input.as_bytes(), &mut output, 1 << 20)
        .await
        .unwrap();
    let lines = String::from_utf8(output).unwrap();
    let answers = lines.lines().collect::<Vec<_>>();
    let expected = first
        .as_array()
        .unwrap()
        .iter()
        .filter(|step| !step["expect"].is_null())
        .collect::<Vec<_>>();
    assert_eq!(answers.len(), expected.len());
    for (answer, step) in answers.iter().zip(expected) {
        assert_matches(
            &serde_json::from_str(answer).unwrap(),
            &step["expect"],
            "stdio",
        );
    }

    let mut refused = Vec::new();
    serve_lines(&server, &b"{\"jsonrpc\":\"2.0\"}"[..], &mut refused, 8)
        .await
        .unwrap();
    let refusal: Value = serde_json::from_slice(&refused).unwrap();
    assert_eq!(refusal["error"]["code"], -32600);
}

#[test]
fn toon_results_carry_toon_text_and_structured_content() {
    let value = serde_json::json!({ "rows": [{ "a": 1, "b": "x" }] });
    let result = CallToolResult::toon(&value);
    let serialized = serde_json::to_value(&result).unwrap();
    assert_eq!(serialized["content"][0]["type"], "text");
    let text = serialized["content"][0]["text"].as_str().unwrap();
    assert_eq!(reddb_io_toon::decode(text).unwrap().to_json_value(), value);
    assert_eq!(serialized["structuredContent"], value);
}
