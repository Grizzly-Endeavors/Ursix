pub mod bash;
pub mod executor;
pub mod file;
pub mod search;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ToolError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    #[error("Tool execution failed: {0}")]
    ExecutionFailed(String),
}

/// Result of executing a tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Whether the tool executed successfully
    pub success: bool,

    /// Output from the tool (stdout for bash, file contents, etc.)
    pub output: String,

    /// Error message if the tool failed
    pub error: Option<String>,
}

impl ToolResult {
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            success: true,
            output: output.into(),
            error: None,
        }
    }

    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            success: false,
            output: String::new(),
            error: Some(error.into()),
        }
    }
}

/// Trait for tool implementations
pub trait Tool: Send + Sync {
    /// The name of the tool (used in LLM tool definitions)
    fn name(&self) -> &'static str;

    /// Description of what the tool does
    fn description(&self) -> &'static str;

    /// JSON schema for the tool's parameters
    fn parameters_schema(&self) -> serde_json::Value;
}
