//! Input schema and validation for the fix command
//!
//! Defines the structured input contract and validates that
//! all required fields are present and consistent with the file system.

use std::path::Path;

use serde::Deserialize;
use thiserror::Error;

/// Structured input for the fix command (atomic mode)
///
/// Input contract:
/// - `issue`: description of the problem to fix
/// - `file`: path to the file containing the code
/// - `lines`: line range [start, end] (1-indexed, inclusive)
///
/// The snippet is always inferred from file content at the specified lines.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct FixInput {
    /// Description of the problem to fix
    pub issue: String,
    /// Path to the file containing the code
    pub file: String,
    /// Line range [start, end] (1-indexed, inclusive)
    pub lines: (usize, usize),
}

/// Errors that can occur during input validation
#[derive(Debug, Error)]
pub(crate) enum InputValidationError {
    #[error("failed to parse input JSON: {0}")]
    ParseError(#[from] serde_json::Error),

    #[error("file does not exist: {path}")]
    FileNotFound { path: String },

    #[error("cannot read file: {path}: {reason}")]
    FileUnreadable { path: String, reason: String },

    #[error("invalid line range: start ({start}) must be <= end ({end})")]
    InvalidLineRange { start: usize, end: usize },

    #[error("line range {start}..{end} exceeds file length ({file_lines} lines)")]
    LineRangeOutOfBounds {
        start: usize,
        end: usize,
        file_lines: usize,
    },

    #[error("line numbers must be 1-indexed (got start={start})")]
    ZeroLineNumber { start: usize },

    #[error("issue description is required")]
    EmptyIssue,

    #[error("issues list is empty")]
    EmptyIssues,

    #[error("overlapping line ranges detected: {range1} and {range2}")]
    OverlappingRanges { range1: String, range2: String },
}

impl FixInput {
    /// Parse input JSON from a string
    ///
    /// # Errors
    /// Returns `InputValidationError::ParseError` if JSON is malformed or missing required fields.
    pub(crate) fn parse(input: &str) -> Result<Self, InputValidationError> {
        let parsed: Self = serde_json::from_str(input)?;
        Ok(parsed)
    }

    /// Validate the input against the file system
    ///
    /// Checks:
    /// - Issue is non-empty
    /// - File exists and is readable
    /// - Line range is valid (start <= end, within file bounds, 1-indexed)
    ///
    /// The snippet is extracted from the file at the specified lines.
    ///
    /// # Errors
    /// Returns appropriate `InputValidationError` if any validation fails.
    pub(crate) fn validate(
        &self,
        working_dir: &Path,
    ) -> Result<ValidatedFixInput, InputValidationError> {
        // Check required fields are non-empty
        if self.issue.trim().is_empty() {
            return Err(InputValidationError::EmptyIssue);
        }

        // Validate line range
        let (start, end) = self.lines;
        if start == 0 {
            return Err(InputValidationError::ZeroLineNumber { start });
        }
        if start > end {
            return Err(InputValidationError::InvalidLineRange { start, end });
        }

        // Resolve file path relative to working directory
        let file_path = working_dir.join(&self.file);

        // Check file exists
        if !file_path.exists() {
            return Err(InputValidationError::FileNotFound {
                path: self.file.clone(),
            });
        }

        // Read file content
        let content = std::fs::read_to_string(&file_path).map_err(|e| {
            InputValidationError::FileUnreadable {
                path: self.file.clone(),
                reason: e.to_string(),
            }
        })?;

        let lines: Vec<&str> = content.lines().collect();

        // Check line range is within bounds
        if end > lines.len() {
            return Err(InputValidationError::LineRangeOutOfBounds {
                start,
                end,
                file_lines: lines.len(),
            });
        }

        // Extract snippet from file (1-indexed to 0-indexed)
        let snippet: String = lines
            .get((start - 1)..end)
            .map(|slice| slice.join("\n"))
            .unwrap_or_default();

        Ok(ValidatedFixInput {
            issue: self.issue.clone(),
            snippet,
            file_path,
            file_content: content,
            lines: self.lines,
        })
    }
}

/// Validated fix input with resolved file path and content
///
/// This type is returned after successful validation, guaranteeing that:
/// - The file exists and was readable
/// - The snippet matches the file content at the specified lines
/// - The line range is valid
#[derive(Debug, Clone)]
pub(crate) struct ValidatedFixInput {
    /// Description of the problem to fix
    pub issue: String,
    /// Exact code lines to transform
    pub snippet: String,
    /// Resolved absolute path to the file
    pub file_path: std::path::PathBuf,
    /// Full content of the file
    pub file_content: String,
    /// Line range [start, end] (1-indexed, inclusive)
    pub lines: (usize, usize),
}

impl ValidatedFixInput {
    /// Get the relative file path for output
    #[must_use]
    pub(crate) fn relative_path(&self, working_dir: &Path) -> String {
        self.file_path.strip_prefix(working_dir).map_or_else(
            |_| self.file_path.display().to_string(),
            |p| p.display().to_string(),
        )
    }
}

// ============================================================================
// Whole-File Mode Types
// ============================================================================

/// Issue specification for whole-file mode
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct IssueSpec {
    /// Description of the problem to fix
    pub issue: String,
    /// Line range [start, end] (1-indexed, inclusive)
    pub lines: (usize, usize),
}

/// Structured input for the fix command (whole-file mode)
///
/// Input contract:
/// - `file`: path to the file containing the code
/// - `issues`: list of issues to fix, each with description and line range
///
/// Issues are processed bottom-to-top to preserve line numbers.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct WholeFileInput {
    /// Path to the file containing the code
    pub file: String,
    /// List of issues to fix
    pub issues: Vec<IssueSpec>,
}

/// Validated issue with snippet extracted from file
#[derive(Debug, Clone)]
pub(crate) struct ValidatedIssue {
    /// Description of the problem to fix
    pub issue: String,
    /// Exact code lines to transform (extracted from file)
    pub snippet: String,
    /// Line range [start, end] (1-indexed, inclusive)
    pub lines: (usize, usize),
}

/// Validated whole-file input with resolved file path and content
#[derive(Debug, Clone)]
pub(crate) struct ValidatedWholeFileInput {
    /// Resolved absolute path to the file
    pub file_path: std::path::PathBuf,
    /// Full content of the file
    pub file_content: String,
    /// Validated issues sorted by line number descending (for bottom-to-top processing)
    pub issues: Vec<ValidatedIssue>,
}

impl ValidatedWholeFileInput {
    /// Get the relative file path for output
    #[must_use]
    pub(crate) fn relative_path(&self, working_dir: &Path) -> String {
        self.file_path.strip_prefix(working_dir).map_or_else(
            |_| self.file_path.display().to_string(),
            |p| p.display().to_string(),
        )
    }
}

impl WholeFileInput {
    /// Parse input JSON from a string
    ///
    /// # Errors
    /// Returns `InputValidationError::ParseError` if JSON is malformed or missing required fields.
    pub(crate) fn parse(input: &str) -> Result<Self, InputValidationError> {
        let parsed: Self = serde_json::from_str(input)?;
        Ok(parsed)
    }

    /// Validate the input against the file system
    ///
    /// Checks:
    /// - Issues list is non-empty
    /// - All issues have non-empty descriptions
    /// - All line ranges are valid
    /// - No overlapping line ranges
    /// - File exists and is readable
    ///
    /// Returns issues sorted by line number descending for bottom-to-top processing.
    ///
    /// # Errors
    /// Returns appropriate `InputValidationError` if any validation fails.
    pub(crate) fn validate(
        &self,
        working_dir: &Path,
    ) -> Result<ValidatedWholeFileInput, InputValidationError> {
        // Check issues list is non-empty
        if self.issues.is_empty() {
            return Err(InputValidationError::EmptyIssues);
        }

        // Validate each issue
        for issue in &self.issues {
            if issue.issue.trim().is_empty() {
                return Err(InputValidationError::EmptyIssue);
            }

            let (start, end) = issue.lines;
            if start == 0 {
                return Err(InputValidationError::ZeroLineNumber { start });
            }
            if start > end {
                return Err(InputValidationError::InvalidLineRange { start, end });
            }
        }

        // Check for overlapping ranges
        for (i, issue_a) in self.issues.iter().enumerate() {
            for issue_b in self.issues.iter().skip(i + 1) {
                if ranges_overlap(issue_a.lines, issue_b.lines) {
                    return Err(InputValidationError::OverlappingRanges {
                        range1: format!("{}..{}", issue_a.lines.0, issue_a.lines.1),
                        range2: format!("{}..{}", issue_b.lines.0, issue_b.lines.1),
                    });
                }
            }
        }

        // Resolve file path relative to working directory
        let file_path = working_dir.join(&self.file);

        // Check file exists
        if !file_path.exists() {
            return Err(InputValidationError::FileNotFound {
                path: self.file.clone(),
            });
        }

        // Read file content
        let content = std::fs::read_to_string(&file_path).map_err(|e| {
            InputValidationError::FileUnreadable {
                path: self.file.clone(),
                reason: e.to_string(),
            }
        })?;

        let lines: Vec<&str> = content.lines().collect();

        // Validate line ranges and extract snippets
        let mut validated_issues = Vec::with_capacity(self.issues.len());
        for issue in &self.issues {
            let (start, end) = issue.lines;

            // Check line range is within bounds
            if end > lines.len() {
                return Err(InputValidationError::LineRangeOutOfBounds {
                    start,
                    end,
                    file_lines: lines.len(),
                });
            }

            // Extract snippet from file (1-indexed to 0-indexed)
            let snippet: String = lines
                .get((start - 1)..end)
                .map(|slice| slice.join("\n"))
                .unwrap_or_default();

            validated_issues.push(ValidatedIssue {
                issue: issue.issue.clone(),
                snippet,
                lines: issue.lines,
            });
        }

        // Sort by line number descending for bottom-to-top processing
        validated_issues.sort_by(|a, b| b.lines.0.cmp(&a.lines.0));

        Ok(ValidatedWholeFileInput {
            file_path,
            file_content: content,
            issues: validated_issues,
        })
    }
}

/// Check if two line ranges overlap
///
/// Ranges are inclusive: (start, end) means lines start through end.
#[must_use]
pub(crate) fn ranges_overlap(a: (usize, usize), b: (usize, usize)) -> bool {
    // Two ranges [a_start, a_end] and [b_start, b_end] overlap if:
    // a_start <= b_end AND b_start <= a_end
    a.0 <= b.1 && b.0 <= a.1
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code uses unwrap for clarity")]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn setup_test_file(content: &str) -> (TempDir, String) {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.rs");
        let mut file = std::fs::File::create(&file_path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        (dir, "test.rs".to_string())
    }

    #[test]
    fn test_parse_valid_input() {
        let json = r#"{
            "issue": "unused variable",
            "file": "src/main.rs",
            "lines": [10, 10]
        }"#;
        let input = FixInput::parse(json).unwrap();
        assert_eq!(input.issue, "unused variable");
        assert_eq!(input.file, "src/main.rs");
        assert_eq!(input.lines, (10, 10));
    }

    #[test]
    fn test_parse_invalid_json() {
        let json = "not valid json";
        assert!(FixInput::parse(json).is_err());
    }

    #[test]
    fn test_parse_missing_field() {
        // Missing 'lines' field
        let json = r#"{"issue": "test", "file": "test.rs"}"#;
        assert!(FixInput::parse(json).is_err());
    }

    #[test]
    fn test_validate_success() {
        let content = "fn main() {\n    let x = 1;\n    println!(\"{}\", x);\n}";
        let (dir, file_name) = setup_test_file(content);

        let input = FixInput {
            issue: "unused variable".to_string(),
            file: file_name,
            lines: (2, 2),
        };

        let validated = input.validate(dir.path()).unwrap();
        assert_eq!(validated.issue, "unused variable");
        assert_eq!(validated.lines, (2, 2));
        assert_eq!(validated.snippet, "    let x = 1;");
    }

    #[test]
    fn test_validate_multiline_snippet() {
        let content =
            "fn main() {\n    let x = 1;\n    let y = 2;\n    println!(\"{}\", x + y);\n}";
        let (dir, file_name) = setup_test_file(content);

        let input = FixInput {
            issue: "combine declarations".to_string(),
            file: file_name,
            lines: (2, 3),
        };

        let validated = input.validate(dir.path()).unwrap();
        assert_eq!(validated.lines, (2, 3));
        assert_eq!(validated.snippet, "    let x = 1;\n    let y = 2;");
    }

    #[test]
    fn test_validate_file_not_found() {
        let dir = TempDir::new().unwrap();
        let input = FixInput {
            issue: "test".to_string(),
            file: "nonexistent.rs".to_string(),
            lines: (1, 1),
        };

        let result = input.validate(dir.path());
        assert!(matches!(
            result,
            Err(InputValidationError::FileNotFound { .. })
        ));
    }

    #[test]
    fn test_validate_invalid_line_range() {
        let (dir, file_name) = setup_test_file("line 1\nline 2");

        let input = FixInput {
            issue: "test".to_string(),
            file: file_name,
            lines: (5, 3), // start > end
        };

        let result = input.validate(dir.path());
        assert!(matches!(
            result,
            Err(InputValidationError::InvalidLineRange { .. })
        ));
    }

    #[test]
    fn test_validate_zero_line_number() {
        let (dir, file_name) = setup_test_file("line 1");

        let input = FixInput {
            issue: "test".to_string(),
            file: file_name,
            lines: (0, 1),
        };

        let result = input.validate(dir.path());
        assert!(matches!(
            result,
            Err(InputValidationError::ZeroLineNumber { .. })
        ));
    }

    #[test]
    fn test_validate_line_range_out_of_bounds() {
        let (dir, file_name) = setup_test_file("line 1\nline 2");

        let input = FixInput {
            issue: "test".to_string(),
            file: file_name,
            lines: (1, 10),
        };

        let result = input.validate(dir.path());
        assert!(matches!(
            result,
            Err(InputValidationError::LineRangeOutOfBounds { .. })
        ));
    }

    #[test]
    fn test_validate_empty_issue() {
        let (dir, file_name) = setup_test_file("content");

        let input = FixInput {
            issue: "   ".to_string(),
            file: file_name,
            lines: (1, 1),
        };

        let result = input.validate(dir.path());
        assert!(matches!(result, Err(InputValidationError::EmptyIssue)));
    }

    #[test]
    fn test_snippet_inferred_from_file() {
        let content = "line 1\nline 2\nline 3";
        let (dir, file_name) = setup_test_file(content);

        let input = FixInput {
            issue: "test".to_string(),
            file: file_name,
            lines: (1, 2),
        };

        let validated = input.validate(dir.path()).unwrap();
        assert_eq!(validated.snippet, "line 1\nline 2");
    }

    // ========================================================================
    // Whole-File Mode Tests
    // ========================================================================

    #[test]
    fn test_parse_whole_file_input() {
        let json = r#"{
            "file": "src/main.rs",
            "issues": [
                {"issue": "unused var", "lines": [10, 10]},
                {"issue": "missing error handling", "lines": [25, 28]}
            ]
        }"#;
        let input = WholeFileInput::parse(json).unwrap();
        assert_eq!(input.file, "src/main.rs");
        assert_eq!(input.issues.len(), 2);
        assert_eq!(
            input.issues.get(0).map(|i| &i.issue),
            Some(&"unused var".to_string())
        );
        assert_eq!(input.issues.get(0).map(|i| i.lines), Some((10, 10)));
        assert_eq!(
            input.issues.get(1).map(|i| &i.issue),
            Some(&"missing error handling".to_string())
        );
        assert_eq!(input.issues.get(1).map(|i| i.lines), Some((25, 28)));
    }

    #[test]
    fn test_validate_whole_file_success() {
        let content = "line 1\nline 2\nline 3\nline 4\nline 5";
        let (dir, file_name) = setup_test_file(content);

        let input = WholeFileInput {
            file: file_name,
            issues: vec![
                IssueSpec {
                    issue: "fix line 1".to_string(),
                    lines: (1, 1),
                },
                IssueSpec {
                    issue: "fix lines 4-5".to_string(),
                    lines: (4, 5),
                },
            ],
        };

        let validated = input.validate(dir.path()).unwrap();
        assert_eq!(validated.issues.len(), 2);
        // Should be sorted descending by line number
        assert_eq!(validated.issues.get(0).map(|i| i.lines), Some((4, 5)));
        assert_eq!(validated.issues.get(1).map(|i| i.lines), Some((1, 1)));
        // Snippets should be extracted
        assert_eq!(
            validated.issues.get(0).map(|i| &i.snippet),
            Some(&"line 4\nline 5".to_string())
        );
        assert_eq!(
            validated.issues.get(1).map(|i| &i.snippet),
            Some(&"line 1".to_string())
        );
    }

    #[test]
    fn test_validate_whole_file_empty_issues() {
        let (dir, file_name) = setup_test_file("content");

        let input = WholeFileInput {
            file: file_name,
            issues: vec![],
        };

        let result = input.validate(dir.path());
        assert!(matches!(result, Err(InputValidationError::EmptyIssues)));
    }

    #[test]
    fn test_validate_whole_file_overlapping_ranges() {
        let (dir, file_name) = setup_test_file("line 1\nline 2\nline 3\nline 4");

        let input = WholeFileInput {
            file: file_name,
            issues: vec![
                IssueSpec {
                    issue: "issue 1".to_string(),
                    lines: (1, 3),
                },
                IssueSpec {
                    issue: "issue 2".to_string(),
                    lines: (2, 4),
                },
            ],
        };

        let result = input.validate(dir.path());
        assert!(matches!(
            result,
            Err(InputValidationError::OverlappingRanges { .. })
        ));
    }

    #[test]
    fn test_validate_whole_file_adjacent_ranges_ok() {
        let content = "line 1\nline 2\nline 3\nline 4";
        let (dir, file_name) = setup_test_file(content);

        let input = WholeFileInput {
            file: file_name,
            issues: vec![
                IssueSpec {
                    issue: "issue 1".to_string(),
                    lines: (1, 2),
                },
                IssueSpec {
                    issue: "issue 2".to_string(),
                    lines: (3, 4),
                },
            ],
        };

        // Adjacent ranges should not overlap
        let result = input.validate(dir.path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_ranges_overlap_basic() {
        // Overlapping
        assert!(ranges_overlap((1, 3), (2, 4)));
        assert!(ranges_overlap((2, 4), (1, 3)));
        assert!(ranges_overlap((1, 5), (2, 3))); // contained
        assert!(ranges_overlap((2, 3), (1, 5))); // container

        // Same range
        assert!(ranges_overlap((1, 3), (1, 3)));

        // Non-overlapping
        assert!(!ranges_overlap((1, 2), (3, 4)));
        assert!(!ranges_overlap((3, 4), (1, 2)));
        assert!(!ranges_overlap((1, 2), (4, 5)));
    }

    #[test]
    fn test_ranges_overlap_single_line() {
        // Single line ranges
        assert!(ranges_overlap((5, 5), (5, 5)));
        assert!(!ranges_overlap((5, 5), (6, 6)));
        assert!(ranges_overlap((5, 5), (4, 6)));
    }
}
