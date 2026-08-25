//! Client transport contracts.
//!
//! Section 8 of the protocol spec draws the processing boundary at complete RPC
//! documents. Two transport shapes sit under that boundary and they are not
//! interchangeable:
//!
//! - a [`DuplexTransport`] carries documents in both directions independently,
//!   so a response document can settle any pending call; and
//! - a [`RequestResponseTransport`] scopes each exchange to its own request, so
//!   the response of one exchange can never settle a different concurrent call.
//!
//! HTTP is the second shape and MUST NOT be dressed up as the first.

use async_trait::async_trait;

use crate::error::RpcError;

/// A transport-level failure. Carries a message rather than a source type so
/// every transport crate can report its own error without a shared dependency.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("TOON-RPC transport error: {message}")]
pub struct TransportError {
    message: String,
}

impl TransportError {
    /// Wrap any failure description as a transport error.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// The failure description, without the shared prefix.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl From<std::io::Error> for TransportError {
    fn from(error: std::io::Error) -> Self {
        Self::new(error.to_string())
    }
}

impl From<crate::framing::FramingError> for TransportError {
    fn from(error: crate::framing::FramingError) -> Self {
        Self::new(error.to_string())
    }
}

/// A framed transport that yields exactly one complete RPC document per item.
///
/// [`DuplexTransport::receive`] is the cursor over the inbound document stream:
/// the client owns exactly one receive pump and calls it in a loop, so each
/// call yields the next complete document and `Ok(None)` ends the stream.
/// [`DuplexTransport::close`] MUST make a pending or subsequent `receive`
/// terminate rather than hang.
#[async_trait]
pub trait DuplexTransport: Send + Sync + 'static {
    /// Establish the connection. Called at most once, before any send.
    async fn open(&self) -> Result<(), TransportError> {
        Ok(())
    }

    /// Write one complete RPC document.
    async fn send(&self, document: Vec<u8>) -> Result<(), TransportError>;

    /// Read the next complete RPC document; `Ok(None)` ends the stream.
    async fn receive(&self) -> Result<Option<Vec<u8>>, TransportError>;

    /// Release the connection and terminate the receive stream. Idempotent.
    async fn close(&self) -> Result<(), TransportError>;
}

/// A transport where each request directly owns its optional response document.
#[async_trait]
pub trait RequestResponseTransport: Send + Sync + 'static {
    /// Establish the connection. Called at most once, before any request.
    async fn open(&self) -> Result<(), TransportError> {
        Ok(())
    }

    /// Perform one exchange. `Ok(None)` is a response-free exchange, which is
    /// how a notification-only result is represented.
    async fn request(&self, document: Vec<u8>) -> Result<Option<Vec<u8>>, TransportError>;

    /// Release the connection. Idempotent.
    async fn close(&self) -> Result<(), TransportError>;
}

/// Legacy byte transport retained for the not-yet-recovered Rust transport
/// crates. It has no document framing and no correlation contract; slices 9
/// and 10 of the recovery replace its implementors with [`DuplexTransport`]
/// and [`RequestResponseTransport`]. New code MUST NOT use it.
#[async_trait]
pub trait ClientTransport: Send + Sync {
    /// Write raw bytes.
    async fn send(&self, data: Vec<u8>) -> Result<(), RpcError>;
    /// Read raw bytes.
    async fn recv(&self) -> Result<Vec<u8>, RpcError>;
}
