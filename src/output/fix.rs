//! Output types for the `fix` command
//!
//! The fix command produces either a successful diff result or
//! a structured error with diagnostic information.

use super::{CommandOutput, ExitCode, ExitStatus};
use serde::Serialize;

/// Successful result from the `fix` command
#[derive(Debug, Clone, Serialize)]
pub struct FixResult {
    /// Unified diff string
    pub diff: String,
    /// Path to the file being fixed
    pub file: String,
    /// Line range that was fixed (start, end)
    pub lines: (usize, usize),
    /// Number of lines added
    pub lines_added: usize,
    /// Number of lines removed
    pub lines_removed: usize,
    /// Warnings from validation (non-blocking)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// Error codes for the fix command
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FixErrorCode {
    /// Input validation failed (bad JSON, missing fields, etc.)
    InputValidation,
    /// LLM call failed (network, rate limit, etc.)
    LlmError,
    /// Output validation failed (bad LLM response)
    OutputValidation,
}

impl FixErrorCode {
    /// Get the corresponding exit code for this error
    #[must_use]
    pub const fn to_exit_code(self) -> ExitCode {
        match self {
            Self::InputValidation => ExitCode::UserError,
            Self::LlmError => ExitCode::TransientError,
            Self::OutputValidation => ExitCode::PermanentError,
        }
    }
}

/// Error result from the fix command
#[derive(Debug, Clone, Serialize)]
pub struct FixError {
    /// Human-readable error message
    pub message: String,
    /// Error code for programmatic handling
    pub code: FixErrorCode,
    /// Raw LLM output (only with --partial flag)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_output: Option<String>,
    /// Validation warnings encountered
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

impl FixError {
    /// Create an input validation error
    #[must_use]
    pub fn input_validation(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: FixErrorCode::InputValidation,
            raw_output: None,
            warnings: Vec::new(),
        }
    }

    /// Create an LLM error
    #[must_use]
    pub fn llm_error(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: FixErrorCode::LlmError,
            raw_output: None,
            warnings: Vec::new(),
        }
    }

    /// Create an output validation error
    #[must_use]
    pub fn output_validation(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: FixErrorCode::OutputValidation,
            raw_output: None,
            warnings: Vec::new(),
        }
    }

    /// Add raw LLM output for --partial mode
    #[must_use]
    pub fn with_raw_output(mut self, output: impl Into<String>) -> Self {
        self.raw_output = Some(output.into());
        self
    }

    /// Add validation warnings
    #[must_use]
    pub fn with_warnings(mut self, warnings: Vec<String>) -> Self {
        self.warnings = warnings;
        self
    }
}

impl CommandOutput for FixResult {
    fn render_human(&self) -> String {
        use std::fmt::Write;
        let mut output = String::new();

        // Show file info
        let _ = writeln!(
            output,
            "Fixed {} (lines {}..{})",
            self.file, self.lines.0, self.lines.1
        );
        let _ = writeln!(
            output,
            "+{} -{} lines\n",
            self.lines_added, self.lines_removed
        );

        // Show warnings if any
        for warning in &self.warnings {
            let _ = writeln!(output, "warning: {warning}");
        }
        if !self.warnings.is_empty() {
            output.push('\n');
        }

        // Show the diff
        output.push_str(&self.diff);

        output
    }
}

impl ExitStatus for FixResult {
    fn exit_code(&self) -> ExitCode {
        ExitCode::Success
    }
}

impl CommandOutput for FixError {
    fn render_human(&self) -> String {
        use std::fmt::Write;
        let mut output = String::new();

        let _ = writeln!(output, "error: {}", self.message);

        for warning in &self.warnings {
            let _ = writeln!(output, "warning: {warning}");
        }

        if let Some(ref raw) = self.raw_output {
            let _ = writeln!(output, "\nraw output:\n{raw}");
        }

        output
    }
}

impl ExitStatus for FixError {
    fn exit_code(&self) -> ExitCode {
        self.code.to_exit_code()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_fix_result_exit_code() {
        let result = FixResult {
            diff: "--- a/test.rs\n+++ b/test.rs".to_string(),
            file: "test.rs".to_string(),
            lines: (10, 12),
            lines_added: 1,
            lines_removed: 2,
            warnings: vec![],
        };
        assert_eq!(result.exit_code(), ExitCode::Success);
    }

    #[test]
    fn test_fix_result_human_output() {
        let result = FixResult {
            diff: "--- a/test.rs\n+++ b/test.rs\n@@ -1 +1 @@\n-old\n+new".to_string(),
            file: "test.rs".to_string(),
            lines: (1, 1),
            lines_added: 1,
            lines_removed: 1,
            warnings: vec![],
        };
        let output = result.render_human();
        assert!(output.contains("Fixed test.rs (lines 1..1)"));
        assert!(output.contains("+1 -1 lines"));
        assert!(output.contains("--- a/test.rs"));
    }

    #[test]
    fn test_fix_result_human_output_with_warnings() {
        let result = FixResult {
            diff: "diff".to_string(),
            file: "test.rs".to_string(),
            lines: (1, 1),
            lines_added: 0,
            lines_removed: 0,
            warnings: vec!["possible unbalanced delimiters".to_string()],
        };
        let output = result.render_human();
        assert!(output.contains("warning: possible unbalanced delimiters"));
    }

    #[test]
    fn test_fix_error_input_validation() {
        let error = FixError::input_validation("invalid JSON");
        assert_eq!(error.code, FixErrorCode::InputValidation);
        assert_eq!(error.exit_code(), ExitCode::UserError);
    }

    #[test]
    fn test_fix_error_llm_error() {
        let error = FixError::llm_error("network timeout");
        assert_eq!(error.code, FixErrorCode::LlmError);
        assert_eq!(error.exit_code(), ExitCode::TransientError);
    }

    #[test]
    fn test_fix_error_output_validation() {
        let error = FixError::output_validation("replacement too long");
        assert_eq!(error.code, FixErrorCode::OutputValidation);
        assert_eq!(error.exit_code(), ExitCode::PermanentError);
    }

    #[test]
    fn test_fix_error_with_raw_output() {
        let error = FixError::output_validation("bad response")
            .with_raw_output("Here's the fix: let x = 1;");
        assert!(error.raw_output.is_some());
        let output = error.render_human();
        assert!(output.contains("raw output:"));
        assert!(output.contains("Here's the fix"));
    }

    #[test]
    fn test_fix_error_with_warnings() {
        let error = FixError::output_validation("validation failed")
            .with_warnings(vec!["warning 1".to_string(), "warning 2".to_string()]);
        assert_eq!(error.warnings.len(), 2);
        let output = error.render_human();
        assert!(output.contains("warning: warning 1"));
        assert!(output.contains("warning: warning 2"));
    }

    #[test]
    fn test_fix_error_code_serialization() {
        let error = FixError::input_validation("test");
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("\"input_validation\""));
    }

    #[test]
    fn test_fix_result_json_skips_empty_warnings() {
        let result = FixResult {
            diff: "diff".to_string(),
            file: "test.rs".to_string(),
            lines: (1, 1),
            lines_added: 0,
            lines_removed: 0,
            warnings: vec![],
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(!json.contains("warnings"));
    }
}
