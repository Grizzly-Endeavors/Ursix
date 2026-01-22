pub mod ollama;
pub mod openai;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LlmError {
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("Failed to parse response: {0}")]
    Parse(String),

    #[error("API error: {0}")]
    Api(String),
}

/// A message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// A tool call requested by the LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Definition of an available tool (sent to LLM)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Response from the LLM
#[derive(Debug, Clone)]
pub struct LlmResponse {
    /// The assistant's text response (may be empty if only tool calls)
    pub content: String,

    /// Tool calls the assistant wants to make
    pub tool_calls: Vec<ToolCall>,

    /// Whether the response indicates completion (no tool calls, has content)
    pub is_complete: bool,
}

impl LlmResponse {
    pub fn new(content: String, tool_calls: Vec<ToolCall>) -> Self {
        let is_complete = tool_calls.is_empty() && !content.is_empty();
        Self {
            content,
            tool_calls,
            is_complete,
        }
    }
}

/// Trait for LLM client implementations
#[async_trait]
pub trait LlmClient: Send + Sync {
    /// Send a conversation to the LLM and get a response
    async fn chat(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<LlmResponse, LlmError>;

    /// Get the model identifier
    fn model_name(&self) -> &str;
}
