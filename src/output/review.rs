//! Output types for the `review` command

use super::{CommandOutput, ExitCode, ExitStatus};
use serde::Serialize;

/// Result from the `review` command
#[derive(Debug, Serialize)]
pub(crate) struct ReviewResult {
    /// The review summary
    pub summary: String,
    /// Identified issues
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<ReviewIssue>,
    /// Whether the review passed (no issues or only warnings)
    pub passed: bool,
    /// Number of chunks processed (only present when chunking enabled)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunks_processed: Option<usize>,
    /// Details about chunks that failed (only present when chunking enabled with --partial)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub chunk_failures: Vec<ChunkFailure>,
}

/// Information about a chunk that failed during chunked processing
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ChunkFailure {
    /// The file path that failed
    pub file_path: String,
    /// Error message describing the failure
    pub error: String,
}

/// An issue identified during code review
///
/// All fields are always present — `file` and `line` are injected by the caller,
/// `rule` is validated at parse time.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ReviewIssue {
    /// Issue severity (error, warning, info)
    pub severity: String,
    /// File path
    pub file: String,
    /// Line number
    pub line: usize,
    /// Issue description
    pub message: String,
    /// Rule name that triggered this issue
    pub rule: String,
}

impl CommandOutput for ReviewResult {
    fn render_human(&self) -> String {
        use std::fmt::Write;
        let mut output = String::new();

        // Show chunking info if applicable
        if let Some(chunks) = self.chunks_processed {
            writeln!(output, "Processed {chunks} file(s)").ok();
            if self.chunk_failures.is_empty() {
                output.push('\n');
            } else {
                let failed_count = self.chunk_failures.len();
                writeln!(output, "  ({failed_count} failed, results are partial)\n").ok();
            }
        }

        // Show summary
        if !self.summary.is_empty() {
            writeln!(output, "{}\n", self.summary).ok();
        }

        // Show issues
        if self.issues.is_empty() {
            output.push_str("No issues found.");
        } else {
            writeln!(output, "Issues ({}):", self.issues.len()).ok();
            for issue in &self.issues {
                writeln!(
                    output,
                    "  [{}] {}:{} ({}): {}",
                    issue.severity, issue.file, issue.line, issue.rule, issue.message
                )
                .ok();
            }
        }

        // Show chunk failures if any
        if !self.chunk_failures.is_empty() {
            writeln!(output, "\nChunk failures ({}):", self.chunk_failures.len()).ok();
            for failure in &self.chunk_failures {
                writeln!(output, "  {}: {}", failure.file_path, failure.error).ok();
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
mod tests {
    use super::*;

    #[test]
    fn test_review_result_exit_status_passed() {
        let result = ReviewResult {
            summary: "All good".to_string(),
            issues: vec![],
            passed: true,
            chunks_processed: None,
            chunk_failures: vec![],
        };
        assert_eq!(result.exit_code(), ExitCode::Success);
    }

    #[test]
    fn test_review_result_exit_status_failed() {
        let result = ReviewResult {
            summary: "Issues found".to_string(),
            issues: vec![ReviewIssue {
                severity: "error".to_string(),
                file: "test.rs".to_string(),
                line: 10,
                message: "bug".to_string(),
                rule: "no-bugs".to_string(),
            }],
            passed: false,
            chunks_processed: None,
            chunk_failures: vec![],
        };
        assert_eq!(result.exit_code(), ExitCode::IssuesFound);
    }

    #[test]
    fn test_review_result_render_human_no_issues() {
        let result = ReviewResult {
            summary: "Code looks great".to_string(),
            issues: vec![],
            passed: true,
            chunks_processed: None,
            chunk_failures: vec![],
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
                file: "src/main.rs".to_string(),
                line: 42,
                message: "unused variable".to_string(),
                rule: "no-unused-vars".to_string(),
            }],
            passed: false,
            chunks_processed: None,
            chunk_failures: vec![],
        };
        let output = result.render_human();
        assert!(output.contains("Issues (1):"));
        assert!(output.contains("[error] src/main.rs:42 (no-unused-vars): unused variable"));
    }

    #[test]
    fn test_review_result_render_human_multiple_issues() {
        let result = ReviewResult {
            summary: "Multiple problems found".to_string(),
            issues: vec![
                ReviewIssue {
                    severity: "error".to_string(),
                    file: "a.rs".to_string(),
                    line: 1,
                    message: "first issue".to_string(),
                    rule: "rule-a".to_string(),
                },
                ReviewIssue {
                    severity: "warning".to_string(),
                    file: "b.rs".to_string(),
                    line: 2,
                    message: "second issue".to_string(),
                    rule: "rule-b".to_string(),
                },
            ],
            passed: false,
            chunks_processed: None,
            chunk_failures: vec![],
        };
        let output = result.render_human();
        assert!(output.contains("Issues (2):"));
        assert!(output.contains("[error] a.rs:1 (rule-a): first issue"));
        assert!(output.contains("[warning] b.rs:2 (rule-b): second issue"));
    }

    #[test]
    fn test_review_result_with_chunks() {
        let result = ReviewResult {
            summary: "Reviewed 3 files".to_string(),
            issues: vec![ReviewIssue {
                severity: "warning".to_string(),
                file: "src/lib.rs".to_string(),
                line: 5,
                message: "unused import".to_string(),
                rule: "no-unused-imports".to_string(),
            }],
            passed: false,
            chunks_processed: Some(3),
            chunk_failures: vec![],
        };
        let output = result.render_human();
        assert!(output.contains("Processed 3 file(s)"));
        assert!(output.contains("Issues (1):"));
    }

    #[test]
    fn test_review_result_with_chunk_failures() {
        let result = ReviewResult {
            summary: "Partial review".to_string(),
            issues: vec![],
            passed: true,
            chunks_processed: Some(3),
            chunk_failures: vec![ChunkFailure {
                file_path: "src/broken.rs".to_string(),
                error: "LLM timeout".to_string(),
            }],
        };
        let output = result.render_human();
        assert!(output.contains("Processed 3 file(s)"));
        assert!(output.contains("1 failed, results are partial"));
        assert!(output.contains("Chunk failures (1):"));
        assert!(output.contains("src/broken.rs: LLM timeout"));
    }
}
