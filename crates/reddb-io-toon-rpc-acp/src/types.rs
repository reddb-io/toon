//! ACP types — Agent, Message, Run, Content
//!
//! Prototype models pending validation against the pinned terminal legacy ACP
//! contract. TOON serialization is a project extension, not ACP conformance.

use serde::{Deserialize, Serialize};

/// ACP API version
pub const ACP_API_VERSION: &str = "0.1.0";

/// Status of an agent run
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Created,
    InProgress,
    Awaiting,
    Cancelled,
    Failed,
    Completed,
}

/// Status of a message part
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessagePartStatus {
    InProgress,
    Done,
    Failed,
}

/// The kind of a message part (what kind of content it carries)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessagePartKind {
    Text,
    File,
    Data,
    Resource,
    ResourceLink,
}

/// Agent metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Output produced by an agent run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub role: String,
    pub parts: Vec<MessagePart>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// A single content part inside a message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagePart {
    pub kind: MessagePartKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_encoding: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_url: Option<String>,
    pub status: MessagePartStatus,
}

/// Input for an agent run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRunInput {
    pub parts: Vec<MessagePart>,
}

/// State of an agent run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRun {
    #[serde(rename = "agentRunId")]
    pub agent_run_id: String,
    #[serde(rename = "agentName")]
    pub agent_name: String,
    pub status: RunStatus,
    pub input: AgentRunInput,
    pub output: Vec<AgentMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<AgentError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Error raised by an agent run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Agent summary (used in list endpoints)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSummary {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

impl AgentRun {
    pub fn completed(
        id: impl Into<String>,
        agent: impl Into<String>,
        output: Vec<AgentMessage>,
    ) -> Self {
        Self {
            agent_run_id: id.into(),
            agent_name: agent.into(),
            status: RunStatus::Completed,
            input: AgentRunInput { parts: vec![] },
            output,
            error: None,
            metadata: None,
        }
    }

    pub fn failed(
        id: impl Into<String>,
        agent: impl Into<String>,
        code: i32,
        msg: impl Into<String>,
    ) -> Self {
        Self {
            agent_run_id: id.into(),
            agent_name: agent.into(),
            status: RunStatus::Failed,
            input: AgentRunInput { parts: vec![] },
            output: vec![],
            error: Some(AgentError {
                code,
                message: msg.into(),
                data: None,
            }),
            metadata: None,
        }
    }
}

impl MessagePart {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            kind: MessagePartKind::Text,
            content_type: Some("text/plain".into()),
            content: Some(serde_json::Value::String(text.into())),
            content_encoding: None,
            content_url: None,
            status: MessagePartStatus::Done,
        }
    }
}
