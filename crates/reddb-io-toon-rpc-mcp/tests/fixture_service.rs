//! Shared fixture service used by the conformance and transport tests.

use reddb_io_toon_rpc_mcp::{
    CallToolResult, Content, GetPromptResult, McpError, McpResult, McpService, Prompt,
    PromptArgument, PromptMessage, Resource, ResourceContents, ServerInfo, Tool,
};
use serde_json::{json, Value};

pub struct Fixture;

impl McpService for Fixture {
    fn server_info(&self) -> ServerInfo {
        ServerInfo {
            name: "fixture-server".into(),
            version: "1.0.0".into(),
            title: None,
        }
    }

    fn instructions(&self) -> Option<String> {
        Some("Fixture server for conformance tests.".into())
    }

    fn list_tools(&self) -> Vec<Tool> {
        vec![Tool {
            name: "echo".into(),
            title: Some("Echo".into()),
            description: Some("Echo the supplied text back".into()),
            input_schema: json!({
                "type": "object",
                "properties": { "text": { "type": "string" } },
                "required": ["text"],
                "additionalProperties": false
            }),
            output_schema: None,
            annotations: None,
        }]
    }

    fn list_resources(&self) -> Vec<Resource> {
        vec![Resource {
            uri: "file:///fixture/readme.md".into(),
            name: "readme.md".into(),
            title: None,
            description: None,
            mime_type: Some("text/markdown".into()),
            size: None,
        }]
    }

    fn list_prompts(&self) -> Vec<Prompt> {
        vec![Prompt {
            name: "greet".into(),
            title: None,
            description: Some("Greet someone".into()),
            arguments: Some(vec![PromptArgument {
                name: "who".into(),
                description: None,
                required: Some(true),
            }]),
        }]
    }

    fn read_resource(&self, uri: &str) -> McpResult<Vec<ResourceContents>> {
        if uri == "file:///fixture/readme.md" {
            Ok(vec![ResourceContents {
                uri: uri.into(),
                mime_type: Some("text/markdown".into()),
                text: Some("# Fixture".into()),
                blob: None,
            }])
        } else {
            Err(McpError::ResourceNotFound(uri.into()))
        }
    }

    fn get_prompt(&self, name: &str, args: Option<Value>) -> McpResult<GetPromptResult> {
        if name != "greet" {
            return Err(McpError::PromptNotFound(name.into()));
        }
        let who = args
            .as_ref()
            .and_then(|a| a.get("who"))
            .and_then(Value::as_str)
            .unwrap_or("world");
        Ok(GetPromptResult {
            result_type: "complete".into(),
            description: Some("Greet someone".into()),
            messages: vec![PromptMessage {
                role: "user".into(),
                content: Content::Text {
                    text: format!("Hello, {who}!"),
                },
            }],
        })
    }

    fn call_tool(&self, _name: &str, args: Value) -> CallToolResult {
        match args.get("text").and_then(Value::as_str) {
            Some(text) => CallToolResult::text(text),
            None => CallToolResult::error("missing \"text\" argument"),
        }
    }
}
