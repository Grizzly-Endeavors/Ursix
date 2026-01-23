//! Output types for the `commit` command

use super::{CommandOutput, ExitCode, ExitStatus};
use serde::Serialize;

/// Result from the `commit` command
#[derive(Debug, Serialize)]
pub struct CommitResult {
    /// The generated commit message
    pub message: String,
    /// Commit title (first line)
    pub title: String,
    /// Commit body (remaining lines)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

impl CommandOutput for CommitResult {
    fn render_human(&self) -> String {
        self.message.clone()
    }
}

impl ExitStatus for CommitResult {
    fn exit_code(&self) -> ExitCode {
        ExitCode::Success
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_commit_result_exit_status() {
        let result = CommitResult {
            message: "test".to_string(),
            title: "test".to_string(),
            body: None,
        };
        assert_eq!(result.exit_code(), ExitCode::Success);
    }
}
