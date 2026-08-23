use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{Error, ErrorCode, RpcError};
use crate::protocol::{Call, Message, Notification, Response};
use crate::types::{Id, Params, Value};

type Handler = dyn Fn(Params, Id) -> Result<Value, RpcError> + Send + Sync;

#[derive(Clone)]
pub struct Dispatcher {
    methods: Arc<HashMap<String, Arc<Handler>>>,
}

impl Dispatcher {
    pub fn new() -> Self {
        Self {
            methods: Arc::new(HashMap::new()),
        }
    }

    pub fn register<F>(&mut self, method: impl Into<String>, handler: F)
    where
        F: Fn(Params, Id) -> Result<Value, RpcError> + Send + Sync + 'static,
    {
        let mut methods = (*self.methods).clone();
        methods.insert(method.into(), Arc::new(handler));
        self.methods = Arc::new(methods);
    }

    pub fn dispatch(&self, raw: &[u8]) -> Result<Vec<u8>, RpcError> {
        let msg = crate::from_wire(raw)?;

        let responses = self.handle_message(msg)?;

        if responses.is_empty() {
            return Ok(vec![]);
        }

        if responses.len() == 1 {
            crate::to_wire(&Message::SingleResponse(responses[0].clone()))
        } else {
            crate::to_wire(&Message::BatchResponse(responses))
        }
    }

    /// Dispatch an already-parsed `Message`, returning the list of typed
    /// `Response` values (no wire serialization). Used by [`crate::multi::MultiRpc`]
    /// and any caller that wants to re-encode in a different wire format.
    pub fn dispatch_message(&self, msg: Message) -> Result<Vec<Response>, RpcError> {
        self.handle_message(msg)
    }

    fn handle_message(&self, msg: Message) -> Result<Vec<Response>, RpcError> {
        match msg {
            Message::Single(Call::Request(req)) => Ok(vec![self.handle_request(req)]),
            Message::Single(Call::Notification(notif)) => {
                self.handle_notification(notif);
                Ok(vec![])
            }
            Message::Batch(calls) => {
                let mut responses = vec![];
                for call in calls {
                    match call {
                        Call::Request(req) => {
                            responses.push(self.handle_request(req));
                        }
                        Call::Notification(notif) => {
                            self.handle_notification(notif);
                        }
                    }
                }
                Ok(responses)
            }
            Message::SingleResponse(_) | Message::BatchResponse(_) => {
                Err(RpcError::InvalidRequest("Unexpected response".into()))
            }
        }
    }

    fn handle_request(&self, req: crate::protocol::Request) -> Response {
        let handler = match self.methods.get(&req.method) {
            Some(h) => h,
            None => {
                return Response::error(
                    Error::with_message(ErrorCode::MethodNotFound, &req.method),
                    req.id,
                );
            }
        };

        match handler(req.params, req.id.clone()) {
            Ok(result) => Response::success(result, req.id),
            Err(e) => {
                // Map handler error variants to JSON-RPC / TOON-RPC error codes.
                // Anything we don't have a specific code for falls back to
                // InternalError so handlers can stay simple.
                let code = match &e {
                    crate::error::RpcError::ParseError(_) => ErrorCode::ParseError,
                    crate::error::RpcError::InvalidRequest(_) => ErrorCode::InvalidRequest,
                    crate::error::RpcError::MethodNotFound(_) => ErrorCode::MethodNotFound,
                    crate::error::RpcError::InvalidParams(_) => ErrorCode::InvalidParams,
                    crate::error::RpcError::InternalError(_) => ErrorCode::InternalError,
                    crate::error::RpcError::ServerError(_, _) => ErrorCode::InternalError,
                    crate::error::RpcError::TransportError(_) => ErrorCode::InternalError,
                    crate::error::RpcError::SerializationError(_) => ErrorCode::InternalError,
                };
                Response::error(Error::with_message(code, e.to_string()), req.id)
            }
        }
    }

    fn handle_notification(&self, notification: Notification) {
        let Some(handler) = self.methods.get(&notification.method) else {
            return;
        };

        // Notifications execute like requests, but neither success nor failure
        // produces a response. `Id::Null` is the handler-level placeholder for
        // the absent wire id; notification-ness itself lives in `Call`.
        let _ = handler(notification.params, Id::Null);
    }
}

impl Default for Dispatcher {
    fn default() -> Self {
        Self::new()
    }
}
