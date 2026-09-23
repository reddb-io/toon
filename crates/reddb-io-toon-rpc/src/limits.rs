//! Resource limits shared by every client, server and transport.
//!
//! The defaults match `packages/toon-rpc/src/limits.ts`. Going past a limit is
//! always visible: a defined RPC error, a refused HTTP request, a rejected
//! call, or a closed connection — never a silently dropped document.

use std::time::Duration;

use crate::framing::DEFAULT_MAX_FRAME_BYTES;

pub const DEFAULT_MAX_BODY_BYTES: usize = DEFAULT_MAX_FRAME_BYTES;
pub const DEFAULT_MAX_BATCH_LENGTH: usize = 1024;
pub const DEFAULT_MAX_PENDING_CALLS: usize = 1024;
pub const DEFAULT_MAX_CONNECTIONS: usize = 1024;
pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(300);
pub const DEFAULT_SHUTDOWN_GRACE: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    /// Largest stream frame, WebSocket message or SSE event, in bytes.
    pub max_frame_bytes: usize,
    /// Largest HTTP request (server) or response (client) body, in bytes.
    pub max_body_bytes: usize,
    /// Most entries a batch may hold; a longer batch is one Invalid Request.
    pub max_batch_length: usize,
    /// Most calls one client keeps pending; the next call is refused.
    pub max_pending_calls: usize,
    /// Most connections a server serves at once; it stops accepting beyond.
    pub max_connections: usize,
    /// A connection with no incoming document for this long is closed.
    pub idle_timeout: Option<Duration>,
    /// Default timeout for a client call that sets none of its own.
    pub request_timeout: Option<Duration>,
    /// How long a shutting-down server lets open connections finish.
    pub shutdown_grace: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_frame_bytes: DEFAULT_MAX_FRAME_BYTES,
            max_body_bytes: DEFAULT_MAX_BODY_BYTES,
            max_batch_length: DEFAULT_MAX_BATCH_LENGTH,
            max_pending_calls: DEFAULT_MAX_PENDING_CALLS,
            max_connections: DEFAULT_MAX_CONNECTIONS,
            idle_timeout: Some(DEFAULT_IDLE_TIMEOUT),
            request_timeout: None,
            shutdown_grace: DEFAULT_SHUTDOWN_GRACE,
        }
    }
}
