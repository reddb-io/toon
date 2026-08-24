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
        let msg = match crate::from_wire(raw) {
            Ok(msg) => msg,
            Err(RpcError::ParseError(_)) => {
                return crate::to_wire(&Message::SingleResponse(protocol_error(
                    ErrorCode::ParseError,
                )))
            }
            Err(error) => return Err(error),
        };
        let (responses, is_batch) = self.handle_message(msg);

        if responses.is_empty() {
            return Ok(vec![]);
        }
        let responses = responses
            .into_iter()
            .map(|response| ensure_toon_response_encodable(response, is_batch))
            .collect();

        if is_batch {
            crate::to_wire(&Message::BatchResponse(responses))
        } else {
            crate::to_wire(&Message::SingleResponse(responses[0].clone()))
        }
    }

    /// Dispatch an already-parsed `Message`, returning the list of typed
    /// `Response` values (no wire serialization), for callers that want to
    /// re-encode in a different wire format.
    pub fn dispatch_message(&self, msg: Message) -> Result<Vec<Response>, RpcError> {
        Ok(self.handle_message(msg).0)
    }

    fn handle_message(&self, msg: Message) -> (Vec<Response>, bool) {
        match msg {
            Message::Single(Call::Request(req)) => (vec![self.handle_request(req)], false),
            Message::Single(Call::Notification(notif)) => {
                self.handle_notification(notif);
                (vec![], false)
            }
            Message::Single(Call::Invalid(_)) | Message::Invalid(_) => {
                (vec![protocol_error(ErrorCode::InvalidRequest)], false)
            }
            Message::Batch(calls) if calls.is_empty() => {
                (vec![protocol_error(ErrorCode::InvalidRequest)], false)
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
                        Call::Invalid(_) => {
                            responses.push(protocol_error(ErrorCode::InvalidRequest));
                        }
                    }
                }
                (responses, true)
            }
            Message::SingleResponse(_) => (vec![protocol_error(ErrorCode::InvalidRequest)], false),
            Message::BatchResponse(responses) => (
                responses
                    .into_iter()
                    .map(|_| protocol_error(ErrorCode::InvalidRequest))
                    .collect(),
                true,
            ),
        }
    }

    fn handle_request(&self, req: crate::protocol::Request) -> Response {
        let handler = match self.methods.get(&req.method) {
            Some(h) => h,
            None => {
                return Response::error(Error::new(ErrorCode::MethodNotFound), req.id);
            }
        };

        match handler(req.params, req.id.clone()) {
            Ok(result) if crate::serialization::validate_core_value(&result).is_ok() => {
                Response::success(result, req.id)
            }
            Ok(_) => Response::error(Error::new(ErrorCode::InternalError), req.id),
            Err(error) => Response::error(error_from_handler(error), req.id),
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

fn protocol_error(code: ErrorCode) -> Response {
    Response::error(Error::new(code), Id::Null)
}

fn error_from_handler(error: RpcError) -> Error {
    match error {
        RpcError::InvalidParams(message) => Error::with_message(ErrorCode::InvalidParams, message),
        RpcError::ServerError(offset, message) if ErrorCode::is_valid_server_offset(offset) => {
            Error::with_message(ErrorCode::ServerError(offset), message)
        }
        RpcError::ApplicationError(code, message) if !ErrorCode::is_reserved_code(code) => {
            Error::with_message(ErrorCode::Other(code), message)
        }
        RpcError::ResponseError(error) if handler_response_error_is_valid(&error) => error,
        RpcError::ParseError(_)
        | RpcError::InvalidRequest(_)
        | RpcError::MethodNotFound(_)
        | RpcError::InternalError(_)
        | RpcError::ServerError(_, _)
        | RpcError::ApplicationError(_, _)
        | RpcError::ResponseError(_)
        | RpcError::TransportError(_)
        | RpcError::SerializationError(_) => Error::new(ErrorCode::InternalError),
    }
}

fn handler_response_error_is_valid(error: &Error) -> bool {
    let code_is_valid = match error.code {
        ErrorCode::InvalidParams | ErrorCode::InternalError => true,
        ErrorCode::ServerError(offset) => ErrorCode::is_valid_server_offset(offset),
        ErrorCode::Other(code) => !ErrorCode::is_reserved_code(code),
        _ => false,
    };
    code_is_valid
        && error.data.as_ref().map_or(true, |data| {
            crate::serialization::validate_core_value(data).is_ok()
        })
}

fn ensure_toon_response_encodable(response: Response, in_batch: bool) -> Response {
    if crate::serialization::validate_response_depth(&response, in_batch).is_err() {
        return replace_with_internal_error(response);
    }
    let message = if in_batch {
        Message::BatchResponse(vec![response.clone()])
    } else {
        Message::SingleResponse(response.clone())
    };
    if crate::to_wire(&message).is_ok() {
        response
    } else {
        replace_with_internal_error(response)
    }
}

fn replace_with_internal_error(mut response: Response) -> Response {
    let id = response.id.clone();
    if let Some(result) = response.result.take() {
        drop_value_iteratively(result);
    }
    if let Some(data) = response.error.as_mut().and_then(|error| error.data.take()) {
        drop_value_iteratively(data);
    }
    Response::error(Error::new(ErrorCode::InternalError), id)
}

fn drop_value_iteratively(value: Value) {
    let mut pending = vec![value];
    while let Some(mut value) = pending.pop() {
        match &mut value {
            Value::Array(values) => pending.append(values),
            Value::Object(object) => {
                pending.extend(std::mem::take(object).into_values());
            }
            _ => {}
        }
    }
}

impl Default for Dispatcher {
    fn default() -> Self {
        Self::new()
    }
}
