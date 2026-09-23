use std::collections::HashMap;

pub mod client;
pub mod dispatcher;
pub mod error;
pub mod framing;
pub mod multi;
pub mod protocol;
pub mod serialization;
pub mod transport;
pub mod types;

pub use client::{
    CallOptions, Client, ClientDiagnostic, ClientError, ClientOptions, ClientStatus,
    DiagnosticHandler, DiagnosticReason,
};
pub use dispatcher::Dispatcher;
pub use error::{Error, ErrorCode, RpcError, RpcResult};
pub use framing::{encode_frame, FrameDecoder, FramingError, DEFAULT_MAX_FRAME_BYTES};
pub use multi::{detect_protocol, MultiRpc, Protocol};
pub use protocol::{Call, Message, Notification, Request, Response, TOONRPC_VERSION};
pub use serialization::{from_wire, response_from_wire, to_wire};
pub use transport::{
    dispatch_document, serve_framed, write_frame, DuplexTransport, FrameReader, FramedTransport,
    RequestResponseTransport,
};
pub use types::{Id, Method, Params, Value};

pub type RpcContext = HashMap<String, Value>;
