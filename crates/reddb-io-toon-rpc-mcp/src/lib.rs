//! TOON-RPC implementation of the Model Context Protocol (MCP)
//!
//! MCP is JSON-RPC 2.0 over stdio / Streamable HTTP. This crate provides:
//!
//! - MCP-specific types (Tool, Resource, Prompt, Content)
//! - [`McpService`] trait for implementing MCP servers
//! - A [`Dispatcher`] adapter that registers all MCP methods automatically
//! - stdio transport compatible with Claude Desktop / Claude Code
//! - Streamable HTTP transport (POST + SSE)
//!
//! # Example
//!
//! ```no_run
//! use reddb_io_toon_rpc_mcp::{McpService, McpDispatcher, serve_stdio};
//!
//! struct MyServer;
//!
//! impl McpService for MyServer {
//!     fn server_info(&self) -> reddb_io_toon_rpc_mcp::ServerInfo {
//!         reddb_io_toon_rpc_mcp::ServerInfo {
//!             name: "my-server".into(),
//!             version: "1.0.0".into(),
//!             title: None,
//!         }
//!     }
//!     fn list_tools(&self) -> Vec<reddb_io_toon_rpc_mcp::Tool> { vec![] }
//!     fn call_tool(&self, _name: &str, _args: serde_json::Value) -> reddb_io_toon_rpc_mcp::CallToolResponse {
//!         reddb_io_toon_rpc_mcp::CallToolResponse::error("not implemented")
//!     }
//! }
//!
//! fn main() {
//!     serve_stdio(MyServer).unwrap();
//! }
//! ```

pub mod dispatcher;
pub mod http;
pub mod stdio;
pub mod types;

pub use dispatcher::{McpDispatcher, dispatch_mcp};
pub use http::serve_streamable_http;
pub use stdio::serve_stdio;
pub use types::{
    CallToolResponse, Capability, ClientCapabilities, ClientInfo, Content, DiscoverMeta,
    DiscoverResponse, Prompt, PromptArgument, RequestMeta, Resource, ServerCapabilities,
    ServerInfo, ToolsCapability, Tool, FIELD_CLIENT_CAPABILITIES, FIELD_CLIENT_INFO,
    FIELD_PROTOCOL_VERSION, FIELD_SERVER_INFO, FIELD_SUBSCRIPTION_ID, MCP_NS,
    MCP_PROTOCOL_VERSION,
};

use serde_json::Value;
use std::sync::Arc;
use thiserror::Error;

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

/// Trait for implementing an MCP server.
///
/// Implement this for your service, then pass it to [`serve_stdio`] or
/// [`serve_streamable_http`].
pub trait McpService: Send + Sync + 'static {
    /// Server identity returned in `server/discover` and per-request `_meta`.
    fn server_info(&self) -> ServerInfo;

    /// Optional override of capabilities. Defaults to the primitives whose
    /// `list_*` methods return non-empty results (heuristic).
    fn capabilities(&self) -> ServerCapabilities {
        let mut caps = ServerCapabilities::default();
        if !self.list_tools().is_empty() {
            caps.tools = Some(ToolsCapability {
                list_changed: Some(false),
            });
        }
        if !self.list_resources().is_empty() {
            caps.resources = Some(Capability::default());
        }
        if !self.list_prompts().is_empty() {
            caps.prompts = Some(Capability::default());
        }
        caps
    }

    /// List available tools. Default: empty.
    fn list_tools(&self) -> Vec<Tool> {
        vec![]
    }

    /// List available resources. Default: empty.
    fn list_resources(&self) -> Vec<Resource> {
        vec![]
    }

    /// List available prompts. Default: empty.
    fn list_prompts(&self) -> Vec<Prompt> {
        vec![]
    }

    /// Read a resource by URI. Default: not found.
    fn read_resource(&self, _uri: &str) -> McpResult<Value> {
        Err(McpError::ResourceNotFound("not implemented".into()))
    }

    /// Get a prompt template.
    fn get_prompt(&self, _name: &str, _args: Option<Value>) -> McpResult<Value> {
        Err(McpError::PromptNotFound("not implemented".into()))
    }

    /// Invoke a tool by name with JSON arguments.
    fn call_tool(&self, name: &str, args: Value) -> CallToolResponse;
}

/// Shared handle for an MCP service.
pub type SharedMcp<S> = Arc<S>;
