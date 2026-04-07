pub mod openai;

pub use openai::OpenAICompatible;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::error::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// JSON string of arguments (as the model emitted them).
    pub arguments: String,
}

#[derive(Debug, Clone)]
pub enum ProviderEvent {
    ContentDelta(String),
    /// A fully assembled tool call, emitted once its arguments have finished streaming.
    ToolCall(ToolCall),
    Done,
    Error(String),
}

/// A single message in the chat history sent to the provider.
#[derive(Debug, Clone)]
pub struct Message {
    pub role: String, // "system" | "user" | "assistant" | "tool"
    pub content: String,
    /// For assistant messages that requested tools.
    pub tool_calls: Vec<ToolCall>,
    /// For role="tool" messages: which call this is responding to.
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: "system".into(), content: content.into(), tool_calls: vec![], tool_call_id: None }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user".into(), content: content.into(), tool_calls: vec![], tool_call_id: None }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: "assistant".into(), content: content.into(), tool_calls: vec![], tool_call_id: None }
    }
    pub fn assistant_with_tools(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self { role: "assistant".into(), content: content.into(), tool_calls, tool_call_id: None }
    }
    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".into(),
            content: content.into(),
            tool_calls: vec![],
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

#[async_trait]
pub trait Provider: Send + Sync {
    /// Stream a response. The returned receiver yields `ProviderEvent`s until `Done` or `Error`.
    async fn stream(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[Value],
    ) -> Result<tokio::sync::mpsc::Receiver<ProviderEvent>, Box<dyn Error + Send + Sync>>;
}
