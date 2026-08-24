use super::error::Error;
use super::types::{Id, Params, Value};
use serde::{Deserialize, Serialize};

pub const TOONRPC_VERSION: &str = "1.0";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    #[serde(rename = "toonrpc")]
    pub toonrpc: String,
    pub method: String,
    #[serde(default, skip_serializing_if = "Params::is_absent")]
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
        false
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notification {
    #[serde(rename = "toonrpc")]
    pub toonrpc: String,
    pub method: String,
    #[serde(default, skip_serializing_if = "Params::is_absent")]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Call {
    Request(Request),
    Notification(Notification),
    #[serde(skip)]
    Invalid(String),
}

impl Call {
    pub fn method(&self) -> &str {
        match self {
            Call::Request(r) => &r.method,
            Call::Notification(n) => &n.method,
            Call::Invalid(_) => "",
        }
    }

    pub fn params(&self) -> &Params {
        match self {
            Call::Request(r) => &r.params,
            Call::Notification(n) => &n.params,
            Call::Invalid(_) => &Params::Absent,
        }
    }

    pub fn id(&self) -> Option<&Id> {
        match self {
            Call::Request(r) => Some(&r.id),
            Call::Notification(_) | Call::Invalid(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Response {
    #[serde(rename = "toonrpc")]
    pub toonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Error>,
    pub id: Id,
}

impl<'de> Deserialize<'de> for Response {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let mut object = value
            .as_object()
            .cloned()
            .ok_or_else(|| serde::de::Error::custom("response must be an object"))?;

        match object.remove("toonrpc") {
            Some(Value::String(version)) if version == TOONRPC_VERSION => {}
            _ => return Err(serde::de::Error::custom("invalid toonrpc version")),
        }
        let id = object
            .remove("id")
            .ok_or_else(|| serde::de::Error::missing_field("id"))?;
        let id = serde_json::from_value(id).map_err(serde::de::Error::custom)?;
        let result = object.remove("result");
        let error = object.remove("error");

        match (result, error) {
            (Some(result), None) => Ok(Self::success(result, id)),
            (None, Some(error)) => {
                let error = serde_json::from_value(error).map_err(serde::de::Error::custom)?;
                Ok(Self::error(error, id))
            }
            _ => Err(serde::de::Error::custom(
                "response must contain exactly one of result and error",
            )),
        }
    }
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Message {
    Single(Call),
    Batch(Vec<Call>),
    SingleResponse(Response),
    BatchResponse(Vec<Response>),
    #[serde(skip)]
    Invalid(String),
}

impl Message {
    pub fn into_response(self) -> Option<Response> {
        match self {
            Message::SingleResponse(r) => Some(r),
            _ => None,
        }
    }
}
