//! MCP types — Tool, Resource, Prompt, Content
//!
//! Prototype models pending validation against a pinned MCP schema.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Protocol revision targeted by the recovery work; conformance is not claimed.
pub const MCP_PROTOCOL_VERSION: &str = "2026-07-28";

/// Reserved namespace for MCP metadata fields
pub const MCP_NS: &str = "io.modelcontextprotocol";

/// Field names within `_meta` (the per-request metadata envelope)
pub const FIELD_PROTOCOL_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
pub const FIELD_CLIENT_INFO: &str = "io.modelcontextprotocol/clientInfo";
pub const FIELD_CLIENT_CAPABILITIES: &str = "io.modelcontextprotocol/clientCapabilities";
pub const FIELD_SERVER_INFO: &str = "io.modelcontextprotocol/serverInfo";
pub const FIELD_SUBSCRIPTION_ID: &str = "io.modelcontextprotocol/subscriptionId";

/// Server identity advertised in `server/discover`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Client identity declared on every request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Client capabilities — which client primitives the client supports
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClientCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elicitation: Option<Capability>,
    #[serde(flatten)]
    pub other: serde_json::Map<String, Value>,
}

/// Marker struct for capabilities that take no configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Capability {}

/// Server capabilities — which server primitives the server exposes
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<Capability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<Capability>,
    #[serde(flatten)]
    pub other: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolsCapability {
    #[serde(rename = "listChanged", skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Discovery response payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoverResponse {
    #[serde(rename = "resultType")]
    pub result_type: String,
    #[serde(rename = "supportedVersions")]
    pub supported_versions: Vec<String>,
    pub capabilities: ServerCapabilities,
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<DiscoverMeta>,
    #[serde(rename = "ttlMs", skip_serializing_if = "Option::is_none")]
    pub ttl_ms: Option<u64>,
    #[serde(rename = "cacheScope", skip_serializing_if = "Option::is_none")]
    pub cache_scope: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoverMeta {
    #[serde(rename = "io.modelcontextprotocol/serverInfo")]
    pub server_info: ServerInfo,
}

/// Per-request `_meta` envelope
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RequestMeta {
    #[serde(
        rename = "io.modelcontextprotocol/protocolVersion",
        skip_serializing_if = "Option::is_none"
    )]
    pub protocol_version: Option<String>,
    #[serde(
        rename = "io.modelcontextprotocol/clientInfo",
        skip_serializing_if = "Option::is_none"
    )]
    pub client_info: Option<ClientInfo>,
    #[serde(
        rename = "io.modelcontextprotocol/clientCapabilities",
        skip_serializing_if = "Option::is_none"
    )]
    pub client_capabilities: Option<ClientCapabilities>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

/// Tool definition — executable function an AI can invoke
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Value>,
}

/// Resource — a data source the AI can read
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub uri: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// Prompt — reusable interaction template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prompt {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<PromptArgument>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptArgument {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

/// Result content for tool calls
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Content {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
    #[serde(rename = "resource")]
    Resource { resource: Value },
}

/// Tool call response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallToolResponse {
    #[serde(rename = "resultType")]
    pub result_type: String,
    pub content: Vec<Content>,
    #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// List response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResponse<T> {
    #[serde(rename = "resultType")]
    pub result_type: String,
    pub items: Vec<T>,
    #[serde(rename = "nextCursor", skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

impl Default for DiscoverResponse {
    fn default() -> Self {
        Self {
            result_type: "complete".into(),
            supported_versions: vec![MCP_PROTOCOL_VERSION.into()],
            capabilities: ServerCapabilities::default(),
            meta: None,
            ttl_ms: Some(3600000),
            cache_scope: Some("public".into()),
        }
    }
}

impl CallToolResponse {
    pub fn text(s: impl Into<String>) -> Self {
        Self {
            result_type: "complete".into(),
            content: vec![Content::Text { text: s.into() }],
            is_error: None,
        }
    }

    pub fn error(s: impl Into<String>) -> Self {
        Self {
            result_type: "complete".into(),
            content: vec![Content::Text { text: s.into() }],
            is_error: Some(true),
        }
    }
}

impl Default for Content {
    fn default() -> Self {
        Content::Text {
            text: String::new(),
        }
    }
}
