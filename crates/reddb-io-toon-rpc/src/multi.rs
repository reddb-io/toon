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
use crate::types::Id;
use crate::Dispatcher;
use serde_json::{json, Value as JsonValue};

const JSONRPC_VERSION: &str = "2.0";

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

/// Detect the protocol from a content-type hint and/or raw bytes, exactly as
/// `detectProtocol` in `@reddb-io/multi-rpc` does.
///
/// A `Content-Type` of `application/json` or `application/toon` (parameters
/// ignored) wins. Otherwise a body that parses as JSON and carries a
/// `jsonrpc` member (on the object, or on any entry of a batch) is JSON-RPC,
/// and so is a body opening with `{` that fails to parse: TOON never starts
/// with `{`, so its JSON client gets a JSON-RPC Parse error. Everything else
/// is TOON-RPC.
pub fn detect_protocol(raw: &[u8], content_type: Option<&str>) -> Protocol {
    if let Some(content_type) = content_type {
        let media_type = content_type
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        match media_type.as_str() {
            "application/json" => return Protocol::JsonRpc,
            "application/toon" => return Protocol::ToonRpc,
            _ => {}
        }
    }

    let Ok(text) = std::str::from_utf8(raw) else {
        return Protocol::ToonRpc;
    };
    let trimmed = text.trim_start();
    if !trimmed.starts_with('{') && !trimmed.starts_with('[') {
        return Protocol::ToonRpc;
    }
    match serde_json::from_str::<JsonValue>(trimmed) {
        Ok(value) if has_jsonrpc_member(&value) => Protocol::JsonRpc,
        Ok(_) => Protocol::ToonRpc,
        Err(_) if trimmed.starts_with('{') => Protocol::JsonRpc,
        Err(_) => Protocol::ToonRpc,
    }
}

fn has_jsonrpc_member(value: &JsonValue) -> bool {
    let member = |value: &JsonValue| value.as_object().is_some_and(|o| o.contains_key("jsonrpc"));
    match value {
        JsonValue::Array(entries) => entries.iter().any(member),
        value => member(value),
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
        let value: JsonValue = match serde_json::from_slice(raw) {
            Ok(value) => value,
            Err(error) => {
                return json_bytes(json_error_response(
                    Id::Null,
                    ErrorCode::ParseError.code(),
                    &format!("Parse error: {error}"),
                ));
            }
        };

        // Batch or single?
        let (entries, is_batch) = if value.is_array() {
            (value.as_array().cloned().unwrap_or_default(), true)
        } else {
            (vec![value.clone()], false)
        };

        if entries.is_empty() {
            return json_bytes(json_error_response(
                Id::Null,
                ErrorCode::InvalidRequest.code(),
                "Invalid Request: empty batch",
            ));
        }
        if entries.len() > self.dispatcher.max_batch_length() {
            return json_bytes(json_error_response(
                Id::Null,
                ErrorCode::InvalidRequest.code(),
                "Invalid Request: batch too large",
            ));
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
            json_bytes(JsonValue::Array(responses))
        } else {
            let obj = responses.into_iter().next().unwrap();
            json_bytes(obj)
        }
    }

    /// Dispatch a single JSON-RPC entry through the TOON-RPC core, so both
    /// dialects validate envelopes, IDs and params identically. Returns `None`
    /// for a notification.
    fn dispatch_jsonrpc_entry(&self, entry: JsonValue) -> Result<Option<JsonValue>, RpcError> {
        let call = match crate::serialization::validate_core_value(&entry) {
            Ok(()) => crate::serialization::call_from_value(to_toon_entry(entry)),
            Err(reason) => Call::Invalid(reason),
        };
        let responses = self.dispatcher.dispatch_message(Message::Single(call))?;
        Ok(responses.into_iter().next().map(json_response_from))
    }

    // ── TOON-RPC path ──────────────────────────────────────────────────────

    fn handle_toonrpc(&self, raw: &[u8]) -> Result<Vec<u8>, RpcError> {
        self.dispatcher.dispatch(raw)
    }
}

impl Default for MultiRpc {
    fn default() -> Self {
        Self::new(Dispatcher::new())
    }
}

// ── helpers ────────────────────────────────────────────────────────────────

/// Convert our typed `Id` back into a JSON value for the response.
pub(crate) fn id_to_json(id: &Id) -> JsonValue {
    match id {
        Id::Null => JsonValue::Null,
        Id::String(s) => JsonValue::String(s.clone()),
        Id::Number(n) => JsonValue::Number(serde_json::Number::from(*n)),
    }
}

/// Rename a JSON-RPC 2.0 envelope to TOON-RPC 1.0. Any other `jsonrpc`
/// value is dropped, so the core refuses the entry as an Invalid Request.
fn to_toon_entry(entry: JsonValue) -> JsonValue {
    let JsonValue::Object(object) = entry else {
        return entry;
    };
    let mut members = serde_json::Map::new();
    if object.get("jsonrpc").and_then(JsonValue::as_str) == Some(JSONRPC_VERSION) {
        members.insert("toonrpc".into(), JsonValue::from(crate::TOONRPC_VERSION));
    }
    for (key, value) in object {
        if key != "jsonrpc" && key != "toonrpc" {
            members.insert(key, value);
        }
    }
    JsonValue::Object(members)
}

/// Build a JSON-RPC 2.0 error response object.
pub(crate) fn json_error_response(id: Id, code: i32, message: &str) -> JsonValue {
    json!({
        "jsonrpc": JSONRPC_VERSION,
        "error": { "code": code, "message": message },
        "id": id_to_json(&id),
    })
}

fn json_bytes(value: JsonValue) -> Result<Vec<u8>, RpcError> {
    serde_json::to_vec(&value).map_err(|error| RpcError::SerializationError(error.to_string()))
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
            err.insert("message".into(), JsonValue::String(error.message.clone()));
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
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    fn build_dispatcher() -> Dispatcher {
        let mut d = Dispatcher::new();
        d.register("add", |params, _id| {
            let arr = match params {
                Params::ByPosition(arr) => arr,
                _ => return Err(RpcError::InvalidParams("expected array".into())),
            };
            let a = arr[0]
                .as_i64()
                .ok_or_else(|| RpcError::InvalidParams("a".into()))?;
            let b = arr[1]
                .as_i64()
                .ok_or_else(|| RpcError::InvalidParams("b".into()))?;
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

        assert!(
            text.contains("toonrpc"),
            "expected TOON marker, got: {}",
            text
        );
        assert!(
            text.contains("result"),
            "expected result field, got: {}",
            text
        );

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
    fn unstructured_or_null_params_are_invalid_requests() {
        let multi = MultiRpc::new(build_dispatcher());
        for raw in [
            &br#"{"jsonrpc":"2.0","method":"add","params":"not-an-array","id":1}"#[..],
            br#"{"jsonrpc":"2.0","method":"add","params":null,"id":1}"#,
        ] {
            let out = multi.handle(raw, None).unwrap();
            let parsed: JsonValue = serde_json::from_slice(&out).unwrap();
            assert_eq!(parsed["error"]["code"], -32600);
            assert!(parsed["id"].is_null());
        }
    }

    #[test]
    fn malformed_json_and_fractional_ids_stay_in_the_json_dialect() {
        let multi = MultiRpc::new(build_dispatcher());
        let (protocol, out) = multi
            .handle_with_protocol(br#"{"jsonrpc":"2.0","#, None)
            .unwrap();
        assert_eq!(protocol, Protocol::JsonRpc);
        let parsed: JsonValue = serde_json::from_slice(&out).unwrap();
        assert_eq!(parsed["error"]["code"], -32700);

        let fractional = br#"{"jsonrpc":"2.0","method":"add","params":[1,2],"id":1.5}"#;
        let parsed: JsonValue =
            serde_json::from_slice(&multi.handle(fractional, None).unwrap()).unwrap();
        assert_eq!(parsed["error"]["code"], -32600);
        assert!(parsed["id"].is_null());
    }

    #[test]
    fn notifications_execute_handlers_in_both_dialects() {
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&calls);
        let mut dispatcher = Dispatcher::new();
        dispatcher.register("observe", move |_params, _id| {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(JsonValue::Null)
        });
        let multi = MultiRpc::new(dispatcher);

        let json = br#"{"jsonrpc":"2.0","method":"observe","params":[]}"#;
        assert!(multi.handle(json, None).unwrap().is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        let toon = b"toonrpc: \"1.0\"\nmethod: observe\nparams[0]:\n";
        assert!(multi.handle(toon, None).unwrap().is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn jsonrpc_null_id_is_answered() {
        let multi = MultiRpc::new(build_dispatcher());
        let raw = br#"{"jsonrpc":"2.0","method":"add","params":[2,3],"id":null}"#;
        let out = multi.handle(raw, None).unwrap();
        let parsed: JsonValue = serde_json::from_slice(&out).unwrap();

        assert_eq!(parsed["result"], 5);
        assert!(parsed["id"].is_null());
    }

    #[test]
    fn jsonrpc_parse_error_is_encoded_as_a_response() {
        let multi = MultiRpc::new(build_dispatcher());
        let raw = br#"{"jsonrpc":"2.0","method":"add"#;
        let out = multi.handle(raw, Some("application/json")).unwrap();
        let parsed: JsonValue = serde_json::from_slice(&out).unwrap();

        assert_eq!(parsed["error"]["code"], -32700);
        assert!(parsed["id"].is_null());
    }

    #[test]
    fn jsonrpc_invalid_requests_are_encoded_as_responses() {
        let multi = MultiRpc::new(build_dispatcher());

        let wrong_version = br#"{"jsonrpc":"1.0","method":"add","params":[2,3],"id":7}"#;
        let out = multi.handle(wrong_version, None).unwrap();
        let parsed: JsonValue = serde_json::from_slice(&out).unwrap();
        assert_eq!(parsed["error"]["code"], -32600);
        assert!(parsed["id"].is_null());

        let empty_batch = multi.handle(b"[]", Some("application/json")).unwrap();
        let parsed: JsonValue = serde_json::from_slice(&empty_batch).unwrap();
        assert_eq!(parsed["error"]["code"], -32600);
        assert!(parsed["id"].is_null());

        let scalar = multi.handle(b"42", Some("application/json")).unwrap();
        let parsed: JsonValue = serde_json::from_slice(&scalar).unwrap();
        assert_eq!(parsed["error"]["code"], -32600);
        assert!(parsed["id"].is_null());
    }

    #[test]
    fn jsonrpc_batch_validates_each_entry_version() {
        let multi = MultiRpc::new(build_dispatcher());
        let raw = br#"[{"jsonrpc":"1.0","method":"add","params":[2,3],"id":1},{"jsonrpc":"2.0","method":"add","params":[3,4],"id":2}]"#;
        let out = multi.handle(raw, None).unwrap();
        let parsed: JsonValue = serde_json::from_slice(&out).unwrap();
        let responses = parsed.as_array().unwrap();

        assert_eq!(responses[0]["error"]["code"], -32600);
        assert!(responses[0]["id"].is_null());
        assert_eq!(responses[1]["result"], 7);
        assert_eq!(responses[1]["id"], 2);
    }

    #[test]
    fn toonrpc_protocol_errors_are_encoded_as_responses() {
        let multi = MultiRpc::new(build_dispatcher());

        let malformed = multi
            .handle(b"toonrpc: \"unterminated", Some("application/toon"))
            .unwrap();
        let Message::SingleResponse(response) = crate::from_wire(&malformed).unwrap() else {
            panic!("expected a TOON-RPC parse error response");
        };
        assert_eq!(response.error.unwrap().code, ErrorCode::ParseError);
        assert_eq!(response.id, Id::Null);

        let wrong_version = b"toonrpc: \"0.9\"\nmethod: add\nparams[2]: 2,3\nid: 4\n";
        let invalid = multi.handle(wrong_version, None).unwrap();
        let Message::SingleResponse(response) = crate::from_wire(&invalid).unwrap() else {
            panic!("expected a TOON-RPC invalid request response");
        };
        assert_eq!(response.error.unwrap().code, ErrorCode::InvalidRequest);
        assert_eq!(response.id, Id::Null);
    }

    #[test]
    fn toonrpc_batch_is_validated_and_preserves_batch_shape() {
        let multi = MultiRpc::new(build_dispatcher());
        let batch = Message::Batch(vec![
            Call::Request(crate::protocol::Request::new(
                "add".into(),
                Params::ByPosition(vec![json!(2), json!(3)]),
                Id::Number(1),
            )),
            Call::Notification(crate::protocol::Notification::new(
                "add".into(),
                Params::ByPosition(vec![json!(4), json!(5)]),
            )),
        ]);
        let raw = crate::to_wire(&batch).unwrap();
        let out = multi.handle(&raw, Some("application/toon")).unwrap();

        let Message::BatchResponse(responses) = crate::from_wire(&out).unwrap() else {
            panic!("expected a TOON-RPC batch response");
        };
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].result, Some(json!(5)));
        assert_eq!(responses[0].id, Id::Number(1));
    }
}
