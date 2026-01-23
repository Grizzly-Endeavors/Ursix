pub mod bash;
pub mod executor;
pub mod file;
pub mod search;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::output::{ExitCode, ToExitCode};

#[derive(Error, Debug)]
pub enum ToolError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    #[error("Tool execution failed: {0}")]
    ExecutionFailed(String),
}

impl ToExitCode for ToolError {
    fn to_exit_code(&self) -> ExitCode {
        match self {
            Self::Io(_) => ExitCode::InputError,
            Self::InvalidArgument(_) => ExitCode::UsageError,
            Self::ExecutionFailed(_) => ExitCode::InternalError,
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_error_to_exit_code_io() {
        let err = ToolError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "file not found",
        ));
        assert_eq!(err.to_exit_code(), ExitCode::InputError);
    }

    #[test]
    fn test_tool_error_to_exit_code_invalid_argument() {
        let err = ToolError::InvalidArgument("missing path".to_string());
        assert_eq!(err.to_exit_code(), ExitCode::UsageError);
    }

    #[test]
    fn test_tool_error_to_exit_code_execution_failed() {
        let err = ToolError::ExecutionFailed("command failed".to_string());
        assert_eq!(err.to_exit_code(), ExitCode::InternalError);
    }
}
