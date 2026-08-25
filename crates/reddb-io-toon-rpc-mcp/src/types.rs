//! MCP data types for the pinned protocol revision.
//!
//! Field names and result shapes follow the official schema for
//! [`MCP_PROTOCOL_VERSION`]; see `docs/mcp-conformance.md` for the per-method
//! citations. Every wire key is spelled exactly as the schema spells it.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The MCP protocol revision this crate implements.
///
/// `2026-07-28` is the *current* revision: it replaces the `initialize`
/// handshake of `2025-11-25` and earlier with per-request `_meta` and a
/// mandatory `server/discover`.
pub const MCP_PROTOCOL_VERSION: &str = "2026-07-28";

/// Legacy revisions that negotiate via an `initialize` handshake. Served only
/// when [`crate::McpDispatcher::with_legacy_initialize`] is enabled.
pub const MCP_LEGACY_PROTOCOL_VERSION: &str = "2025-11-25";

/// Reserved namespace for MCP metadata fields.
pub const MCP_NS: &str = "io.modelcontextprotocol";

/// `_meta` key carrying the protocol version of a request.
pub const FIELD_PROTOCOL_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
/// `_meta` key carrying client identity.
pub const FIELD_CLIENT_INFO: &str = "io.modelcontextprotocol/clientInfo";
/// `_meta` key carrying client capabilities.
pub const FIELD_CLIENT_CAPABILITIES: &str = "io.modelcontextprotocol/clientCapabilities";
/// `_meta` key carrying server identity in a `DiscoverResult`.
pub const FIELD_SERVER_INFO: &str = "io.modelcontextprotocol/serverInfo";
/// `_meta` key correlating subscription notifications.
pub const FIELD_SUBSCRIPTION_ID: &str = "io.modelcontextprotocol/subscriptionId";

/// Server identity. Self-reported and unverified; for display only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Client identity, declared per request in `_meta`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// A capability that carries no configuration, serialized as `{}`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Capability {}

/// Client capabilities declared in `_meta`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ClientCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elicitation: Option<Capability>,
    #[serde(flatten)]
    pub other: serde_json::Map<String, Value>,
}

/// `tools` capability.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ToolsCapability {
    #[serde(
        rename = "listChanged",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub list_changed: Option<bool>,
}

/// `resources` capability. Both features are independently optional.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ResourcesCapability {
    #[serde(
        rename = "listChanged",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub list_changed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscribe: Option<bool>,
}

/// `prompts` capability.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PromptsCapability {
    #[serde(
        rename = "listChanged",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub list_changed: Option<bool>,
}

/// Capabilities a server advertises in `server/discover`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ServerCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsCapability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourcesCapability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompts: Option<PromptsCapability>,
    #[serde(flatten)]
    pub other: serde_json::Map<String, Value>,
}

/// `_meta` of a `DiscoverResult`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoverMeta {
    #[serde(rename = "io.modelcontextprotocol/serverInfo")]
    pub server_info: ServerInfo,
}

/// Result of `server/discover`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoverResult {
    #[serde(rename = "resultType")]
    pub result_type: String,
    #[serde(rename = "supportedVersions")]
    pub supported_versions: Vec<String>,
    pub capabilities: ServerCapabilities,
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<DiscoverMeta>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(rename = "ttlMs", default, skip_serializing_if = "Option::is_none")]
    pub ttl_ms: Option<u64>,
    #[serde(
        rename = "cacheScope",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub cache_scope: Option<String>,
}

/// Per-request `_meta` envelope.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RequestMeta {
    #[serde(
        rename = "io.modelcontextprotocol/protocolVersion",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub protocol_version: Option<String>,
    #[serde(
        rename = "io.modelcontextprotocol/clientInfo",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub client_info: Option<ClientInfo>,
    #[serde(
        rename = "io.modelcontextprotocol/clientCapabilities",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub client_capabilities: Option<ClientCapabilities>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

/// A tool an AI can invoke.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// MUST be a valid JSON Schema object, never `null`.
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
    #[serde(
        rename = "outputSchema",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub output_schema: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Value>,
}

impl Tool {
    /// A tool taking no parameters, using the schema the spec recommends.
    pub fn new(name: impl Into<String>, input_schema: Value) -> Self {
        Self {
            name: name.into(),
            title: None,
            description: None,
            input_schema,
            output_schema: None,
            annotations: None,
        }
    }
}

/// A readable data source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Resource {
    pub uri: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Serialized as `mimeType`, which is the only spelling the schema defines.
    #[serde(rename = "mimeType", default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

/// One entry of a `resources/read` result, carrying text or binary data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceContents {
    pub uri: String,
    #[serde(rename = "mimeType", default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Base64-encoded binary payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
}

impl ResourceContents {
    pub fn text(uri: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            mime_type: None,
            text: Some(text.into()),
            blob: None,
        }
    }
}

/// A reusable interaction template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Prompt {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<PromptArgument>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptArgument {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

/// One message of a `prompts/get` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptMessage {
    /// `"user"` or `"assistant"`.
    pub role: String,
    pub content: Content,
}

/// A content block. The tag is the wire's `type` discriminator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    #[serde(rename = "audio")]
    Audio {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
    #[serde(rename = "resource_link")]
    ResourceLink {
        uri: String,
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(rename = "mimeType", default, skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
    },
    #[serde(rename = "resource")]
    Resource { resource: Value },
}

impl Default for Content {
    fn default() -> Self {
        Content::Text {
            text: String::new(),
        }
    }
}

/// Result of `tools/call`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallToolResult {
    #[serde(rename = "resultType")]
    pub result_type: String,
    pub content: Vec<Content>,
    #[serde(
        rename = "structuredContent",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub structured_content: Option<Value>,
    #[serde(rename = "isError", default, skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

impl CallToolResult {
    pub fn text(s: impl Into<String>) -> Self {
        Self {
            result_type: "complete".into(),
            content: vec![Content::Text { text: s.into() }],
            structured_content: None,
            is_error: None,
        }
    }

    /// A *tool execution* error: a normal result with `isError: true`, which
    /// the model can read and self-correct from. Protocol-level failures use a
    /// JSON-RPC error instead.
    pub fn error(s: impl Into<String>) -> Self {
        Self {
            result_type: "complete".into(),
            content: vec![Content::Text { text: s.into() }],
            structured_content: None,
            is_error: Some(true),
        }
    }
}

/// Result of `tools/list`. The array key is `tools`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListToolsResult {
    #[serde(rename = "resultType")]
    pub result_type: String,
    pub tools: Vec<Tool>,
    #[serde(
        rename = "nextCursor",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub next_cursor: Option<String>,
}

/// Result of `resources/list`. The array key is `resources`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListResourcesResult {
    #[serde(rename = "resultType")]
    pub result_type: String,
    pub resources: Vec<Resource>,
    #[serde(
        rename = "nextCursor",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub next_cursor: Option<String>,
}

/// Result of `resources/read`. The array key is `contents`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadResourceResult {
    #[serde(rename = "resultType")]
    pub result_type: String,
    pub contents: Vec<ResourceContents>,
}

/// Result of `prompts/list`. The array key is `prompts`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListPromptsResult {
    #[serde(rename = "resultType")]
    pub result_type: String,
    pub prompts: Vec<Prompt>,
    #[serde(
        rename = "nextCursor",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub next_cursor: Option<String>,
}

/// Result of `prompts/get`. The array key is `messages`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetPromptResult {
    #[serde(rename = "resultType")]
    pub result_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub messages: Vec<PromptMessage>,
}

/// Result of the legacy `initialize` handshake, served only in dual-era mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InitializeResult {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    #[serde(rename = "serverInfo")]
    pub server_info: ServerInfo,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

/// `"complete"`, the only `resultType` this crate emits. Multi-round-trip
/// results (`"input_required"`) are not implemented.
pub(crate) const RESULT_TYPE_COMPLETE: &str = "complete";
