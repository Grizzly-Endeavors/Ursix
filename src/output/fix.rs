//! Output types for the `fix` command

use super::{CommandOutput, ExitCode, ExitStatus};
use serde::Serialize;

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
