//! A Model Context Protocol server for MCP revision `2026-07-28`.
//!
//! # Protocol pin
//!
//! This crate targets exactly one revision, [`MCP_PROTOCOL_VERSION`]. That
//! revision carries the protocol version, client identity, and client
//! capabilities as per-request `_meta` and requires servers to implement
//! `server/discover`; it replaces the `initialize` handshake used by
//! `2025-11-25` and earlier. A dual-era server that also answers `initialize`
//! is available through [`McpDispatcher::with_legacy_initialize`].
//!
//! # Wire format
//!
//! MCP is plain JSON-RPC 2.0. Per Spec #389 §9, TOON and TOON-RPC extensions
//! are never presented as MCP wire, so this crate depends on `serde_json` only
//! and shares no codec with the TOON-RPC crates.
//!
//! # Transports
//!
//! - [`serve_stdio`] — the stdio binding: one JSON-RPC message per line.
//! - [`serve_http_post`] — `POST /mcp` request/response only. This is a subset
//!   of Streamable HTTP with no SSE; see the [`http`] module docs.
//!
//! # Example
//!
//! ```no_run
//! use reddb_io_toon_rpc_mcp::{CallToolResult, McpService, ServerInfo, Tool, serve_stdio};
//! use serde_json::{json, Value};
//!
//! struct Echo;
//!
//! impl McpService for Echo {
//!     fn server_info(&self) -> ServerInfo {
//!         ServerInfo { name: "echo".into(), version: "1.0.0".into(), title: None }
//!     }
//!
//!     fn list_tools(&self) -> Vec<Tool> {
//!         vec![Tool::new("echo", json!({
//!             "type": "object",
//!             "properties": { "text": { "type": "string" } },
//!             "required": ["text"]
//!         }))]
//!     }
//!
//!     fn call_tool(&self, _name: &str, args: Value) -> CallToolResult {
//!         match args.get("text").and_then(Value::as_str) {
//!             Some(text) => CallToolResult::text(text),
//!             None => CallToolResult::error("missing \"text\" argument"),
//!         }
//!     }
//! }
//!
//! fn main() -> std::io::Result<()> {
//!     serve_stdio(Echo)
//! }
//! ```

pub mod dispatcher;
pub mod http;
pub mod jsonrpc;
pub mod stdio;
pub mod types;

pub use dispatcher::{dispatch_mcp, McpDispatcher};
pub use http::serve_http_post;
pub use jsonrpc::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
pub use stdio::{serve_stdio, serve_stdio_with};
pub use types::{
    CallToolResult, Capability, ClientCapabilities, ClientInfo, Content, DiscoverMeta,
    DiscoverResult, GetPromptResult, InitializeResult, ListPromptsResult, ListResourcesResult,
    ListToolsResult, Prompt, PromptArgument, PromptMessage, PromptsCapability, ReadResourceResult,
    RequestMeta, Resource, ResourceContents, ResourcesCapability, ServerCapabilities, ServerInfo,
    Tool, ToolsCapability, FIELD_CLIENT_CAPABILITIES, FIELD_CLIENT_INFO, FIELD_PROTOCOL_VERSION,
    FIELD_SERVER_INFO, FIELD_SUBSCRIPTION_ID, MCP_LEGACY_PROTOCOL_VERSION, MCP_NS,
    MCP_PROTOCOL_VERSION,
};

use serde_json::Value;
use std::sync::Arc;
use thiserror::Error;

/// Failures a service can report. The dispatcher maps each to the JSON-RPC
/// error code the spec assigns it.
#[derive(Debug, Error)]
pub enum McpError {
    #[error("method not found: {0}")]
    MethodNotFound(String),
    #[error("tool not found: {0}")]
    ToolNotFound(String),
    #[error("resource not found: {0}")]
    ResourceNotFound(String),
    #[error("prompt not found: {0}")]
    PromptNotFound(String),
    #[error("invalid params: {0}")]
    InvalidParams(String),
    #[error("internal error: {0}")]
    Internal(String),
}

pub type McpResult<T> = Result<T, McpError>;

/// A Model Context Protocol server.
///
/// Implement this, then serve it with [`serve_stdio`] or [`serve_http_post`].
pub trait McpService: Send + Sync + 'static {
    /// Server identity, reported in `server/discover`. Self-reported and
    /// unverified: clients use it for display, not for security decisions.
    fn server_info(&self) -> ServerInfo;

    /// Optional natural-language guidance for models using this server.
    fn instructions(&self) -> Option<String> {
        None
    }

    /// Capabilities to advertise. The default declares each primitive whose
    /// `list_*` method returns a non-empty set.
    fn capabilities(&self) -> ServerCapabilities {
        let mut caps = ServerCapabilities::default();
        if !self.list_tools().is_empty() {
            caps.tools = Some(ToolsCapability {
                list_changed: Some(false),
            });
        }
        if !self.list_resources().is_empty() {
            caps.resources = Some(ResourcesCapability::default());
        }
        if !self.list_prompts().is_empty() {
            caps.prompts = Some(PromptsCapability::default());
        }
        caps
    }

    /// Tools this server exposes. The set must not vary per connection, and
    /// should be returned in a stable order so clients can cache it.
    fn list_tools(&self) -> Vec<Tool> {
        vec![]
    }

    fn list_resources(&self) -> Vec<Resource> {
        vec![]
    }

    fn list_prompts(&self) -> Vec<Prompt> {
        vec![]
    }

    /// Read a resource. Returning an empty vector is treated as "not found",
    /// because an empty `contents` array is ambiguous on the wire.
    fn read_resource(&self, uri: &str) -> McpResult<Vec<ResourceContents>> {
        Err(McpError::ResourceNotFound(uri.to_string()))
    }

    fn get_prompt(&self, name: &str, _args: Option<Value>) -> McpResult<GetPromptResult> {
        Err(McpError::PromptNotFound(name.to_string()))
    }

    /// Invoke a tool.
    ///
    /// Failures the model can act on — bad input, business rules, upstream
    /// errors — belong in [`CallToolResult::error`], which returns a normal
    /// result with `isError: true`. The dispatcher raises a JSON-RPC error for
    /// protocol-level problems such as an unknown tool.
    fn call_tool(&self, name: &str, args: Value) -> CallToolResult;
}

/// Shared handle for an MCP service.
pub type SharedMcp<S> = Arc<S>;
