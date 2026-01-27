pub mod gemini;
mod http;
pub mod ollama;
pub mod openai;
pub mod retry;

pub use http::{HttpClientConfig, SharedHttpClient};
pub use retry::{RetryConfig, with_retry};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::output::{ExitCode, ToExitCode};

#[derive(Error, Debug)]
pub enum LlmError {
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("Failed to parse response: {0}")]
    Parse(String),

    #[error("API error: {0}")]
    Api(String),

    #[error("request timed out after {0} seconds")]
    Timeout(u64),
}

impl ToExitCode for LlmError {
    fn to_exit_code(&self) -> ExitCode {
        match self {
            Self::Request(_) | Self::Timeout(_) => ExitCode::TransientError,
            Self::Parse(_) | Self::Api(_) => ExitCode::PermanentError,
        }
    }
}

impl LlmError {
    /// Returns whether this error is likely retryable
    ///
    /// Classification reasoning:
    /// - **Request/Timeout**: Network errors are transient (connection lost, DNS resolution failure, timeouts).
    ///   Waiting and retrying may succeed if the service recovers or network stabilizes.
    /// - **Parse**: Response parsing failures are permanent. If the LLM returned malformed content once,
    ///   retrying the same request will produce the same invalid output.
    /// - **Api**: Retryable only if the error indicates a transient server condition (rate limits,
    ///   temporary overload). Detected by HTTP status codes (429, 502, 503) or text patterns.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        match self {
            // Request/Timeout: Transient network failures
            Self::Request(_) | Self::Timeout(_) => true,
            // Parse: Permanent - malformed response won't improve on retry
            Self::Parse(_) => false,
            // Api: Retryable only if server indicates transient overload/rate-limit
            Self::Api(msg) => {
                let lower = msg.to_lowercase();
                // Check for rate limit or overload status codes and messages
                lower.contains("rate")
                    || lower.contains("limit")
                    || lower.contains("overload")
                    || lower.contains("capacity")
                    || lower.contains("429") // HTTP 429 Too Many Requests
                    || lower.contains("503") // HTTP 503 Service Unavailable
                    || lower.contains("502") // HTTP 502 Bad Gateway
            }
        }
    }
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

/// Options for LLM chat requests
#[derive(Debug, Clone, Default)]
pub struct ChatOptions {
    /// Enable JSON mode for structured output
    ///
    /// When enabled, the API enforces that the model outputs valid JSON.
    /// The prompt must still instruct the model about the expected JSON structure.
    pub json_mode: bool,
}

impl ChatOptions {
    /// Create options with JSON mode enabled
    #[must_use]
    pub fn json() -> Self {
        Self { json_mode: true }
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
        options: &ChatOptions,
    ) -> Result<LlmResponse, LlmError>;

    /// Get the model identifier
    fn model_name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_error_to_exit_code_parse() {
        let err = LlmError::Parse("bad json".to_string());
        assert_eq!(err.to_exit_code(), ExitCode::PermanentError);
    }

    #[test]
    fn test_llm_error_to_exit_code_api() {
        let err = LlmError::Api("auth failed".to_string());
        assert_eq!(err.to_exit_code(), ExitCode::PermanentError);
    }

    #[test]
    fn test_llm_error_to_exit_code_request() {
        let err = LlmError::Api("connection refused".to_string());
        assert_eq!(err.to_exit_code(), ExitCode::PermanentError);
    }

    #[test]
    fn test_llm_error_is_retryable() {
        // Parse errors are not retryable
        let parse_err = LlmError::Parse("invalid json".to_string());
        assert!(!parse_err.is_retryable());

        // Rate limit API errors are retryable
        let rate_err = LlmError::Api("rate limit exceeded".to_string());
        assert!(rate_err.is_retryable());

        let limit_err = LlmError::Api("Error 429: too many requests".to_string());
        assert!(limit_err.is_retryable());

        // Auth errors are not retryable
        let auth_err = LlmError::Api("invalid api key".to_string());
        assert!(!auth_err.is_retryable());

        // Timeout errors are retryable
        let timeout_err = LlmError::Timeout(60);
        assert!(timeout_err.is_retryable());
    }

    #[test]
    fn test_llm_error_to_exit_code_timeout() {
        let err = LlmError::Timeout(60);
        assert_eq!(err.to_exit_code(), ExitCode::TransientError);
    }

    #[test]
    fn test_llm_error_timeout_display() {
        let err = LlmError::Timeout(60);
        assert_eq!(err.to_string(), "request timed out after 60 seconds");
    }
}
