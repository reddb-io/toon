//! MCP method dispatcher — registers all MCP methods on a toon-rpc Dispatcher.

use crate::types::{DiscoverResponse, ListResponse, MCP_PROTOCOL_VERSION};
use crate::{McpError, McpResult, McpService};
use reddb_io_toon_rpc::{Dispatcher, Params, RpcError, Value};
use serde_json::json;

/// Adapter that turns an `McpService` into a set of registered methods on a
/// `Dispatcher`. Use this when you need fine-grained control (e.g. mixing MCP
/// with custom RPC methods).
pub struct McpDispatcher<S: McpService> {
    service: std::sync::Arc<S>,
}

impl<S: McpService> McpDispatcher<S> {
    pub fn new(service: std::sync::Arc<S>) -> Self {
        Self { service }
    }

    /// Build the discover response.
    pub fn discover(&self) -> DiscoverResponse {
        let info = self.service.server_info();
        let caps = self.service.capabilities();
        DiscoverResponse {
            result_type: "complete".into(),
            supported_versions: vec![MCP_PROTOCOL_VERSION.into()],
            capabilities: caps,
            meta: Some(crate::types::DiscoverMeta { server_info: info }),
            ttl_ms: Some(3600000),
            cache_scope: Some("public".into()),
        }
    }

    /// Handle any MCP method call. Returns a TOON-friendly JSON value.
    pub fn dispatch_method(&self, method: &str, params: Params) -> McpResult<Value> {
        match method {
            "server/discover" => {
                let response = self.discover();
                Ok(serde_json::to_value(response).unwrap())
            }
            "tools/list" => {
                let tools = self.service.list_tools();
                Ok(serde_json::to_value(ListResponse {
                    result_type: "complete".into(),
                    items: tools,
                    next_cursor: None,
                })
                .unwrap())
            }
            "resources/list" => {
                let resources = self.service.list_resources();
                Ok(serde_json::to_value(ListResponse {
                    result_type: "complete".into(),
                    items: resources,
                    next_cursor: None,
                })
                .unwrap())
            }
            "resources/read" => {
                let uri = extract_named_string(&params, "uri")?;
                self.service.read_resource(&uri)
            }
            "prompts/list" => {
                let prompts = self.service.list_prompts();
                Ok(serde_json::to_value(ListResponse {
                    result_type: "complete".into(),
                    items: prompts,
                    next_cursor: None,
                })
                .unwrap())
            }
            "prompts/get" => {
                let name = extract_named_string(&params, "name")?;
                let args = extract_named_value(&params, "arguments");
                self.service.get_prompt(&name, args)
            }
            "tools/call" => {
                let name = extract_named_string(&params, "name")?;
                let arguments = extract_named_value(&params, "arguments").unwrap_or(json!({}));
                let response = self.service.call_tool(&name, arguments);
                Ok(serde_json::to_value(response).unwrap())
            }
            _ => Err(McpError::MethodNotFound(method.to_string())),
        }
    }

    /// Register all MCP methods on a `Dispatcher`.
    pub fn register(&self, dispatcher: &mut Dispatcher) {
        let svc = self.service.clone();
        let disp = self.clone();
        let svc_for_register = svc.clone();
        let disp_for_register = disp.clone();

        dispatcher.register("server/discover", move |params, _id| {
            disp_for_register
                .dispatch_method("server/discover", params)
                .map_err(into_rpc_error)
        });

        dispatcher.register("tools/list", move |_params, _id| {
            svc_for_register
                .list_tools()
                .pipe_list()
                .map_err(into_rpc_error)
        });

        let svc_for_register2 = svc.clone();
        dispatcher.register("tools/call", move |params, _id| {
            let name = extract_named_string(&params, "name").map_err(into_rpc_error)?;
            let arguments = extract_named_value(&params, "arguments").unwrap_or(json!({}));
            Ok(serde_json::to_value(svc_for_register2.call_tool(&name, arguments)).unwrap())
        });

        let svc_for_register3 = svc.clone();
        dispatcher.register("resources/list", move |_params, _id| {
            svc_for_register3
                .list_resources()
                .pipe_list()
                .map_err(into_rpc_error)
        });

        let svc_for_register4 = svc.clone();
        dispatcher.register("resources/read", move |params, _id| {
            let uri = extract_named_string(&params, "uri").map_err(into_rpc_error)?;
            svc_for_register4
                .read_resource(&uri)
                .map_err(into_rpc_error)
        });

        let svc_for_register5 = svc.clone();
        dispatcher.register("prompts/list", move |_params, _id| {
            svc_for_register5
                .list_prompts()
                .pipe_list()
                .map_err(into_rpc_error)
        });

        let svc_for_register6 = svc.clone();
        dispatcher.register("prompts/get", move |params, _id| {
            let name = extract_named_string(&params, "name").map_err(into_rpc_error)?;
            let args = extract_named_value(&params, "arguments");
            svc_for_register6
                .get_prompt(&name, args)
                .map_err(into_rpc_error)
        });
    }
}

impl<S: McpService> Clone for McpDispatcher<S> {
    fn clone(&self) -> Self {
        Self {
            service: self.service.clone(),
        }
    }
}

/// Convenience: build a fully-loaded Dispatcher from an MCP service.
pub fn dispatch_mcp<S: McpService>(service: std::sync::Arc<S>) -> Dispatcher {
    let mut dispatcher = Dispatcher::new();
    McpDispatcher::new(service).register(&mut dispatcher);
    dispatcher
}

/// Extension trait to glue `Vec<T>` through the `to_value` serialization for
/// `Result<Value, RpcError>` returns.
trait PipeList: Sized {
    fn pipe_list(self) -> Result<Value, McpError>;
}

impl<T: serde::Serialize> PipeList for Vec<T> {
    fn pipe_list(self) -> Result<Value, McpError> {
        Ok(serde_json::to_value(ListResponse {
            result_type: "complete".into(),
            items: self,
            next_cursor: None,
        })
        .unwrap())
    }
}

/// Pull a named string field out of a `Params::ByName` (or fall back to
/// positional index 0 for by-position callers).
fn extract_named_string(params: &Params, key: &str) -> McpResult<String> {
    match params {
        Params::ByName(map) => match map.get(key) {
            Some(Value::String(s)) => Ok(s.clone()),
            Some(v) => Ok(v.to_string()),
            None => Err(McpError::InvalidParams(format!("missing field: {}", key))),
        },
        Params::ByPosition(arr) => {
            if let Some(Value::String(s)) = arr.first() {
                Ok(s.clone())
            } else {
                Err(McpError::InvalidParams(format!(
                    "expected by-name params with field: {}",
                    key
                )))
            }
        }
    }
}

fn extract_named_value(params: &Params, key: &str) -> Option<Value> {
    match params {
        Params::ByName(map) => map.get(key).cloned(),
        Params::ByPosition(arr) => arr.get(1).cloned(),
    }
}

fn into_rpc_error(e: McpError) -> RpcError {
    match e {
        McpError::MethodNotFound(m) => RpcError::InvalidParams(format!("method not found: {}", m)),
        McpError::ToolNotFound(t) => RpcError::InvalidParams(format!("tool not found: {}", t)),
        McpError::ResourceNotFound(r) => {
            RpcError::InvalidParams(format!("resource not found: {}", r))
        }
        McpError::PromptNotFound(p) => RpcError::InvalidParams(format!("prompt not found: {}", p)),
        McpError::InvalidParams(p) => RpcError::InvalidParams(p),
        McpError::Internal(p) => RpcError::InternalError(p),
    }
}
