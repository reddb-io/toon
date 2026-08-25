//! JSON-RPC 2.0 wire types for MCP.
//!
//! MCP is plain JSON-RPC 2.0. Per Spec #389 §9, TOON-RPC extensions must never
//! be presented as standard MCP wire, so this module deliberately depends on
//! `serde_json` alone and shares no codec with the TOON-RPC crates.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Parse error: invalid JSON was received.
pub const PARSE_ERROR: i32 = -32700;
/// The JSON sent is not a valid Request object.
pub const INVALID_REQUEST: i32 = -32600;
/// The method does not exist or is not available.
pub const METHOD_NOT_FOUND: i32 = -32601;
/// Invalid method parameters. MCP also uses this for "resource not found".
pub const INVALID_PARAMS: i32 = -32602;
/// Internal JSON-RPC error.
pub const INTERNAL_ERROR: i32 = -32603;
/// `UnsupportedProtocolVersionError`, defined by MCP revision 2026-07-28.
pub const UNSUPPORTED_PROTOCOL_VERSION: i32 = -32022;

/// A JSON-RPC 2.0 request or notification.
///
/// A message with no `id` is a notification and MUST NOT be answered. An
/// explicit `id: null` is still a request and receives a response.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
}

impl JsonRpcRequest {
    /// True when the message carries no `id` key at all.
    pub fn is_notification(&self) -> bool {
        self.id.is_none()
    }

    /// `params` as an object map, or `None` when absent or not an object.
    pub fn params_object(&self) -> Option<&serde_json::Map<String, Value>> {
        self.params.as_ref().and_then(Value::as_object)
    }
}

/// A JSON-RPC 2.0 response. Carries exactly one of `result` or `error`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    pub id: Value,
}

impl JsonRpcResponse {
    pub fn success(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            result: Some(result),
            error: None,
            id,
        }
    }

    pub fn failure(id: Value, error: JsonRpcError) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            result: None,
            error: Some(error),
            id,
        }
    }
}

/// A JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(code: i32, message: impl Into<String>, data: Value) -> Self {
        Self {
            code,
            message: message.into(),
            data: Some(data),
        }
    }
}

/// Serialize a response as a single line with no embedded newlines, as the
/// MCP stdio transport requires.
pub fn to_line(response: &JsonRpcResponse) -> String {
    // `serde_json::to_string` never emits raw newlines; control characters in
    // strings are escaped. The invariant is asserted in tests.
    serde_json::to_string(response).unwrap_or_else(|e| {
        let fallback = JsonRpcResponse::failure(
            Value::Null,
            JsonRpcError::new(
                INTERNAL_ERROR,
                format!("response serialization failed: {e}"),
            ),
        );
        serde_json::to_string(&fallback).unwrap_or_else(|_| {
            r#"{"jsonrpc":"2.0","error":{"code":-32603,"message":"Internal error"},"id":null}"#
                .to_string()
        })
    })
}
