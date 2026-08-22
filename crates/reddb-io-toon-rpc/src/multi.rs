//! Multi-protocol RPC — auto-detects JSON-RPC 2.0 vs TOON-RPC 1.0 on the wire
//! and answers in the same format the client used.
//!
//! ## Wire format detection
//!
//! The first non-whitespace character plus a peek at the first ~64 bytes decide:
//!
//! - Starts with `toonrpc` → TOON-RPC
//! - Starts with `{` and contains `"jsonrpc"` → JSON-RPC 2.0
//! - Anything else → TOON-RPC (our preferred format)
//!
//! An explicit `Content-Type: application/json` or `application/toon` HTTP
//! header always wins over sniffing.
//!
//! ## Usage
//!
//! ```no_run
//! use reddb_io_toon_rpc::{Dispatcher, Params};
//! use reddb_io_toon_rpc::multi::MultiRpc;
//!
//! let mut dispatcher = Dispatcher::new();
//! dispatcher.register("add", |params, _id| {
//!     let nums = match params {
//!         Params::ByPosition(arr) => arr,
//!         _ => return Err(reddb_io_toon_rpc::RpcError::InvalidParams("expected array".into())),
//!     };
//!     let a = nums[0].as_i64().unwrap();
//!     let b = nums[1].as_i64().unwrap();
//!     Ok(serde_json::json!(a + b))
//! });
//!
//! let multi = MultiRpc::new(dispatcher);
//!
//! // JSON-RPC request → JSON-RPC response
//! let json_req = br#"{"jsonrpc":"2.0","method":"add","params":[2,3],"id":1}"#;
//! let json_resp = multi.handle(json_req, None).unwrap();
//! assert!(std::str::from_utf8(&json_resp).unwrap().starts_with('{'));
//!
//! // TOON-RPC request → TOON-RPC response
//! let toon_req = b"toonrpc: \"1.0\"\nmethod: add\nparams[2]: 2,3\nid: 1\n";
//! let toon_resp = multi.handle(toon_req, None).unwrap();
//! assert!(std::str::from_utf8(&toon_resp).unwrap().contains("toonrpc"));
//! ```

use crate::error::{ErrorCode, RpcError};
use crate::protocol::{Call, Message, Response};
use crate::types::{Id, Params};
use crate::Dispatcher;
use serde_json::{json, Value as JsonValue};

const JSONRPC_VERSION: &str = "2.0";
const TOONRPC_VERSION: &str = "1.0";

/// Wire protocol variants the dispatcher can negotiate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    /// JSON-RPC 2.0 — standard JSON with `"jsonrpc":"2.0"` field
    JsonRpc,
    /// TOON-RPC 1.0 — TOON with `"toonrpc":"1.0"` field
    ToonRpc,
}

impl Protocol {
    /// MIME type for HTTP `Content-Type` / `Accept` negotiation.
    pub fn content_type(self) -> &'static str {
        match self {
            Protocol::JsonRpc => "application/json",
            Protocol::ToonRpc => "application/toon",
        }
    }
}

/// Detect the protocol from a content-type hint and/or raw bytes.
///
/// An explicit content-type hint (when provided) wins over byte sniffing.
pub fn detect_protocol(raw: &[u8], content_type: Option<&str>) -> Protocol {
    if let Some(ct) = content_type {
        let lower = ct.to_ascii_lowercase();
        if lower.contains("application/json") {
            return Protocol::JsonRpc;
        }
        if lower.contains("application/toon") {
            return Protocol::ToonRpc;
        }
    }

    // Body sniffing: skip leading whitespace, then peek at the first few bytes.
    let s = match std::str::from_utf8(raw) {
        Ok(s) => s,
        Err(_) => return Protocol::ToonRpc,
    };
    let trimmed = s.trim_start();

    // JSON-RPC bodies (single or batch) — they contain `"jsonrpc"` within
    // the first ~80 bytes. Batch requests start with `[` but the first
    // object inside carries the discriminator.
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        let head: String = trimmed.chars().take(80).collect();
        if head.contains("\"jsonrpc\"") {
            return Protocol::JsonRpc;
        }
        // Could still be JSON but not JSON-RPC; fall through and let the parser
        // decide (it'll surface a "missing jsonrpc" error if it really was meant
        // to be JSON-RPC). Default to TOON-RPC since that's our canonical format.
        return Protocol::ToonRpc;
    }

    // TOON-RPC bodies start with `toonrpc:` or `{ toonrpc`.
    if trimmed.starts_with("toonrpc:") || trimmed.starts_with("{toonrpc") {
        return Protocol::ToonRpc;
    }

    // Last resort: if the bytes are pure JSON (start with `{` or `[`), prefer JSON-RPC.
    // Otherwise assume TOON-RPC.
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        Protocol::JsonRpc
    } else {
        Protocol::ToonRpc
    }
}

/// Multi-protocol dispatcher — single handler, two wire formats.
#[derive(Clone)]
pub struct MultiRpc {
    dispatcher: Dispatcher,
}

impl MultiRpc {
    pub fn new(dispatcher: Dispatcher) -> Self {
        Self { dispatcher }
    }

    /// Detect the protocol of `raw` and dispatch, returning the wire-encoded
    /// response in the same format as the request.
    pub fn handle(&self, raw: &[u8], content_type: Option<&str>) -> Result<Vec<u8>, RpcError> {
        let protocol = detect_protocol(raw, content_type);
        match protocol {
            Protocol::JsonRpc => self.handle_jsonrpc(raw),
            Protocol::ToonRpc => self.handle_toonrpc(raw),
        }
    }

    /// Handle a request, returning the detected protocol alongside the response
    /// bytes — useful for transports that need to set the right `Content-Type`.
    pub fn handle_with_protocol(
        &self,
        raw: &[u8],
        content_type: Option<&str>,
    ) -> Result<(Protocol, Vec<u8>), RpcError> {
        let protocol = detect_protocol(raw, content_type);
        let bytes = match protocol {
            Protocol::JsonRpc => self.handle_jsonrpc(raw)?,
            Protocol::ToonRpc => self.handle_toonrpc(raw)?,
        };
        Ok((protocol, bytes))
    }

    /// Expose the underlying `Dispatcher` so callers can `register` methods.
    pub fn dispatcher_mut(&mut self) -> &mut Dispatcher {
        &mut self.dispatcher
    }

    pub fn dispatcher(&self) -> &Dispatcher {
        &self.dispatcher
    }

    // ── JSON-RPC path ─────────────────────────────────────────────────────

    fn handle_jsonrpc(&self, raw: &[u8]) -> Result<Vec<u8>, RpcError> {
        let value: JsonValue = serde_json::from_slice(raw).map_err(|e| {
            RpcError::ParseError(format!("JSON parse error: {}", e))
        })?;

        // Batch or single?
        let (entries, is_batch) = if value.is_array() {
            (value.as_array().cloned().unwrap_or_default(), true)
        } else {
            (vec![value.clone()], false)
        };

        if !is_batch {
            // Validate protocol version on single requests only — for batches,
            // each entry carries its own version.
            let version = entries[0]["jsonrpc"].as_str();
            if version != Some(JSONRPC_VERSION) {
                return Err(RpcError::InvalidRequest(format!(
                    "expected jsonrpc {}, got {:?}",
                    JSONRPC_VERSION, version
                )));
            }
        }

        if entries.is_empty() {
            return Err(RpcError::InvalidRequest("empty batch".into()));
        }

        let mut responses = Vec::with_capacity(entries.len());
        for entry in entries {
            if let Some(r) = self.dispatch_jsonrpc_entry(entry)? {
                responses.push(r);
            }
        }

        if responses.is_empty() {
            // All entries were notifications — JSON-RPC says nothing to return.
            return Ok(vec![]);
        }

        // Echo the batch shape: array if the request was an array, otherwise a
        // single object.
        if is_batch {
            let arr: Vec<JsonValue> = responses.into_iter().collect();
            serde_json::to_vec(&arr).map_err(|e| RpcError::SerializationError(e.to_string()))
        } else {
            let obj = responses.into_iter().next().unwrap();
            serde_json::to_vec(&obj).map_err(|e| RpcError::SerializationError(e.to_string()))
        }
    }

    /// Dispatch a single JSON-RPC entry. Returns `None` for notifications
    /// (no response should be sent).
    fn dispatch_jsonrpc_entry(&self, entry: JsonValue) -> Result<Option<JsonValue>, RpcError> {
        // Sanity-check that this looks like a request, not a stray response.
        let obj = match entry.as_object() {
            Some(o) => o,
            None => return Ok(None),
        };

        let method = match obj.get("method").and_then(JsonValue::as_str) {
            Some(m) => m.to_string(),
            None => {
                let id = id_from_json(obj.get("id"));
                return Ok(Some(json_error_response(
                    id,
                    -32600,
                    "missing method field",
                )));
            }
        };

        let id = id_from_json(obj.get("id"));
        let is_notification = matches!(id, Id::Null);

        // Build a TOON-RPC Request so we can reuse the dispatcher.
        let params_value = obj.get("params").cloned().unwrap_or(JsonValue::Null);
        let params = params_from_json(params_value);

        let request = crate::protocol::Request {
            toonrpc: crate::TOONRPC_VERSION.to_string(),
            method,
            params,
            id: id.clone(),
        };

        let responses = self.dispatcher.dispatch_message(Message::Single(Call::Request(request)))?;

        if is_notification || responses.is_empty() {
            return Ok(None);
        }

        let resp = responses.into_iter().next().unwrap();
        Ok(Some(json_response_from(resp)))
    }

    // ── TOON-RPC path ──────────────────────────────────────────────────────

    fn handle_toonrpc(&self, raw: &[u8]) -> Result<Vec<u8>, RpcError> {
        let msg = crate::from_wire(raw)?;

        // Sanity check: TOON-RPC body must carry `toonrpc: "1.0"`.
        if !has_toonrpc_marker(&msg) {
            return Err(RpcError::InvalidRequest(
                "missing toonrpc field on request".into(),
            ));
        }

        let responses = self.dispatcher.dispatch_message(msg)?;

        if responses.is_empty() {
            return Ok(vec![]);
        }

        if responses.len() == 1 {
            crate::to_wire(&Message::SingleResponse(responses[0].clone()))
        } else {
            crate::to_wire(&Message::BatchResponse(responses))
        }
    }
}

impl Default for MultiRpc {
    fn default() -> Self {
        Self::new(Dispatcher::new())
    }
}

// ── helpers ────────────────────────────────────────────────────────────────

fn has_toonrpc_marker(msg: &Message) -> bool {
    match msg {
        Message::Single(Call::Request(r)) => r.toonrpc == TOONRPC_VERSION,
        Message::Single(Call::Notification(n)) => n.toonrpc == TOONRPC_VERSION,
        _ => false,
    }
}

/// Convert a JSON-RPC `id` field (string | number | null) into our typed `Id`.
pub(crate) fn id_from_json(v: Option<&JsonValue>) -> Id {
    match v {
        None | Some(JsonValue::Null) => Id::Null,
        Some(JsonValue::String(s)) => Id::String(s.clone()),
        Some(JsonValue::Number(n)) => Id::Number(n.as_i64().unwrap_or(0)),
        _ => Id::Null,
    }
}

/// Convert our typed `Id` back into a JSON value for the response.
pub(crate) fn id_to_json(id: &Id) -> JsonValue {
    match id {
        Id::Null => JsonValue::Null,
        Id::String(s) => JsonValue::String(s.clone()),
        Id::Number(n) => JsonValue::Number(serde_json::Number::from(*n)),
    }
}

/// Parse a JSON-RPC `params` field into our `Params` enum.
pub(crate) fn params_from_json(v: JsonValue) -> Params {
    match v {
        JsonValue::Null => Params::ByPosition(vec![]),
        JsonValue::Array(arr) => Params::ByPosition(arr),
        JsonValue::Object(map) => Params::ByName(map),
        _ => Params::ByPosition(vec![v]),
    }
}

/// Build a JSON-RPC 2.0 error response object.
pub(crate) fn json_error_response(id: Id, code: i32, message: &str) -> JsonValue {
    json!({
        "jsonrpc": JSONRPC_VERSION,
        "error": { "code": code, "message": message },
        "id": id_to_json(&id),
    })
}

/// Convert a typed `Response` (TOON-RPC) into a JSON-RPC 2.0 response object.
pub(crate) fn json_response_from(resp: Response) -> JsonValue {
    let mut obj = serde_json::Map::new();
    obj.insert("jsonrpc".into(), JsonValue::String(JSONRPC_VERSION.into()));
    obj.insert("id".into(), id_to_json(&resp.id));

    match (resp.result, resp.error) {
        (Some(value), None) => {
            obj.insert("result".into(), value);
        }
        (None, Some(error)) => {
            let mut err = serde_json::Map::new();
            err.insert("code".into(), JsonValue::Number(error.code.code().into()));
            err.insert(
                "message".into(),
                JsonValue::String(error.message.clone()),
            );
            if let Some(data) = error.data {
                err.insert("data".into(), data);
            }
            obj.insert("error".into(), JsonValue::Object(err));
        }
        _ => {
            // Malformed response — emit a synthetic internal error
            obj.insert(
                "error".into(),
                json!({"code": ErrorCode::InternalError.code(), "message": "invalid response"}),
            );
        }
    }

    JsonValue::Object(obj)
}

// Lightweight re-export so transports only need one `use` statement.
pub use Protocol as DetectedProtocol;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Id, Params};

    fn build_dispatcher() -> Dispatcher {
        let mut d = Dispatcher::new();
        d.register("add", |params, _id| {
            let arr = match params {
                Params::ByPosition(arr) => arr,
                _ => return Err(RpcError::InvalidParams("expected array".into())),
            };
            let a = arr[0].as_i64().ok_or_else(|| RpcError::InvalidParams("a".into()))?;
            let b = arr[1].as_i64().ok_or_else(|| RpcError::InvalidParams("b".into()))?;
            Ok(serde_json::json!(a + b))
        });
        d.register("echo", |params, _id| {
            Ok(serde_json::Value::String(format!("echo: {:?}", params)))
        });
        d
    }

    #[test]
    fn detects_jsonrpc_object() {
        let raw = br#"{"jsonrpc":"2.0","method":"add","params":[1,2],"id":1}"#;
        assert_eq!(detect_protocol(raw, None), Protocol::JsonRpc);
    }

    #[test]
    fn detects_toonrpc_object() {
        let raw = b"toonrpc: \"1.0\"\nmethod: add\nparams[2]: 1,2\nid: 1\n";
        assert_eq!(detect_protocol(raw, None), Protocol::ToonRpc);
    }

    #[test]
    fn content_type_wins_over_sniffing() {
        let raw = br#"{"jsonrpc":"2.0","method":"add","params":[1,2],"id":1}"#;
        assert_eq!(
            detect_protocol(raw, Some("application/toon")),
            Protocol::ToonRpc
        );

        let raw = b"toonrpc: \"1.0\"\nmethod: add\nparams[2]: 1,2\nid: 1\n";
        assert_eq!(
            detect_protocol(raw, Some("application/json")),
            Protocol::JsonRpc
        );
    }

    #[test]
    fn jsonrpc_request_yields_jsonrpc_response() {
        let multi = MultiRpc::new(build_dispatcher());
        let raw = br#"{"jsonrpc":"2.0","method":"add","params":[2,3],"id":1}"#;
        let out = multi.handle(raw, None).unwrap();
        let text = std::str::from_utf8(&out).unwrap();

        assert!(text.starts_with('{'), "expected JSON object, got: {}", text);
        let parsed: JsonValue = serde_json::from_slice(&out).unwrap();
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert_eq!(parsed["result"], 5);
        assert_eq!(parsed["id"], 1);
    }

    #[test]
    fn toonrpc_request_yields_toonrpc_response() {
        let multi = MultiRpc::new(build_dispatcher());
        let raw = b"toonrpc: \"1.0\"\nmethod: add\nparams[2]: 2,3\nid: 1\n";
        let out = multi.handle(raw, None).unwrap();
        let text = std::str::from_utf8(&out).unwrap();

        assert!(text.contains("toonrpc"), "expected TOON marker, got: {}", text);
        assert!(text.contains("result"), "expected result field, got: {}", text);

        // The TOON value should decode back to a Response with result=5
        let parsed = crate::from_wire(&out).unwrap();
        match parsed {
            Message::SingleResponse(resp) => {
                assert_eq!(resp.result, Some(serde_json::json!(5)));
                assert_eq!(resp.id, Id::Number(1));
            }
            other => panic!("expected SingleResponse, got {:?}", other),
        }
    }

    #[test]
    fn mixed_request_preserves_protocol() {
        let multi = MultiRpc::new(build_dispatcher());

        // JSON-RPC request for the same dispatcher
        let json_req = br#"{"jsonrpc":"2.0","method":"echo","params":["hi"],"id":"abc"}"#;
        let json_out = multi.handle(json_req, None).unwrap();
        let parsed: JsonValue = serde_json::from_slice(&json_out).unwrap();
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert_eq!(parsed["id"], "abc");
        assert!(parsed["result"].as_str().unwrap().contains("hi"));

        // TOON-RPC request for the same dispatcher
        let toon_req = b"toonrpc: \"1.0\"\nmethod: echo\nparams[1]: hi\nid: abc\n";
        let toon_out = multi.handle(toon_req, None).unwrap();
        let parsed = crate::from_wire(&toon_out).unwrap();
        match parsed {
            Message::SingleResponse(resp) => {
                assert_eq!(resp.id, Id::String("abc".into()));
                let s = resp.result.unwrap();
                assert!(s.as_str().unwrap().contains("hi"));
            }
            other => panic!("expected SingleResponse, got {:?}", other),
        }
    }

    #[test]
    fn unknown_method_returns_method_not_found() {
        let multi = MultiRpc::new(build_dispatcher());

        let json_req = br#"{"jsonrpc":"2.0","method":"nope","params":[],"id":7}"#;
        let out = multi.handle(json_req, None).unwrap();
        let parsed: JsonValue = serde_json::from_slice(&out).unwrap();
        assert_eq!(parsed["error"]["code"], -32601);
        assert_eq!(parsed["id"], 7);

        let toon_req = b"toonrpc: \"1.0\"\nmethod: nope\nparams[0]:\nid: 7\n";
        let out = multi.handle(toon_req, None).unwrap();
        let text = std::str::from_utf8(&out).unwrap();
        assert!(text.contains("-32601"), "got: {}", text);
    }

    #[test]
    fn notification_returns_no_response() {
        let multi = MultiRpc::new(build_dispatcher());

        // JSON-RPC notification (id field absent)
        let json_req = br#"{"jsonrpc":"2.0","method":"add","params":[1,2]}"#;
        let out = multi.handle(json_req, None).unwrap();
        assert!(out.is_empty(), "expected empty response, got: {:?}", out);

        // TOON-RPC notification — omit `id` entirely (matches JSON-RPC semantics
        // for notifications).
        let toon_req = b"toonrpc: \"1.0\"\nmethod: add\nparams[2]: 1,2\n";
        let out = multi.handle(toon_req, None).unwrap();
        assert!(out.is_empty(), "expected empty response, got: {:?}", out);
    }

    #[test]
    fn batch_jsonrpc_works() {
        let multi = MultiRpc::new(build_dispatcher());
        let json_req = br#"[{"jsonrpc":"2.0","method":"add","params":[1,2],"id":1},{"jsonrpc":"2.0","method":"add","params":[3,4],"id":2}]"#;
        let out = multi.handle(json_req, None).unwrap();
        let parsed: JsonValue = serde_json::from_slice(&out).unwrap();
        let arr = parsed.as_array().expect("batch must be array");
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["result"], 3);
        assert_eq!(arr[1]["result"], 7);
    }

    #[test]
    fn invalid_jsonrpc_returns_parse_error() {
        let multi = MultiRpc::new(build_dispatcher());
        let raw = br#"{"jsonrpc":"2.0","method":"add","params":"not-an-array","id":1}"#;
        let out = multi.handle(raw, None).unwrap();
        let parsed: JsonValue = serde_json::from_slice(&out).unwrap();
        assert!(parsed["error"].is_object());
        assert_eq!(parsed["error"]["code"], -32602);
    }
}
