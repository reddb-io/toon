use super::types::Value;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    ParseError,
    InvalidRequest,
    MethodNotFound,
    InvalidParams,
    InternalError,
    ServerError(i16),
    Other(i32),
}

// Serialize as the numeric code so JSON-RPC and TOON-RPC wire formats both
// carry `-32601` instead of the variant name.
impl Serialize for ErrorCode {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            ErrorCode::ServerError(offset) if !Self::is_valid_server_offset(*offset) => {
                return Err(serde::ser::Error::custom(
                    "server error offset must be between 0 and 99",
                ));
            }
            ErrorCode::Other(code) if !matches!(Self::from_code(*code), Some(ErrorCode::Other(mapped)) if mapped == *code) =>
            {
                return Err(serde::ser::Error::custom(
                    "error code must use its canonical named variant",
                ));
            }
            _ => {}
        }
        s.serialize_i32(self.code())
    }
}

impl<'de> Deserialize<'de> for ErrorCode {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let code = i32::deserialize(d)?;
        ErrorCode::from_code(code)
            .ok_or_else(|| serde::de::Error::custom("error code must be a signed 32-bit integer"))
    }
}

impl ErrorCode {
    pub fn code(&self) -> i32 {
        match self {
            ErrorCode::ParseError => -32700,
            ErrorCode::InvalidRequest => -32600,
            ErrorCode::MethodNotFound => -32601,
            ErrorCode::InvalidParams => -32602,
            ErrorCode::InternalError => -32603,
            ErrorCode::ServerError(n) => -32000 - *n as i32,
            ErrorCode::Other(code) => *code,
        }
    }

    pub fn from_code(code: i32) -> Option<Self> {
        Some(match code {
            -32700 => ErrorCode::ParseError,
            -32600 => ErrorCode::InvalidRequest,
            -32601 => ErrorCode::MethodNotFound,
            -32602 => ErrorCode::InvalidParams,
            -32603 => ErrorCode::InternalError,
            _ if (-32099..=-32000).contains(&code) => {
                ErrorCode::ServerError((-32000 - code) as i16)
            }
            _ => ErrorCode::Other(code),
        })
    }

    pub fn message(&self) -> &'static str {
        match self {
            ErrorCode::ParseError => "Parse error",
            ErrorCode::InvalidRequest => "Invalid Request",
            ErrorCode::MethodNotFound => "Method not found",
            ErrorCode::InvalidParams => "Invalid params",
            ErrorCode::InternalError => "Internal error",
            ErrorCode::ServerError(_) => "Server error",
            ErrorCode::Other(_) => "Application error",
        }
    }

    pub(crate) fn is_valid_server_offset(offset: i16) -> bool {
        (0..=99).contains(&offset)
    }

    pub(crate) fn is_reserved_code(code: i32) -> bool {
        (-32768..=-32000).contains(&code)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Error {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl<'de> Deserialize<'de> for Error {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        let mut object = value
            .as_object()
            .cloned()
            .ok_or_else(|| serde::de::Error::custom("error must be an object"))?;
        let code = object
            .remove("code")
            .ok_or_else(|| serde::de::Error::missing_field("code"))?;
        let code = serde_json::from_value(code).map_err(serde::de::Error::custom)?;
        let message = object
            .remove("message")
            .and_then(|value| value.as_str().map(str::to_owned))
            .ok_or_else(|| serde::de::Error::custom("error message must be a string"))?;
        let data = object.remove("data");

        Ok(Self {
            code,
            message,
            data,
        })
    }
}

impl Error {
    pub fn new(code: ErrorCode) -> Self {
        Self {
            code,
            message: code.message().to_string(),
            data: None,
        }
    }

    pub fn with_message(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(code: ErrorCode, data: Value) -> Self {
        Self {
            code,
            message: code.message().to_string(),
            data: Some(data),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.message, self.code.code())
    }
}

impl std::error::Error for Error {}

#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    #[error("parse error: {0}")]
    ParseError(String),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("method not found: {0}")]
    MethodNotFound(String),
    #[error("invalid params: {0}")]
    InvalidParams(String),
    #[error("internal error: {0}")]
    InternalError(String),
    #[error("server error: {0}")]
    ServerError(i16, String),
    #[error("application error: {1}")]
    ApplicationError(i32, String),
    #[error(transparent)]
    ResponseError(#[from] Error),
    #[error("transport error: {0}")]
    TransportError(String),
    #[error("serialization error: {0}")]
    SerializationError(String),
}

pub type RpcResult<T> = Result<T, RpcError>;
