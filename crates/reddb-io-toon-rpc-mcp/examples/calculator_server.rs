//! A calculator MCP server on stdio: one `add` tool, answering in TOON text.
//!
//! Register it with an MCP host as a stdio server, e.g.
//! `cargo run -p reddb-io-toon-rpc-mcp --example calculator_server`.

use reddb_io_toon_rpc_mcp::{
    serve_stdio, CallToolResult, Implementation, JsonObject, McpError, McpServer, McpService, Tool,
};
use serde_json::json;

struct Calculator;

impl McpService for Calculator {
    fn server_info(&self) -> Implementation {
        Implementation {
            name: "calculator".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            title: Some("Calculator".into()),
        }
    }

    fn tools_enabled(&self) -> bool {
        true
    }

    fn list_tools(&self) -> Vec<Tool> {
        vec![Tool {
            name: "add".into(),
            title: Some("Add".into()),
            description: Some("Add two numbers.".into()),
            input_schema: json!({
                "type": "object",
                "properties": { "a": { "type": "number" }, "b": { "type": "number" } },
                "required": ["a", "b"]
            }),
            output_schema: None,
            annotations: None,
        }]
    }

    fn call_tool(&self, name: &str, arguments: &JsonObject) -> Result<CallToolResult, McpError> {
        if name != "add" {
            return Err(McpError::unknown_tool(name));
        }
        let number = |key| arguments.get(key).and_then(serde_json::Value::as_f64);
        Ok(match (number("a"), number("b")) {
            (Some(a), Some(b)) => CallToolResult::toon(&json!({ "sum": a + b })),
            _ => CallToolResult::error("a and b must be numbers"),
        })
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    serve_stdio(&McpServer::new(Calculator)).await
}
