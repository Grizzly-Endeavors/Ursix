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
    ///
    /// # Errors
    /// Returns an error if serialization fails.
    fn render_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Render in the specified output mode
    ///
    /// For JSON mode, serialization errors are reported in the output string
    /// rather than silently returning an empty object.
    fn render(&self, mode: OutputMode) -> String {
        match mode {
            OutputMode::Human => self.render_human(),
            OutputMode::Json => self
                .render_json()
                .unwrap_or_else(|e| format!("{{\"error\": \"serialization failed: {e}\"}}")),
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
    /// Warning message if the LLM response could not be parsed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_warning: Option<String>,
    /// Raw LLM response (included when parsing fails)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_response: Option<String>,
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
    /// Rule name that triggered this issue (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
}

impl CommandOutput for ReviewResult {
    fn render_human(&self) -> String {
        use std::fmt::Write;
        let mut output = String::new();

        // Show parse warning if present
        if let Some(ref warning) = self.parse_warning {
            let _ = writeln!(output, "Warning: {warning}\n");
        }

        // Show summary
        if !self.summary.is_empty() {
            let _ = writeln!(output, "{}\n", self.summary);
        }

        // Show issues
        if self.issues.is_empty() && self.parse_warning.is_none() {
            output.push_str("No issues found.");
        } else if !self.issues.is_empty() {
            let _ = writeln!(output, "Issues ({}):", self.issues.len());
            for issue in &self.issues {
                // Build location string (file:line format)
                let location = match (&issue.file, issue.line) {
                    (Some(file), Some(line)) => format!("{file}:{line}"),
                    (Some(file), None) => file.clone(),
                    (None, Some(line)) => format!("line {line}"),
                    (None, None) => String::new(),
                };

                // Format: [severity] location: message
                if location.is_empty() {
                    let _ = writeln!(output, "  [{}] {}", issue.severity, issue.message);
                } else {
                    let _ = writeln!(
                        output,
                        "  [{}] {}: {}",
                        issue.severity, location, issue.message
                    );
                }
            }
        }

        // Show raw response if parsing failed
        if let Some(ref raw) = self.raw_response {
            let _ = writeln!(output, "\nRaw LLM response:\n{raw}");
        }

        output
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

/// Result from the `fix` command
#[derive(Debug, Serialize)]
pub struct FixResult {
    /// Brief description of the identified issue(s)
    pub diagnosis: String,
    /// Suggested fixes
    pub fixes: Vec<Fix>,
    /// Number of issues that could not be fixed
    pub unfixable_count: usize,
    /// Warning message if the LLM response could not be parsed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_warning: Option<String>,
    /// Raw LLM response (included when parsing fails)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_response: Option<String>,
}

/// A suggested fix from the fix command
#[derive(Debug, Clone, Serialize)]
pub struct Fix {
    /// Path to the file to change
    pub file: String,
    /// Line number (if known)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    /// Original code to replace
    pub original: String,
    /// Replacement code
    pub replacement: String,
    /// Explanation of why this fixes the issue
    pub explanation: String,
}

/// Status of an individual fix application
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyStatus {
    /// Fix was successfully applied
    Applied,
    /// Fix failed to apply
    Failed,
}

/// Result of applying a single fix
#[derive(Debug, Clone, Serialize)]
pub struct AppliedFix {
    /// The original fix that was attempted
    #[serde(flatten)]
    pub fix: Fix,
    /// Whether the fix was applied successfully
    pub status: ApplyStatus,
    /// Error message if the fix failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Summary of fix application results
#[derive(Debug, Clone, Serialize)]
pub struct ApplyResults {
    /// Results for each fix attempt
    pub applied_fixes: Vec<AppliedFix>,
    /// Number of fixes successfully applied
    pub success_count: usize,
    /// Number of fixes that failed to apply
    pub failure_count: usize,
}

impl ApplyResults {
    /// Returns true if all fixes were applied successfully
    #[must_use]
    pub fn all_succeeded(&self) -> bool {
        self.failure_count == 0
    }
}

impl CommandOutput for FixResult {
    fn render_human(&self) -> String {
        use std::fmt::Write;
        let mut output = String::new();

        // Show parse warning if present
        if let Some(ref warning) = self.parse_warning {
            let _ = writeln!(output, "Warning: {warning}\n");
        }

        // Show diagnosis
        if !self.diagnosis.is_empty() {
            let _ = writeln!(output, "Diagnosis: {}\n", self.diagnosis);
        }

        if self.fixes.is_empty() && self.parse_warning.is_none() {
            output.push_str("No fixes suggested.");
        } else if !self.fixes.is_empty() {
            let _ = writeln!(output, "Suggested fixes ({}):", self.fixes.len());
            for (i, fix) in self.fixes.iter().enumerate() {
                let location = fix
                    .line
                    .map_or(fix.file.clone(), |l| format!("{}:{}", fix.file, l));
                let _ = writeln!(output, "\n{}. {} - {}", i + 1, location, fix.explanation);
                let _ = writeln!(output, "   - {}", fix.original);
                let _ = writeln!(output, "   + {}", fix.replacement);
            }
        }

        if self.unfixable_count > 0 {
            let _ = write!(
                output,
                "\n{} issues could not be fixed automatically.",
                self.unfixable_count
            );
        }

        // Show raw response if parsing failed
        if let Some(ref raw) = self.raw_response {
            let _ = writeln!(output, "\nRaw LLM response:\n{raw}");
        }

        output
    }
}

impl ExitStatus for FixResult {
    fn exit_code(&self) -> ExitCode {
        if self.unfixable_count > 0 {
            ExitCode::IssuesFound
        } else {
            ExitCode::Success
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
        let json = result.render_json().unwrap();
        assert!(json.contains("\"response\": \"Hello\""));
        assert!(json.contains("\"turns\": 2"));
    }

    #[test]
    fn test_render_json_returns_result() {
        let result = AskResult {
            response: "test".to_string(),
            turns: 1,
        };
        // render_json should return Ok for valid serializable types
        assert!(result.render_json().is_ok());
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
            parse_warning: None,
            raw_response: None,
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
                rule: None,
            }],
            passed: false,
            parse_warning: None,
            raw_response: None,
        };
        assert_eq!(result.exit_code(), ExitCode::IssuesFound);
    }

    #[test]
    fn test_review_result_render_human_no_issues() {
        let result = ReviewResult {
            summary: "Code looks great".to_string(),
            issues: vec![],
            passed: true,
            parse_warning: None,
            raw_response: None,
        };
        let output = result.render_human();
        assert!(output.contains("Code looks great"));
        assert!(output.contains("No issues found."));
    }

    #[test]
    fn test_review_result_render_human_with_file_and_line() {
        let result = ReviewResult {
            summary: "Found some issues".to_string(),
            issues: vec![ReviewIssue {
                severity: "error".to_string(),
                file: Some("src/main.rs".to_string()),
                line: Some(42),
                message: "unused variable".to_string(),
                rule: None,
            }],
            passed: false,
            parse_warning: None,
            raw_response: None,
        };
        let output = result.render_human();
        assert!(output.contains("Issues (1):"));
        assert!(output.contains("[error] src/main.rs:42: unused variable"));
    }

    #[test]
    fn test_review_result_render_human_file_only() {
        let result = ReviewResult {
            summary: String::new(),
            issues: vec![ReviewIssue {
                severity: "warning".to_string(),
                file: Some("lib.rs".to_string()),
                line: None,
                message: "missing docs".to_string(),
                rule: None,
            }],
            passed: false,
            parse_warning: None,
            raw_response: None,
        };
        let output = result.render_human();
        assert!(output.contains("[warning] lib.rs: missing docs"));
    }

    #[test]
    fn test_review_result_render_human_line_only() {
        let result = ReviewResult {
            summary: String::new(),
            issues: vec![ReviewIssue {
                severity: "info".to_string(),
                file: None,
                line: Some(100),
                message: "consider refactoring".to_string(),
                rule: None,
            }],
            passed: true,
            parse_warning: None,
            raw_response: None,
        };
        let output = result.render_human();
        assert!(output.contains("[info] line 100: consider refactoring"));
    }

    #[test]
    fn test_review_result_render_human_no_location() {
        let result = ReviewResult {
            summary: String::new(),
            issues: vec![ReviewIssue {
                severity: "error".to_string(),
                file: None,
                line: None,
                message: "global issue".to_string(),
                rule: None,
            }],
            passed: false,
            parse_warning: None,
            raw_response: None,
        };
        let output = result.render_human();
        assert!(output.contains("[error] global issue"));
    }

    #[test]
    fn test_review_result_render_human_multiple_issues() {
        let result = ReviewResult {
            summary: "Multiple problems found".to_string(),
            issues: vec![
                ReviewIssue {
                    severity: "error".to_string(),
                    file: Some("a.rs".to_string()),
                    line: Some(1),
                    message: "first issue".to_string(),
                    rule: None,
                },
                ReviewIssue {
                    severity: "warning".to_string(),
                    file: Some("b.rs".to_string()),
                    line: Some(2),
                    message: "second issue".to_string(),
                    rule: None,
                },
            ],
            passed: false,
            parse_warning: None,
            raw_response: None,
        };
        let output = result.render_human();
        assert!(output.contains("Issues (2):"));
        assert!(output.contains("[error] a.rs:1: first issue"));
        assert!(output.contains("[warning] b.rs:2: second issue"));
    }

    #[test]
    fn test_review_result_render_human_with_parse_warning() {
        let result = ReviewResult {
            summary: "Fallback summary".to_string(),
            issues: vec![],
            passed: true,
            parse_warning: Some("could not parse structured response".to_string()),
            raw_response: Some("raw llm output here".to_string()),
        };
        let output = result.render_human();
        assert!(output.contains("Warning: could not parse structured response"));
        assert!(output.contains("Raw LLM response:"));
        assert!(output.contains("raw llm output here"));
    }

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

    #[test]
    fn test_fix_result_exit_status_success() {
        let result = FixResult {
            diagnosis: "unused variable".to_string(),
            fixes: vec![Fix {
                file: "test.rs".to_string(),
                line: Some(10),
                original: "let x = 1;".to_string(),
                replacement: "let _x = 1;".to_string(),
                explanation: "prefix unused variable with underscore".to_string(),
            }],
            unfixable_count: 0,
            parse_warning: None,
            raw_response: None,
        };
        assert_eq!(result.exit_code(), ExitCode::Success);
    }

    #[test]
    fn test_fix_result_exit_status_with_remaining() {
        let result = FixResult {
            diagnosis: "multiple issues".to_string(),
            fixes: vec![],
            unfixable_count: 2,
            parse_warning: None,
            raw_response: None,
        };
        assert_eq!(result.exit_code(), ExitCode::IssuesFound);
    }

    #[test]
    fn test_fix_result_human_output_with_fixes() {
        let result = FixResult {
            diagnosis: "found linting issues".to_string(),
            fixes: vec![
                Fix {
                    file: "src/main.rs".to_string(),
                    line: Some(42),
                    original: "let x = 1;".to_string(),
                    replacement: "let _x = 1;".to_string(),
                    explanation: "prefix unused variable".to_string(),
                },
                Fix {
                    file: "src/lib.rs".to_string(),
                    line: None,
                    original: "unwrap()".to_string(),
                    replacement: "?".to_string(),
                    explanation: "use ? operator instead of unwrap".to_string(),
                },
            ],
            unfixable_count: 0,
            parse_warning: None,
            raw_response: None,
        };
        let output = result.render_human();
        assert!(output.contains("Diagnosis: found linting issues"));
        assert!(output.contains("Suggested fixes (2):"));
        assert!(output.contains("src/main.rs:42"));
        assert!(output.contains("prefix unused variable"));
        assert!(output.contains("- let x = 1;"));
        assert!(output.contains("+ let _x = 1;"));
    }

    #[test]
    fn test_fix_result_human_output_no_fixes() {
        let result = FixResult {
            diagnosis: "no issues found".to_string(),
            fixes: vec![],
            unfixable_count: 0,
            parse_warning: None,
            raw_response: None,
        };
        let output = result.render_human();
        assert!(output.contains("No fixes suggested."));
    }

    #[test]
    fn test_fix_result_human_output_with_unfixable() {
        let result = FixResult {
            diagnosis: "complex issues".to_string(),
            fixes: vec![],
            unfixable_count: 3,
            parse_warning: None,
            raw_response: None,
        };
        let output = result.render_human();
        assert!(output.contains("3 issues could not be fixed automatically."));
    }
}
