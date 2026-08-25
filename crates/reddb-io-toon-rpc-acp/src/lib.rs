//! Legacy ACP-style REST adapter — **terminal contract, no new features**.
//!
//! This crate serves *this repository's own* legacy ACP-style REST shape. It
//! is **not** IBM/BeeAI's Agent Communication Protocol and **not** Zed's Agent
//! Client Protocol, and it is not interoperable with either: the run envelope
//! (`agentRunId`, `agentName`), the message-part model and the run-status
//! vocabulary were invented here.
//!
//! The wire contract is pinned by `docs/acp-legacy-openapi.yaml` at the
//! repository root and is frozen: this crate accepts correctness, safety and
//! lifecycle fixes that keep those shapes byte-identical, and nothing else.
//! New agent-protocol work belongs on a different, non-legacy surface.
//!
//! What it provides:
//!
//! - [`AcpService`] trait for implementing agents
//! - REST endpoints: `GET /agents`, `POST /agents/:name/runs`, etc.
//! - TOON serialization on every response when the client opts in
//! - bounded run retention ([`RunStore`]) so finished runs are releasable
//!
//! # Example
//!
//! ```no_run
//! use reddb_io_toon_rpc_acp::{AcpService, Agent, AgentMessage, AgentRun, MessagePart, serve_http};
//!
//! struct EchoAgent;
//!
//! impl AcpService for EchoAgent {
//!     fn list_agents(&self) -> Vec<Agent> {
//!         vec![Agent {
//!             name: "echo".into(),
//!             description: "Echoes the user's message back.".into(),
//!             version: Some("0.1.0".into()),
//!             metadata: None,
//!         }]
//!     }
//!     fn run(&self, agent: &str, input_parts: Vec<MessagePart>) -> AgentRun {
//!         let text = input_parts.iter()
//!             .filter_map(|p| p.content.as_ref().and_then(|v| v.as_str()))
//!             .collect::<Vec<_>>()
//!             .join(" ");
//!         AgentRun::completed(
//!             uuid::Uuid::new_v4().to_string(),
//!             agent,
//!             vec![AgentMessage {
//!                 role: "assistant".into(),
//!                 parts: vec![MessagePart::text(text)],
//!                 metadata: None,
//!             }],
//!         )
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     serve_http(EchoAgent, "0.0.0.0:9000".parse().unwrap()).await.unwrap();
//! }
//! ```

pub mod http;
pub mod runs;
pub mod types;

pub use http::{serve_http, serve_listener, AcpHttpConfig};
pub use runs::{is_terminal, RunStore, DEFAULT_MAX_RUNS};
pub use types::{
    Agent, AgentError, AgentMessage, AgentRun, AgentRunInput, AgentSummary, MessagePart,
    MessagePartKind, MessagePartStatus, RunStatus, ACP_API_VERSION,
};

use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AcpError {
    #[error("agent not found: {0}")]
    AgentNotFound(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("internal error: {0}")]
    Internal(String),
}

pub type AcpResult<T> = Result<T, AcpError>;

/// Trait for implementing an ACP agent.
pub trait AcpService: Send + Sync + 'static {
    /// List all agents this server exposes.
    fn list_agents(&self) -> Vec<Agent>;

    /// Get a single agent by name.
    fn get_agent(&self, name: &str) -> Option<Agent> {
        self.list_agents().into_iter().find(|a| a.name == name)
    }

    /// Run an agent synchronously and return the result.
    ///
    /// This is allowed to block for the whole length of an agent run: the HTTP
    /// transport calls it on the blocking pool, never on an async worker.
    fn run(&self, agent: &str, input_parts: Vec<MessagePart>) -> AgentRun;

    /// Cancel a running agent. Default: not implemented.
    ///
    /// This hook is only consulted for runs that are still live. A run that
    /// already reached a terminal state is released by the transport without
    /// calling it, so `DELETE /runs/{id}` on a finished run always succeeds.
    fn cancel(&self, _run_id: &str) -> AcpResult<()> {
        Err(AcpError::Internal("cancel not supported".into()))
    }
}

/// Shared handle to an ACP service.
pub type SharedAcp<S> = Arc<S>;
