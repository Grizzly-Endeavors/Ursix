//! Output formatting for CLI commands
//!
//! Provides structured output types that can be rendered as either
//! human-readable text or JSON for scripting/automation.

use serde::Serialize;

/// Exit codes for CLI commands
///
/// Follows Unix conventions:
/// - 0 for success
/// - 1 for issues/warnings found (e.g., review found problems)
/// - 2 for errors (command failed to execute properly)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExitCode {
    /// Command completed successfully with no issues
    Success = 0,
    /// Command completed but found issues (e.g., review found problems)
    IssuesFound = 1,
    /// Command failed due to an error
    Error = 2,
}

impl From<ExitCode> for u8 {
    fn from(code: ExitCode) -> Self {
        code as u8
    }
}

impl From<ExitCode> for i32 {
    fn from(code: ExitCode) -> Self {
        i32::from(code as u8)
    }
}

/// Trait for types that can report an exit status
pub trait ExitStatus {
    /// Returns the exit code for this result
    fn exit_code(&self) -> ExitCode;
}

/// Output mode for command results
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    /// Human-readable text output
    Human,
    /// JSON output for scripting
    Json,
}

/// Trait for command outputs that can be rendered in multiple formats
pub trait CommandOutput: Serialize {
    /// Render as human-readable text
    fn render_human(&self) -> String;

    /// Render as JSON
    fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Render in the specified output mode
    fn render(&self, mode: OutputMode) -> String {
        match mode {
            OutputMode::Human => self.render_human(),
            OutputMode::Json => self.render_json(),
        }
    }
}

/// Result from the `ask` command
#[derive(Debug, Serialize)]
pub struct AskResult {
    /// The LLM's response
    pub response: String,
    /// Number of agent turns used
    pub turns: usize,
}

impl CommandOutput for AskResult {
    fn render_human(&self) -> String {
        self.response.clone()
    }
}

impl ExitStatus for AskResult {
    fn exit_code(&self) -> ExitCode {
        ExitCode::Success
    }
}

/// Result from the `config` command
#[derive(Debug, Serialize)]
pub struct ConfigResult {
    /// Configuration entries
    pub entries: Vec<ConfigEntry>,
}

/// A single configuration entry
#[derive(Debug, Serialize)]
pub struct ConfigEntry {
    /// Configuration key
    pub key: String,
    /// Configuration value
    pub value: String,
}

impl CommandOutput for ConfigResult {
    fn render_human(&self) -> String {
        self.entries
            .iter()
            .map(|e| format!("{} = {}", e.key, e.value))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl ExitStatus for ConfigResult {
    fn exit_code(&self) -> ExitCode {
        ExitCode::Success
    }
}

/// Result from the `models` command
#[derive(Debug, Serialize)]
pub struct ModelsResult {
    /// Available models
    pub models: Vec<String>,
}

impl CommandOutput for ModelsResult {
    fn render_human(&self) -> String {
        self.models.join("\n")
    }
}

impl ExitStatus for ModelsResult {
    fn exit_code(&self) -> ExitCode {
        ExitCode::Success
    }
}

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

/// Result from the `review` command
#[derive(Debug, Serialize)]
pub struct ReviewResult {
    /// The review summary
    pub summary: String,
    /// Identified issues
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<ReviewIssue>,
    /// Whether the review passed (no issues or only warnings)
    pub passed: bool,
}

/// An issue identified during code review
#[derive(Debug, Serialize)]
pub struct ReviewIssue {
    /// Issue severity (error, warning, info)
    pub severity: String,
    /// File path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Line number
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    /// Issue description
    pub message: String,
}

impl CommandOutput for ReviewResult {
    fn render_human(&self) -> String {
        self.summary.clone()
    }
}

impl ExitStatus for ReviewResult {
    fn exit_code(&self) -> ExitCode {
        if self.passed {
            ExitCode::Success
        } else {
            ExitCode::IssuesFound
        }
    }
}

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

/// Result from the `fix` command
#[derive(Debug, Serialize)]
pub struct FixResult {
    /// Changes made to fix issues
    pub changes: Vec<Change>,
    /// Number of issues that could not be fixed
    pub remaining_issues: usize,
}

/// A change made by the fix command
#[derive(Debug, Serialize)]
pub struct Change {
    /// Path to the file that was changed
    pub file_path: String,
    /// Description of the change
    pub description: String,
}

impl CommandOutput for FixResult {
    fn render_human(&self) -> String {
        use std::fmt::Write;
        let mut output = String::new();
        if self.changes.is_empty() {
            output.push_str("No changes made.");
        } else {
            output.push_str("Changes:\n");
            for change in &self.changes {
                let _ = writeln!(output, "  {} - {}", change.file_path, change.description);
            }
        }
        if self.remaining_issues > 0 {
            let _ = write!(
                output,
                "\n{} issues could not be fixed.",
                self.remaining_issues
            );
        }
        output
    }
}

impl ExitStatus for FixResult {
    fn exit_code(&self) -> ExitCode {
        if self.remaining_issues > 0 {
            ExitCode::IssuesFound
        } else {
            ExitCode::Success
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ask_result_human_output() {
        let result = AskResult {
            response: "Hello, world!".to_string(),
            turns: 1,
        };
        assert_eq!(result.render_human(), "Hello, world!");
    }

    #[test]
    fn test_ask_result_json_output() {
        let result = AskResult {
            response: "Hello".to_string(),
            turns: 2,
        };
        let json = result.render_json();
        assert!(json.contains("\"response\": \"Hello\""));
        assert!(json.contains("\"turns\": 2"));
    }

    #[test]
    fn test_config_result_human_output() {
        let result = ConfigResult {
            entries: vec![
                ConfigEntry {
                    key: "model".to_string(),
                    value: "llama3.2".to_string(),
                },
                ConfigEntry {
                    key: "max_turns".to_string(),
                    value: "50".to_string(),
                },
            ],
        };
        let output = result.render_human();
        assert!(output.contains("model = llama3.2"));
        assert!(output.contains("max_turns = 50"));
    }

    #[test]
    fn test_models_result_human_output() {
        let result = ModelsResult {
            models: vec!["llama3.2".to_string(), "qwen2.5-coder".to_string()],
        };
        let output = result.render_human();
        assert_eq!(output, "llama3.2\nqwen2.5-coder");
    }

    #[test]
    fn test_exit_code_values() {
        assert_eq!(u8::from(ExitCode::Success), 0);
        assert_eq!(u8::from(ExitCode::IssuesFound), 1);
        assert_eq!(u8::from(ExitCode::Error), 2);
    }

    #[test]
    fn test_exit_code_to_i32() {
        assert_eq!(i32::from(ExitCode::Success), 0);
        assert_eq!(i32::from(ExitCode::IssuesFound), 1);
        assert_eq!(i32::from(ExitCode::Error), 2);
    }

    #[test]
    fn test_ask_result_exit_status() {
        let result = AskResult {
            response: "test".to_string(),
            turns: 1,
        };
        assert_eq!(result.exit_code(), ExitCode::Success);
    }

    #[test]
    fn test_config_result_exit_status() {
        let result = ConfigResult { entries: vec![] };
        assert_eq!(result.exit_code(), ExitCode::Success);
    }

    #[test]
    fn test_models_result_exit_status() {
        let result = ModelsResult { models: vec![] };
        assert_eq!(result.exit_code(), ExitCode::Success);
    }

    #[test]
    fn test_commit_result_exit_status() {
        let result = CommitResult {
            message: "test".to_string(),
            title: "test".to_string(),
            body: None,
        };
        assert_eq!(result.exit_code(), ExitCode::Success);
    }

    #[test]
    fn test_review_result_exit_status_passed() {
        let result = ReviewResult {
            summary: "All good".to_string(),
            issues: vec![],
            passed: true,
        };
        assert_eq!(result.exit_code(), ExitCode::Success);
    }

    #[test]
    fn test_review_result_exit_status_failed() {
        let result = ReviewResult {
            summary: "Issues found".to_string(),
            issues: vec![ReviewIssue {
                severity: "error".to_string(),
                file: Some("test.rs".to_string()),
                line: Some(10),
                message: "bug".to_string(),
            }],
            passed: false,
        };
        assert_eq!(result.exit_code(), ExitCode::IssuesFound);
    }

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

    #[test]
    fn test_fix_result_exit_status_success() {
        let result = FixResult {
            changes: vec![Change {
                file_path: "test.rs".to_string(),
                description: "Fixed bug".to_string(),
            }],
            remaining_issues: 0,
        };
        assert_eq!(result.exit_code(), ExitCode::Success);
    }

    #[test]
    fn test_fix_result_exit_status_with_remaining() {
        let result = FixResult {
            changes: vec![],
            remaining_issues: 2,
        };
        assert_eq!(result.exit_code(), ExitCode::IssuesFound);
    }

    #[test]
    fn test_fix_result_human_output_with_changes() {
        let result = FixResult {
            changes: vec![
                Change {
                    file_path: "src/main.rs".to_string(),
                    description: "Fixed null pointer".to_string(),
                },
                Change {
                    file_path: "src/lib.rs".to_string(),
                    description: "Added error handling".to_string(),
                },
            ],
            remaining_issues: 0,
        };
        let output = result.render_human();
        assert!(output.contains("Changes:"));
        assert!(output.contains("src/main.rs - Fixed null pointer"));
        assert!(output.contains("src/lib.rs - Added error handling"));
    }

    #[test]
    fn test_fix_result_human_output_no_changes() {
        let result = FixResult {
            changes: vec![],
            remaining_issues: 0,
        };
        assert_eq!(result.render_human(), "No changes made.");
    }

    #[test]
    fn test_fix_result_human_output_with_remaining() {
        let result = FixResult {
            changes: vec![],
            remaining_issues: 3,
        };
        let output = result.render_human();
        assert!(output.contains("3 issues could not be fixed."));
    }
}
