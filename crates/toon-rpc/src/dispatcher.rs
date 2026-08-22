use std::collections::HashMap;

use crate::error::{Error, ErrorCode, RpcError};
use crate::protocol::{Call, Message, Response};
use crate::types::{Id, Params, Value};

pub struct Dispatcher {
    methods: HashMap<String, Box<dyn Fn(Params, Id) -> Result<Value, RpcError> + Send + Sync>>,
}

impl Dispatcher {
    pub fn new() -> Self {
        Self {
            methods: HashMap::new(),
        }
    }

    pub fn register<F>(&mut self, method: impl Into<String>, handler: F)
    where
        F: Fn(Params, Id) -> Result<Value, RpcError> + Send + Sync + 'static,
    {
        self.methods.insert(method.into(), Box::new(handler));
    }

    pub fn dispatch(&self, raw: &[u8]) -> Result<Vec<u8>, RpcError> {
        let msg: Message = serde_json::from_slice(raw)
            .map_err(|e| RpcError::ParseError(e.to_string()))?;

        let responses = self.handle_message(msg)?;

        if responses.is_empty() {
            return Ok(vec![]);
        }

        if responses.len() == 1 {
            serde_json::to_vec(&responses[0])
                .map_err(|e| RpcError::SerializationError(e.to_string()))
        } else {
            serde_json::to_vec(&responses)
                .map_err(|e| RpcError::SerializationError(e.to_string()))
        }
    }

    fn handle_message(&self, msg: Message) -> Result<Vec<Response>, RpcError> {
        match msg {
            Message::Single(Call::Request(req)) => {
                Ok(vec![self.handle_request(req)])
            }
            Message::Single(Call::Notification(notif)) => {
                drop(notif);
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
                            drop(notif);
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
            Err(e) => Response::error(
                Error::with_message(ErrorCode::InternalError, e.to_string()),
                req.id,
            ),
        }
    }
}

impl Default for Dispatcher {
    fn default() -> Self {
        Self::new()
    }
}
