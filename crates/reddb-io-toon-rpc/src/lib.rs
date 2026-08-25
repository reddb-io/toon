use std::collections::HashMap;

pub mod cancel;
pub mod client;
pub mod dispatcher;
pub mod error;
pub mod framing;
pub mod multi;
pub mod protocol;
pub mod serialization;
pub mod transport;
pub mod types;

pub use cancel::CancelToken;
pub use client::{
    CallOptions, Client, ClientDiagnostic, ClientError, ClientOptions, ClientStatus,
    DiagnosticReason, NotifyOptions,
};
pub use dispatcher::Dispatcher;
pub use error::{Error, ErrorCode, RpcError, RpcResult};
pub use framing::{encode_frame, FrameDecoder, FramingError};
pub use multi::{detect_protocol, MultiRpc, Protocol};
pub use protocol::{Call, Message, Notification, Request, Response, TOONRPC_VERSION};
pub use serialization::{from_wire, response_from_value, response_from_wire, to_wire};
pub use transport::{ClientTransport, DuplexTransport, RequestResponseTransport, TransportError};
pub use types::{Id, Method, Params, Value};

pub type RpcContext = HashMap<String, Value>;

pub trait Transport {
    type Send;
    type Recv;
    fn split(self) -> (Self::Send, Self::Recv);
}
