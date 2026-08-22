use std::collections::HashMap;

pub mod error;
pub mod protocol;
pub mod types;

pub use error::{Error, ErrorCode, RpcResult};
pub use protocol::{Call, Message, Notification, Request, Response, TOONRPC_VERSION};
pub use types::{Id, Method, Params, Value};

pub type RpcContext = HashMap<String, Value>;

pub trait Transport {
    type Send;
    type Recv;
    fn split(self) -> (Self::Send, Self::Recv);
}
