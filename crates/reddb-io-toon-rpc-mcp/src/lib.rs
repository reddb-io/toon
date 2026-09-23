//! A Model Context Protocol server, pinned to the official 2025-06-18 schema.
//!
//! MCP is JSON-RPC 2.0, not TOON-RPC: messages are JSON objects, IDs are
//! strings or numbers (never null), params are objects, and this revision has
//! no batching. Before `initialize` only `ping` is answered; feature requests
//! are accepted once `initialize` is (the client's `notifications/initialized`
//! needs no answer). The server serves the tools, resources and prompts
//! features for whichever of them the service provides. TOON appears only as
//! an optional encoding of text content (`toon_content`). Mirrors
//! `@reddb-io/toon-rpc-mcp`; both run `tests/corpus/mcp/transcript.json`.

use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

mod stdio;
pub use stdio::{serve_lines, serve_stdio};

pub const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

pub type JsonObject = Map<String, Value>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Implementation {
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Value>,
}

/// One content block; `text` is the only kind this crate builds, but a
/// service may return any block the schema defines.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Content {
    Text {
        text: String,
    },
    #[serde(untagged)]
    Other(JsonObject),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallToolResult {
    pub content: Vec<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structured_content: Option<JsonObject>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

impl CallToolResult {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![text_content(text)],
            structured_content: None,
            is_error: None,
        }
    }

    /// A tool-level failure the model should see, not a protocol error.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            is_error: Some(true),
            ..Self::text(message)
        }
    }

    /// `value` as TOON text, and as structured content when it is an object.
    pub fn toon(value: &Value) -> Self {
        Self {
            content: vec![toon_content(value)],
            structured_content: value.as_object().cloned(),
            is_error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    pub uri: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceContents {
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Base64 binary contents, for a resource that is not text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadResourceResult {
    pub contents: Vec<ResourceContents>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptArgument {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Prompt {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<PromptArgument>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptMessage {
    pub role: Role,
    pub content: Content,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetPromptResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub messages: Vec<PromptMessage>,
}

/// An error answered as a JSON-RPC error object.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{message}")]
pub struct McpError {
    pub code: i32,
    pub message: String,
    pub data: Option<Value>,
}

impl McpError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(INVALID_PARAMS, message)
    }

    pub fn unknown_tool(name: &str) -> Self {
        Self::invalid_params(format!("Unknown tool: {name}"))
    }

    pub fn resource_not_found(uri: &str) -> Self {
        Self {
            data: Some(json!({ "uri": uri })),
            ..Self::new(RESOURCE_NOT_FOUND, "Resource not found")
        }
    }
}

/// One text content block.
pub fn text_content(text: impl Into<String>) -> Content {
    Content::Text { text: text.into() }
}

/// A value rendered as TOON inside a text content block.
pub fn toon_content(value: &Value) -> Content {
    let toon = reddb_io_toon::Value::from_json_value(value.clone());
    // Every JSON value has a TOON encoding; fall back to JSON regardless.
    let text = reddb_io_toon::encode(&toon).unwrap_or_else(|_| value.to_string());
    text_content(text)
}

/// What a server offers. A feature is served and advertised in the
/// capabilities only when its `*_enabled` method returns true.
pub trait McpService: Send + Sync {
    fn server_info(&self) -> Implementation;

    fn instructions(&self) -> Option<String> {
        None
    }

    fn tools_enabled(&self) -> bool {
        false
    }
    fn list_tools(&self) -> Vec<Tool> {
        Vec::new()
    }
    /// Return `McpError::unknown_tool` for a tool that does not exist.
    fn call_tool(&self, name: &str, _arguments: &JsonObject) -> Result<CallToolResult, McpError> {
        Err(McpError::unknown_tool(name))
    }

    fn resources_enabled(&self) -> bool {
        false
    }
    fn list_resources(&self) -> Vec<Resource> {
        Vec::new()
    }
    /// Return `McpError::resource_not_found` for a resource that does not exist.
    fn read_resource(&self, uri: &str) -> Result<ReadResourceResult, McpError> {
        Err(McpError::resource_not_found(uri))
    }

    fn prompts_enabled(&self) -> bool {
        false
    }
    fn list_prompts(&self) -> Vec<Prompt> {
        Vec::new()
    }
    /// Return `McpError::invalid_params` for an unknown prompt or a missing argument.
    fn get_prompt(
        &self,
        name: &str,
        _arguments: &Map<String, Value>,
    ) -> Result<GetPromptResult, McpError> {
        Err(McpError::invalid_params(format!("Unknown prompt: {name}")))
    }
}

const PARSE_ERROR: i32 = -32700;
const INVALID_REQUEST: i32 = -32600;
const METHOD_NOT_FOUND: i32 = -32601;
const INVALID_PARAMS: i32 = -32602;
const INTERNAL_ERROR: i32 = -32603;
const RESOURCE_NOT_FOUND: i32 = -32002;

/// One MCP session: create one per connection.
pub struct McpServer<S> {
    service: S,
    initialized: AtomicBool,
}

impl<S: McpService> McpServer<S> {
    pub fn new(service: S) -> Self {
        Self {
            service,
            initialized: AtomicBool::new(false),
        }
    }

    /// Answer one line of newline-delimited JSON; `None` when there is none.
    pub fn handle_line(&self, line: &str) -> Option<String> {
        let answer = match serde_json::from_str::<Value>(line) {
            Ok(message) => self.handle_message(message)?,
            Err(_) => error_response(Value::Null, &McpError::new(PARSE_ERROR, "Parse error")),
        };
        Some(answer.to_string())
    }

    /// Answer one decoded message; `None` for a notification or a response.
    pub fn handle_message(&self, message: Value) -> Option<Value> {
        let invalid = || {
            Some(error_response(
                Value::Null,
                &McpError::new(INVALID_REQUEST, "Invalid Request"),
            ))
        };
        let Value::Object(mut message) = message else {
            return invalid();
        };
        if message.get("jsonrpc") != Some(&json!("2.0")) {
            return invalid();
        }
        // A response to a request this server never sends is ignored.
        if !message.contains_key("method")
            && (message.contains_key("result") || message.contains_key("error"))
        {
            return None;
        }
        let id = message.remove("id");
        let params = message.remove("params");
        let (Some(Value::String(method)), true, true) = (
            message.remove("method"),
            matches!(id, None | Some(Value::String(_) | Value::Number(_))),
            matches!(params, None | Some(Value::Object(_))),
        ) else {
            return invalid();
        };
        // Notifications (`notifications/initialized`, `notifications/cancelled`)
        // need no answer, and none changes what this server does.
        let id = id?;
        let params = match params {
            Some(Value::Object(params)) => params,
            _ => JsonObject::new(),
        };
        Some(match self.dispatch(&method, &params) {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(error) => error_response(id, &error),
        })
    }

    fn dispatch(&self, method: &str, params: &JsonObject) -> Result<Value, McpError> {
        match method {
            "ping" => return Ok(json!({})),
            "initialize" => return Ok(self.initialize()),
            _ if !self.initialized.load(Ordering::SeqCst) => {
                return Err(McpError::new(INVALID_REQUEST, "Server not initialized"))
            }
            _ => {}
        }
        let service = &self.service;
        let result = match method {
            "tools/list" if service.tools_enabled() => json!({ "tools": service.list_tools() }),
            "tools/call" if service.tools_enabled() => {
                let name = required_str(params, "name")?;
                let arguments = match params.get("arguments") {
                    None => JsonObject::new(),
                    Some(Value::Object(arguments)) => arguments.clone(),
                    Some(_) => return Err(McpError::invalid_params("arguments must be an object")),
                };
                to_value(service.call_tool(name, &arguments)?)?
            }
            "resources/list" if service.resources_enabled() => {
                json!({ "resources": service.list_resources() })
            }
            "resources/read" if service.resources_enabled() => {
                to_value(service.read_resource(required_str(params, "uri")?)?)?
            }
            "prompts/list" if service.prompts_enabled() => {
                json!({ "prompts": service.list_prompts() })
            }
            "prompts/get" if service.prompts_enabled() => {
                let name = required_str(params, "name")?;
                let arguments = match params.get("arguments") {
                    None => JsonObject::new(),
                    Some(Value::Object(arguments)) if arguments.values().all(Value::is_string) => {
                        arguments.clone()
                    }
                    Some(_) => {
                        return Err(McpError::invalid_params(
                            "arguments must map names to strings",
                        ))
                    }
                };
                to_value(service.get_prompt(name, &arguments)?)?
            }
            _ => return Err(McpError::new(METHOD_NOT_FOUND, "Method not found")),
        };
        Ok(result)
    }

    fn initialize(&self) -> Value {
        self.initialized.store(true, Ordering::SeqCst);
        let service = &self.service;
        let mut capabilities = JsonObject::new();
        if service.tools_enabled() {
            capabilities.insert("tools".into(), json!({ "listChanged": false }));
        }
        if service.resources_enabled() {
            capabilities.insert(
                "resources".into(),
                json!({ "subscribe": false, "listChanged": false }),
            );
        }
        if service.prompts_enabled() {
            capabilities.insert("prompts".into(), json!({ "listChanged": false }));
        }
        // This server speaks one revision; a client asking for another decides
        // from this answer whether it can continue.
        let mut result = json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": capabilities,
            "serverInfo": service.server_info(),
        });
        if let Some(instructions) = service.instructions() {
            result["instructions"] = Value::String(instructions);
        }
        result
    }
}

fn required_str<'a>(params: &'a JsonObject, key: &str) -> Result<&'a str, McpError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| McpError::invalid_params(format!("{key} must be a string")))
}

fn to_value(result: impl Serialize) -> Result<Value, McpError> {
    serde_json::to_value(result).map_err(|_| McpError::new(INTERNAL_ERROR, "Internal error"))
}

fn error_response(id: Value, error: &McpError) -> Value {
    let mut body = json!({ "code": error.code, "message": error.message });
    if let Some(data) = &error.data {
        body["data"] = data.clone();
    }
    json!({ "jsonrpc": "2.0", "id": id, "error": body })
}
