use std::collections::HashMap;

pub mod client;
pub mod dispatcher;
pub mod error;
pub mod multi;
pub mod protocol;
pub mod serialization;
pub mod types;

pub use client::{Client, ClientTransport};
pub use dispatcher::Dispatcher;
pub use error::{Error, ErrorCode, RpcError, RpcResult};
pub use multi::{detect_protocol, MultiRpc, Protocol};
pub use protocol::{Call, Message, Notification, Request, Response, TOONRPC_VERSION};
pub use serialization::{from_wire, to_wire};
pub use types::{Id, Method, Params, Value};

pub type RpcContext = HashMap<String, Value>;

pub trait Transport {
    type Send;
    type Recv;
    fn split(self) -> (Self::Send, Self::Recv);
}
