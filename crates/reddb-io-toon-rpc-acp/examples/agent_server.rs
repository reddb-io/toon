//! Echo ACP agent — exposes a single "echo" agent that repeats the user's
//! message back.
//!
//! Run with: cargo run --example agent_server -p reddb-io-toon-rpc-acp
//!
//! Try:
//!   curl http://127.0.0.1:9000/agents
//!   curl -X POST http://127.0.0.1:9000/agents/echo/runs \
//!        -H 'Content-Type: application/json' \
//!        -H 'Accept: application/toon' \
//!        -d '{"parts":[{"kind":"text","content_type":"text/plain","content":"hello","status":"done"}]}'

use reddb_io_toon_rpc_acp::{AcpService, Agent, AgentMessage, AgentRun, MessagePart};
use serde_json::Value;
use std::net::SocketAddr;

struct EchoAgent;

impl AcpService for EchoAgent {
    fn list_agents(&self) -> Vec<Agent> {
        vec![Agent {
            name: "echo".into(),
            description: "Echoes the user's message back as a single assistant turn.".into(),
            version: Some("0.29.0".into()),
            metadata: None,
        }]
    }

    fn run(&self, agent: &str, input_parts: Vec<MessagePart>) -> AgentRun {
        let text: String = input_parts
            .iter()
            .filter_map(|p| match &p.content {
                Some(Value::String(s)) => Some(s.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ");

        AgentRun::completed(
            uuid::Uuid::new_v4().to_string(),
            agent,
            vec![AgentMessage {
                role: "assistant".into(),
                parts: vec![MessagePart::text(if text.is_empty() {
                    "(empty message)".into()
                } else {
                    format!("echo: {}", text)
                })],
                metadata: None,
            }],
        )
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr: SocketAddr = "127.0.0.1:9000".parse()?;
    reddb_io_toon_rpc_acp::serve_http(EchoAgent, addr).await
}
