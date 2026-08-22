use serde::{Deserialize, Serialize};
use super::types::{Id, Params, Value};
use super::error::Error;

pub const TOONRPC_VERSION: &str = "1.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    #[serde(rename = "toonrpc")]
    pub toonrpc: String,
    pub method: String,
    pub params: Params,
    pub id: Id,
}

impl Request {
    pub fn new(method: String, params: Params, id: Id) -> Self {
        Self {
            toonrpc: TOONRPC_VERSION.to_string(),
            method,
            params,
            id,
        }
    }

    pub fn is_notification(&self) -> bool {
        self.id.is_notification()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    #[serde(rename = "toonrpc")]
    pub toonrpc: String,
    pub method: String,
    pub params: Params,
}

impl Notification {
    pub fn new(method: String, params: Params) -> Self {
        Self {
            toonrpc: TOONRPC_VERSION.to_string(),
            method,
            params,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Call {
    Request(Request),
    Notification(Notification),
}

impl Call {
    pub fn method(&self) -> &str {
        match self {
            Call::Request(r) => &r.method,
            Call::Notification(n) => &n.method,
        }
    }

    pub fn params(&self) -> &Params {
        match self {
            Call::Request(r) => &r.params,
            Call::Notification(n) => &n.params,
        }
    }

    pub fn id(&self) -> Option<&Id> {
        match self {
            Call::Request(r) => Some(&r.id),
            Call::Notification(_) => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    #[serde(rename = "toonrpc")]
    pub toonrpc: String,
    pub result: Option<Value>,
    pub error: Option<Error>,
    pub id: Id,
}

impl Response {
    pub fn success(result: Value, id: Id) -> Self {
        Self {
            toonrpc: TOONRPC_VERSION.to_string(),
            result: Some(result),
            error: None,
            id,
        }
    }

    pub fn error(error: Error, id: Id) -> Self {
        Self {
            toonrpc: TOONRPC_VERSION.to_string(),
            result: None,
            error: Some(error),
            id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Message {
    Single(Call),
    Batch(Vec<Call>),
    SingleResponse(Response),
    BatchResponse(Vec<Response>),
}

impl Message {
    pub fn into_response(self) -> Option<Response> {
        match self {
            Message::SingleResponse(r) => Some(r),
            _ => None,
        }
    }
}
