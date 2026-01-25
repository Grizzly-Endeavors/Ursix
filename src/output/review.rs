//! Output types for the `review` command

use super::{CommandOutput, ExitCode, ExitStatus};
use serde::Serialize;

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
    /// Rule name that triggered this issue (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
}

impl CommandOutput for ReviewResult {
    fn render_human(&self) -> String {
        use std::fmt::Write;
        let mut output = String::new();

        // Show summary
        if !self.summary.is_empty() {
            let _ = writeln!(output, "{}\n", self.summary);
        }

        // Show issues
        if self.issues.is_empty() {
            output.push_str("No issues found.");
        } else {
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

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
                rule: None,
            }],
            passed: false,
        };
        assert_eq!(result.exit_code(), ExitCode::IssuesFound);
    }

    #[test]
    fn test_review_result_render_human_no_issues() {
        let result = ReviewResult {
            summary: "Code looks great".to_string(),
            issues: vec![],
            passed: true,
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
        };
        let output = result.render_human();
        assert!(output.contains("Issues (2):"));
        assert!(output.contains("[error] a.rs:1: first issue"));
        assert!(output.contains("[warning] b.rs:2: second issue"));
    }
}
