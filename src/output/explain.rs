//! Output types for the `explain` command

use super::{CommandOutput, ExitCode, ExitStatus};
use serde::Serialize;

/// Result from the `explain` command
#[derive(Debug, Serialize)]
pub struct ExplainResult {
    /// The explanation of the code or concept
    pub explanation: String,
}

impl CommandOutput for ExplainResult {
    fn render_human(&self) -> String {
        self.explanation.clone()
    }
}

impl ExitStatus for ExplainResult {
    fn exit_code(&self) -> ExitCode {
        ExitCode::Success
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_explain_result_exit_status() {
        let result = ExplainResult {
            explanation: "This is how it works".to_string(),
        };
        assert_eq!(result.exit_code(), ExitCode::Success);
    }

    #[test]
    fn test_explain_result_human_output() {
        let result = ExplainResult {
            explanation: "This is the explanation".to_string(),
        };
        assert_eq!(result.render_human(), "This is the explanation");
    }
}
