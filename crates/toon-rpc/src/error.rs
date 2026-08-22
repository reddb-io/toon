#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    ParseError,
    InvalidRequest,
    MethodNotFound,
    InvalidParams,
    InternalError,
    ServerError(i16),
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
        }
    }

    pub fn from_code(code: i32) -> Option<Self> {
        match code {
            -32700 => Some(ErrorCode::ParseError),
            -32600 => Some(ErrorCode::InvalidRequest),
            -32601 => Some(ErrorCode::MethodNotFound),
            -32602 => Some(ErrorCode::InvalidParams),
            -32603 => Some(ErrorCode::InternalError),
            _ if code >= -32099 && code <= -32000 => {
                Some(ErrorCode::ServerError((code + 32000) as i16))
            }
            _ => None,
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            ErrorCode::ParseError => "Parse error",
            ErrorCode::InvalidRequest => "Invalid Request",
            ErrorCode::MethodNotFound => "Method not found",
            ErrorCode::InvalidParams => "Invalid params",
            ErrorCode::InternalError => "Internal error",
            ErrorCode::ServerError(_) => "Server error",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Error {
    pub code: ErrorCode,
    pub message: String,
    pub data: Option<super::Value>,
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

    pub fn with_data(code: ErrorCode, data: super::Value) -> Self {
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
    #[error("transport error: {0}")]
    TransportError(String),
    #[error("serialization error: {0}")]
    SerializationError(String),
}

pub type RpcResult<T> = Result<T, RpcError>;

impl From<Error> for RpcError {
    fn from(e: Error) -> Self {
        match e.code {
            ErrorCode::ParseError => RpcError::ParseError(e.message),
            ErrorCode::InvalidRequest => RpcError::InvalidRequest(e.message),
            ErrorCode::MethodNotFound => RpcError::MethodNotFound(e.message),
            ErrorCode::InvalidParams => RpcError::InvalidParams(e.message),
            ErrorCode::InternalError => RpcError::InternalError(e.message),
            ErrorCode::ServerError(n) => RpcError::ServerError(n, e.message),
        }
    }
}
