//! MCP method dispatch over JSON-RPC 2.0.

use crate::jsonrpc::{
    JsonRpcError, JsonRpcRequest, JsonRpcResponse, INTERNAL_ERROR, INVALID_PARAMS, INVALID_REQUEST,
    METHOD_NOT_FOUND, PARSE_ERROR, UNSUPPORTED_PROTOCOL_VERSION,
};
use crate::types::{
    CallToolResult, DiscoverMeta, DiscoverResult, InitializeResult, ListPromptsResult,
    ListResourcesResult, ListToolsResult, ReadResourceResult, MCP_LEGACY_PROTOCOL_VERSION,
    MCP_PROTOCOL_VERSION, RESULT_TYPE_COMPLETE,
};
use crate::{McpError, McpService};
use serde_json::{json, Value};
use std::sync::Arc;

/// Routes JSON-RPC messages to an [`McpService`].
pub struct McpDispatcher<S: McpService> {
    service: Arc<S>,
    legacy_initialize: bool,
}

impl<S: McpService> Clone for McpDispatcher<S> {
    fn clone(&self) -> Self {
        Self {
            service: self.service.clone(),
            legacy_initialize: self.legacy_initialize,
        }
    }
}

impl<S: McpService> McpDispatcher<S> {
    pub fn new(service: Arc<S>) -> Self {
        Self {
            service,
            legacy_initialize: false,
        }
    }

    /// Also answer the legacy `initialize` handshake of MCP `2025-11-25`,
    /// making this a dual-era server.
    ///
    /// The modern path is unaffected: a request carrying per-request `_meta` is
    /// still served according to [`MCP_PROTOCOL_VERSION`]. This only adds a
    /// reply for clients that open with `initialize`. Off by default, because a
    /// modern-only server SHOULD reject `initialize` while naming the versions
    /// it does support — which is what [`Self::new`] produces.
    pub fn with_legacy_initialize(mut self, enabled: bool) -> Self {
        self.legacy_initialize = enabled;
        self
    }

    /// Build the `server/discover` result. Servers MUST implement this method.
    pub fn discover(&self) -> DiscoverResult {
        DiscoverResult {
            result_type: RESULT_TYPE_COMPLETE.into(),
            supported_versions: self.supported_versions(),
            capabilities: self.service.capabilities(),
            meta: Some(DiscoverMeta {
                server_info: self.service.server_info(),
            }),
            instructions: self.service.instructions(),
            ttl_ms: None,
            cache_scope: None,
        }
    }

    fn supported_versions(&self) -> Vec<String> {
        let mut versions = vec![MCP_PROTOCOL_VERSION.to_string()];
        if self.legacy_initialize {
            versions.push(MCP_LEGACY_PROTOCOL_VERSION.to_string());
        }
        versions
    }

    /// Handle one decoded JSON-RPC message.
    ///
    /// Returns `None` for notifications, which MUST NOT be answered.
    pub fn handle_request(&self, request: &JsonRpcRequest) -> Option<JsonRpcResponse> {
        if request.jsonrpc != "2.0" {
            let id = request.id.clone().unwrap_or(Value::Null);
            return Some(JsonRpcResponse::failure(
                id,
                JsonRpcError::new(INVALID_REQUEST, "jsonrpc must be exactly \"2.0\""),
            ));
        }

        if request.is_notification() {
            self.handle_notification(&request.method);
            return None;
        }

        // Present but null is still a request, and is answered with id null.
        let id = request.id.clone().unwrap_or(Value::Null);

        if let Some(error) = self.check_protocol_version(request) {
            return Some(JsonRpcResponse::failure(id, error));
        }

        match self.route(&request.method, request) {
            Ok(result) => Some(JsonRpcResponse::success(id, result)),
            Err(error) => Some(JsonRpcResponse::failure(id, error)),
        }
    }

    /// Notifications are accepted and produce no reply. Unknown ones are
    /// ignored rather than answered, as JSON-RPC 2.0 requires.
    fn handle_notification(&self, method: &str) {
        if method == "notifications/cancelled" || method == "notifications/initialized" {
            // No server-side state to unwind: dispatch is stateless.
        }
    }

    /// Reject a request whose declared `_meta` protocol version is not served.
    ///
    /// Absent metadata is accepted: `_meta` is required of clients, but
    /// rejecting its absence would break the `server/discover` probe that
    /// dual-era clients use, and the spec makes version checking per-request.
    fn check_protocol_version(&self, request: &JsonRpcRequest) -> Option<JsonRpcError> {
        let requested = request
            .params_object()?
            .get("_meta")?
            .as_object()?
            .get(crate::types::FIELD_PROTOCOL_VERSION)?
            .as_str()?
            .to_string();

        let supported = self.supported_versions();
        if supported.contains(&requested) {
            return None;
        }

        Some(JsonRpcError::with_data(
            UNSUPPORTED_PROTOCOL_VERSION,
            "Unsupported protocol version",
            json!({ "supported": supported, "requested": requested }),
        ))
    }

    fn route(&self, method: &str, request: &JsonRpcRequest) -> Result<Value, JsonRpcError> {
        match method {
            "ping" => Ok(json!({})),
            "server/discover" => to_value(&self.discover()),
            "initialize" => self.handle_initialize(),
            "tools/list" => to_value(&ListToolsResult {
                result_type: RESULT_TYPE_COMPLETE.into(),
                tools: self.service.list_tools(),
                next_cursor: None,
            }),
            "tools/call" => self.handle_tools_call(request),
            "resources/list" => to_value(&ListResourcesResult {
                result_type: RESULT_TYPE_COMPLETE.into(),
                resources: self.service.list_resources(),
                next_cursor: None,
            }),
            "resources/read" => self.handle_resources_read(request),
            "prompts/list" => to_value(&ListPromptsResult {
                result_type: RESULT_TYPE_COMPLETE.into(),
                prompts: self.service.list_prompts(),
                next_cursor: None,
            }),
            "prompts/get" => self.handle_prompts_get(request),
            other => Err(JsonRpcError::new(
                METHOD_NOT_FOUND,
                format!("Method not found: {other}"),
            )),
        }
    }

    fn handle_initialize(&self) -> Result<Value, JsonRpcError> {
        if !self.legacy_initialize {
            // A modern-only server SHOULD name the versions it supports in the
            // error, because legacy clients have no fall-forward mechanism and
            // this message may be their only diagnostic.
            return Err(JsonRpcError::with_data(
                METHOD_NOT_FOUND,
                format!(
                    "Method not found: initialize. This server speaks MCP {MCP_PROTOCOL_VERSION}, \
                     which replaces the initialize handshake with per-request _meta. \
                     Call server/discover instead."
                ),
                json!({ "supported": self.supported_versions() }),
            ));
        }

        to_value(&InitializeResult {
            protocol_version: MCP_LEGACY_PROTOCOL_VERSION.into(),
            capabilities: self.service.capabilities(),
            server_info: self.service.server_info(),
            instructions: self.service.instructions(),
        })
    }

    fn handle_tools_call(&self, request: &JsonRpcRequest) -> Result<Value, JsonRpcError> {
        let name = required_string(request, "name")?;
        let arguments = optional_value(request, "arguments").unwrap_or_else(|| json!({}));

        // An unknown tool is a protocol error, not a tool execution error.
        if !self.service.list_tools().iter().any(|t| t.name == name) {
            return Err(JsonRpcError::new(
                INVALID_PARAMS,
                format!("Unknown tool: {name}"),
            ));
        }

        to_value(&self.service.call_tool(&name, arguments))
    }

    fn handle_resources_read(&self, request: &JsonRpcRequest) -> Result<Value, JsonRpcError> {
        let uri = required_string(request, "uri")?;
        let contents = self.service.read_resource(&uri).map_err(mcp_error)?;

        // An empty contents array is ambiguous and MUST NOT stand in for a
        // missing resource.
        if contents.is_empty() {
            return Err(JsonRpcError::with_data(
                INVALID_PARAMS,
                "Resource not found",
                json!({ "uri": uri }),
            ));
        }

        to_value(&ReadResourceResult {
            result_type: RESULT_TYPE_COMPLETE.into(),
            contents,
        })
    }

    fn handle_prompts_get(&self, request: &JsonRpcRequest) -> Result<Value, JsonRpcError> {
        let name = required_string(request, "name")?;
        let arguments = optional_value(request, "arguments");
        let result = self
            .service
            .get_prompt(&name, arguments)
            .map_err(mcp_error)?;
        to_value(&result)
    }

    /// Handle one raw newline-delimited JSON line.
    ///
    /// Returns the response line to write, or `None` for a notification.
    pub fn handle_line(&self, line: &str) -> Option<String> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        let value: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                return Some(crate::jsonrpc::to_line(&JsonRpcResponse::failure(
                    Value::Null,
                    JsonRpcError::new(PARSE_ERROR, format!("Parse error: {e}")),
                )))
            }
        };

        // Batches are not part of MCP: every message is a single object.
        let request: JsonRpcRequest = match serde_json::from_value(value.clone()) {
            Ok(r) => r,
            Err(e) => {
                let id = value.get("id").cloned().unwrap_or(Value::Null);
                return Some(crate::jsonrpc::to_line(&JsonRpcResponse::failure(
                    id,
                    JsonRpcError::new(INVALID_REQUEST, format!("Invalid Request: {e}")),
                )));
            }
        };

        // `serde_json` cannot distinguish an absent `id` from `"id": null`, so
        // consult the raw object: only an absent key is a notification.
        let has_id_key = value.get("id").is_some();
        let request = JsonRpcRequest {
            id: if has_id_key {
                Some(request.id.unwrap_or(Value::Null))
            } else {
                None
            },
            ..request
        };

        self.handle_request(&request)
            .map(|r| crate::jsonrpc::to_line(&r))
    }
}

fn to_value<T: serde::Serialize>(value: &T) -> Result<Value, JsonRpcError> {
    serde_json::to_value(value)
        .map_err(|e| JsonRpcError::new(INTERNAL_ERROR, format!("Internal error: {e}")))
}

fn required_string(request: &JsonRpcRequest, key: &str) -> Result<String, JsonRpcError> {
    match request.params_object().and_then(|p| p.get(key)) {
        Some(Value::String(s)) => Ok(s.clone()),
        Some(_) => Err(JsonRpcError::new(
            INVALID_PARAMS,
            format!("Invalid params: \"{key}\" must be a string"),
        )),
        None => Err(JsonRpcError::new(
            INVALID_PARAMS,
            format!("Invalid params: missing \"{key}\""),
        )),
    }
}

fn optional_value(request: &JsonRpcRequest, key: &str) -> Option<Value> {
    request.params_object().and_then(|p| p.get(key)).cloned()
}

fn mcp_error(error: McpError) -> JsonRpcError {
    match error {
        // The spec assigns -32602 to a missing resource, with -32002 accepted
        // only for backward compatibility on the client side.
        McpError::ResourceNotFound(uri) => {
            JsonRpcError::with_data(INVALID_PARAMS, "Resource not found", json!({ "uri": uri }))
        }
        McpError::PromptNotFound(name) => {
            JsonRpcError::with_data(INVALID_PARAMS, "Prompt not found", json!({ "name": name }))
        }
        McpError::ToolNotFound(name) => {
            JsonRpcError::new(INVALID_PARAMS, format!("Unknown tool: {name}"))
        }
        McpError::MethodNotFound(m) => {
            JsonRpcError::new(METHOD_NOT_FOUND, format!("Method not found: {m}"))
        }
        McpError::InvalidParams(m) => {
            JsonRpcError::new(INVALID_PARAMS, format!("Invalid params: {m}"))
        }
        McpError::Internal(m) => JsonRpcError::new(INTERNAL_ERROR, format!("Internal error: {m}")),
    }
}

/// Convenience constructor for a dispatcher over a service.
pub fn dispatch_mcp<S: McpService>(service: Arc<S>) -> McpDispatcher<S> {
    McpDispatcher::new(service)
}

/// The result of a tool call, for callers that want the tool-execution-error
/// convention spelled out.
pub type ToolOutcome = CallToolResult;
