//! Input schema and validation for the fix command
//!
//! Defines the structured input contract and validates that
//! all required fields are present and consistent with the file system.

use std::path::Path;

use serde::Deserialize;
use thiserror::Error;

/// Structured input for the fix command
///
/// Input contract:
/// - `issue`: description of the problem to fix
/// - `snippet`: exact code lines to fix
/// - `file`: path to the file containing the code
/// - `lines`: line range [start, end] (1-indexed, inclusive)
#[derive(Debug, Clone, Deserialize)]
pub struct FixInput {
    /// Description of the problem to fix
    pub issue: String,
    /// Exact code lines to transform
    pub snippet: String,
    /// Path to the file containing the code
    pub file: String,
    /// Line range [start, end] (1-indexed, inclusive)
    pub lines: (usize, usize),
}

/// Errors that can occur during input validation
#[derive(Debug, Error)]
pub enum InputValidationError {
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

    #[error("snippet does not match file content at lines {start}..{end}")]
    SnippetMismatch { start: usize, end: usize },

    #[error("issue description is required")]
    EmptyIssue,

    #[error("snippet is required")]
    EmptySnippet,
}

impl FixInput {
    /// Parse input JSON from a string
    ///
    /// # Errors
    /// Returns `InputValidationError::ParseError` if JSON is malformed or missing required fields.
    pub fn parse(input: &str) -> Result<Self, InputValidationError> {
        let parsed: Self = serde_json::from_str(input)?;
        Ok(parsed)
    }

    /// Validate the input against the file system
    ///
    /// Checks:
    /// - Issue and snippet are non-empty
    /// - File exists and is readable
    /// - Line range is valid (start <= end, within file bounds)
    /// - Snippet matches file content at specified lines
    ///
    /// # Errors
    /// Returns appropriate `InputValidationError` if any validation fails.
    pub fn validate(&self, working_dir: &Path) -> Result<ValidatedFixInput, InputValidationError> {
        // Check required fields are non-empty
        if self.issue.trim().is_empty() {
            return Err(InputValidationError::EmptyIssue);
        }
        if self.snippet.is_empty() {
            return Err(InputValidationError::EmptySnippet);
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

        // Extract lines from file (1-indexed to 0-indexed)
        let file_snippet: String = lines[(start - 1)..end].join("\n");

        // Normalize snippets for comparison (trim trailing whitespace from each line)
        let normalize =
            |s: &str| -> String { s.lines().map(str::trim_end).collect::<Vec<_>>().join("\n") };

        let normalized_input = normalize(&self.snippet);
        let normalized_file = normalize(&file_snippet);

        if normalized_input != normalized_file {
            return Err(InputValidationError::SnippetMismatch { start, end });
        }

        Ok(ValidatedFixInput {
            issue: self.issue.clone(),
            snippet: self.snippet.clone(),
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
pub struct ValidatedFixInput {
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
    pub fn relative_path(&self, working_dir: &Path) -> String {
        self.file_path.strip_prefix(working_dir).map_or_else(
            |_| self.file_path.display().to_string(),
            |p| p.display().to_string(),
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
            "snippet": "let x = 1;",
            "file": "src/main.rs",
            "lines": [10, 10]
        }"#;
        let input = FixInput::parse(json).unwrap();
        assert_eq!(input.issue, "unused variable");
        assert_eq!(input.snippet, "let x = 1;");
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
        let json = r#"{"issue": "test", "snippet": "code"}"#;
        assert!(FixInput::parse(json).is_err());
    }

    #[test]
    fn test_validate_success() {
        let content = "fn main() {\n    let x = 1;\n    println!(\"{}\", x);\n}";
        let (dir, file_name) = setup_test_file(content);

        let input = FixInput {
            issue: "unused variable".to_string(),
            snippet: "    let x = 1;".to_string(),
            file: file_name,
            lines: (2, 2),
        };

        let validated = input.validate(dir.path()).unwrap();
        assert_eq!(validated.issue, "unused variable");
        assert_eq!(validated.lines, (2, 2));
    }

    #[test]
    fn test_validate_multiline_snippet() {
        let content =
            "fn main() {\n    let x = 1;\n    let y = 2;\n    println!(\"{}\", x + y);\n}";
        let (dir, file_name) = setup_test_file(content);

        let input = FixInput {
            issue: "combine declarations".to_string(),
            snippet: "    let x = 1;\n    let y = 2;".to_string(),
            file: file_name,
            lines: (2, 3),
        };

        let validated = input.validate(dir.path()).unwrap();
        assert_eq!(validated.lines, (2, 3));
    }

    #[test]
    fn test_validate_file_not_found() {
        let dir = TempDir::new().unwrap();
        let input = FixInput {
            issue: "test".to_string(),
            snippet: "code".to_string(),
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
            snippet: "code".to_string(),
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
            snippet: "line 1".to_string(),
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
            snippet: "line 1".to_string(),
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
    fn test_validate_snippet_mismatch() {
        let (dir, file_name) = setup_test_file("actual content");

        let input = FixInput {
            issue: "test".to_string(),
            snippet: "wrong content".to_string(),
            file: file_name,
            lines: (1, 1),
        };

        let result = input.validate(dir.path());
        assert!(matches!(
            result,
            Err(InputValidationError::SnippetMismatch { .. })
        ));
    }

    #[test]
    fn test_validate_empty_issue() {
        let (dir, file_name) = setup_test_file("content");

        let input = FixInput {
            issue: "   ".to_string(),
            snippet: "content".to_string(),
            file: file_name,
            lines: (1, 1),
        };

        let result = input.validate(dir.path());
        assert!(matches!(result, Err(InputValidationError::EmptyIssue)));
    }

    #[test]
    fn test_validate_empty_snippet() {
        let (dir, file_name) = setup_test_file("content");

        let input = FixInput {
            issue: "test".to_string(),
            snippet: String::new(),
            file: file_name,
            lines: (1, 1),
        };

        let result = input.validate(dir.path());
        assert!(matches!(result, Err(InputValidationError::EmptySnippet)));
    }

    #[test]
    fn test_validate_trailing_whitespace_normalization() {
        // File has trailing whitespace, input does not
        let content = "line 1  \nline 2\t";
        let (dir, file_name) = setup_test_file(content);

        let input = FixInput {
            issue: "test".to_string(),
            snippet: "line 1\nline 2".to_string(),
            file: file_name,
            lines: (1, 2),
        };

        // Should succeed because trailing whitespace is normalized
        assert!(input.validate(dir.path()).is_ok());
    }
}
