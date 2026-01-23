//! Output types for the `explain` command

use super::{CommandOutput, ExitCode, ExitStatus};
use serde::Serialize;

/// Result from the `explain` command
#[derive(Debug, Serialize)]
pub struct ExplainResult {
    /// The explanation of the code or concept
    pub explanation: String,
    /// Warning message if the LLM response could not be parsed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_warning: Option<String>,
}

impl CommandOutput for ExplainResult {
    fn render_human(&self) -> String {
        use std::fmt::Write;
        let mut output = String::new();

        // Show parse warning if present
        if let Some(ref warning) = self.parse_warning {
            let _ = writeln!(output, "Warning: {warning}\n");
        }

        output.push_str(&self.explanation);
        output
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
            parse_warning: None,
        };
        assert_eq!(result.exit_code(), ExitCode::Success);
    }

    #[test]
    fn test_explain_result_human_output() {
        let result = ExplainResult {
            explanation: "This is the explanation".to_string(),
            parse_warning: None,
        };
        assert_eq!(result.render_human(), "This is the explanation");
    }
}
