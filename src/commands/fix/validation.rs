//! Output validation for the fix command
//!
//! Validates LLM output before generating the diff to catch
//! obvious problems early.

use thiserror::Error;

/// Maximum allowed expansion ratio for replacement code
const MAX_EXPANSION_RATIO: f64 = 10.0;

/// Validation errors for LLM output
#[derive(Debug, Error)]
pub enum ValidationError {
    #[error(
        "replacement is {ratio:.1}x longer than original (max {max}x); likely includes explanation or metadata"
    )]
    LengthExceeded { ratio: f64, max: f64 },

    #[error("unbalanced delimiters: {details}")]
    UnbalancedDelimiters { details: String },
}

/// Warning for potential issues that don't block the fix
#[derive(Debug, Clone)]
pub struct ValidationWarning {
    pub message: String,
}

/// Result of validating LLM output
#[derive(Debug)]
pub struct ValidationResult {
    /// Warnings that don't block the fix
    pub warnings: Vec<ValidationWarning>,
}

impl ValidationResult {
    fn new() -> Self {
        Self {
            warnings: Vec::new(),
        }
    }

    fn add_warning(&mut self, message: impl Into<String>) {
        self.warnings.push(ValidationWarning {
            message: message.into(),
        });
    }
}

/// Validate LLM output before generating diff
///
/// # Checks
/// 1. Length sanity: Reject if replacement is >10x original length
/// 2. Balanced delimiters: Warn on obvious mismatches
///
/// # Errors
/// Returns `ValidationError` if replacement fails hard validation checks.
pub fn validate_replacement(
    original: &str,
    replacement: &str,
) -> Result<ValidationResult, ValidationError> {
    let mut result = ValidationResult::new();

    // Length sanity check
    let original_len = original.len();
    let replacement_len = replacement.len();

    if original_len > 0 {
        // Precision loss is acceptable for ratio calculation
        #[allow(clippy::cast_precision_loss)]
        let ratio = replacement_len as f64 / original_len as f64;
        if ratio > MAX_EXPANSION_RATIO {
            return Err(ValidationError::LengthExceeded {
                ratio,
                max: MAX_EXPANSION_RATIO,
            });
        }
    } else if replacement_len > 500 {
        // If original was empty, reject if replacement is unreasonably large
        return Err(ValidationError::LengthExceeded {
            ratio: f64::INFINITY,
            max: MAX_EXPANSION_RATIO,
        });
    }

    // Balanced delimiter check
    if let Some(details) = check_delimiter_balance(replacement) {
        // This is a warning, not an error
        result.add_warning(format!("possible unbalanced delimiters: {details}"));
    }

    // Check for common LLM mistakes
    check_common_mistakes(replacement, &mut result);

    Ok(result)
}

/// Check for balanced delimiters in code
///
/// Returns `Some(details)` if imbalance detected, `None` if balanced.
fn check_delimiter_balance(code: &str) -> Option<String> {
    let mut stack: Vec<char> = Vec::new();
    let mut in_string = false;
    let mut string_char = '"';
    let mut prev_char = ' ';

    for ch in code.chars() {
        // Track string literals to avoid counting delimiters inside strings
        if (ch == '"' || ch == '\'') && prev_char != '\\' {
            if in_string && ch == string_char {
                in_string = false;
            } else if !in_string {
                in_string = true;
                string_char = ch;
            }
        }

        if !in_string {
            match ch {
                '(' | '[' | '{' => stack.push(ch),
                ')' => {
                    if stack.last() == Some(&'(') {
                        stack.pop();
                    } else {
                        return Some("unexpected ')'".to_string());
                    }
                }
                ']' => {
                    if stack.last() == Some(&'[') {
                        stack.pop();
                    } else {
                        return Some("unexpected ']'".to_string());
                    }
                }
                '}' => {
                    if stack.last() == Some(&'{') {
                        stack.pop();
                    } else {
                        return Some("unexpected '}'".to_string());
                    }
                }
                _ => {}
            }
        }
        prev_char = ch;
    }

    if !stack.is_empty() {
        let unclosed: String = stack.iter().collect();
        return Some(format!("unclosed delimiters: {unclosed}"));
    }

    None
}

/// Check for common LLM mistakes in output
fn check_common_mistakes(code: &str, result: &mut ValidationResult) {
    // Check for markdown code fences
    if code.starts_with("```") || code.contains("\n```") {
        result.add_warning("output contains markdown code fences");
    }

    // Check for common explanation markers
    let explanation_markers = [
        "Here's the fixed",
        "Here is the fixed",
        "I've fixed",
        "I have fixed",
        "The fix is",
        "// Fixed:",
        "// Fix:",
        "/* Fixed",
    ];

    for marker in explanation_markers {
        if code.contains(marker) {
            result.add_warning(format!("output may contain explanation text: '{marker}'"));
            break;
        }
    }
}

/// Strip common LLM formatting issues from output
///
/// Attempts to clean up the output by removing:
/// - Leading/trailing blank lines
/// - Leading/trailing markdown code fences
///
/// IMPORTANT: Preserves leading whitespace (indentation) on code lines.
/// Only blank lines and fence lines are removed, not indentation.
#[must_use]
pub fn sanitize_replacement(raw: &str) -> String {
    let mut lines: Vec<&str> = raw.lines().collect();

    // Remove blank lines from the start
    while lines.first().is_some_and(|l| l.trim().is_empty()) {
        lines.remove(0);
    }

    // Remove blank lines from the end
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }

    // Handle markdown code fence at start (e.g., "```rust" or "```")
    if lines
        .first()
        .is_some_and(|l| l.trim_start().starts_with("```"))
    {
        lines.remove(0);
    }

    // Handle markdown code fence at end
    if lines.last().is_some_and(|l| l.trim() == "```") {
        lines.pop();
    }

    // Remove any blank lines that were adjacent to the fences
    while lines.first().is_some_and(|l| l.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }

    lines.join("\n")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_replacement_success() {
        let original = "let x = 1;";
        let replacement = "let _x = 1;";
        let result = validate_replacement(original, replacement).unwrap();
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_validate_replacement_length_exceeded() {
        let original = "x";
        let replacement = "x".repeat(100);
        let result = validate_replacement(original, &replacement);
        assert!(matches!(
            result,
            Err(ValidationError::LengthExceeded { .. })
        ));
    }

    #[test]
    fn test_validate_replacement_moderate_expansion() {
        let original = "let x = 1;";
        // 5x expansion should be fine
        let replacement = "let x = 1;\nlet y = 2;\nlet z = 3;\nlet w = 4;\nlet v = 5;";
        let result = validate_replacement(original, replacement).unwrap();
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_validate_replacement_with_code_fence() {
        let original = "let x = 1;";
        let replacement = "```rust\nlet _x = 1;\n```";
        let result = validate_replacement(original, replacement).unwrap();
        assert!(!result.warnings.is_empty());
        assert!(result.warnings[0].message.contains("code fences"));
    }

    #[test]
    fn test_validate_replacement_with_explanation() {
        let original = "let x = 1;";
        let replacement = "Here's the fixed code:\nlet _x = 1;";
        let result = validate_replacement(original, replacement).unwrap();
        assert!(!result.warnings.is_empty());
        assert!(result.warnings[0].message.contains("explanation"));
    }

    #[test]
    fn test_check_delimiter_balance_balanced() {
        assert!(check_delimiter_balance("fn foo() { bar([1, 2]) }").is_none());
        assert!(check_delimiter_balance("let x = \"}\";").is_none());
        assert!(check_delimiter_balance("").is_none());
    }

    #[test]
    fn test_check_delimiter_balance_unclosed_brace() {
        let result = check_delimiter_balance("fn foo() {");
        assert!(result.is_some());
        assert!(result.as_ref().is_some_and(|s| s.contains("unclosed")));
    }

    #[test]
    fn test_check_delimiter_balance_unexpected_close() {
        let result = check_delimiter_balance("fn foo() }");
        assert!(result.is_some());
        assert!(result.as_ref().is_some_and(|s| s.contains("unexpected")));
    }

    #[test]
    fn test_check_delimiter_balance_in_string() {
        // Delimiters inside strings should be ignored
        assert!(check_delimiter_balance(r#"let s = "({[";"#).is_none());
        assert!(check_delimiter_balance(r"let s = '(';").is_none());
    }

    #[test]
    fn test_sanitize_replacement_clean() {
        let input = "let x = 1;";
        assert_eq!(sanitize_replacement(input), "let x = 1;");
    }

    #[test]
    fn test_sanitize_replacement_with_fences() {
        let input = "```rust\nlet x = 1;\n```";
        assert_eq!(sanitize_replacement(input), "let x = 1;");
    }

    #[test]
    fn test_sanitize_replacement_preserves_indentation() {
        // Leading whitespace (indentation) should be preserved
        let input = "    let x = 1;";
        assert_eq!(sanitize_replacement(input), "    let x = 1;");
    }

    #[test]
    fn test_sanitize_replacement_removes_blank_lines() {
        // Blank lines at start/end should be removed, but indentation preserved
        let input = "\n\n    let x = 1;\n\n";
        assert_eq!(sanitize_replacement(input), "    let x = 1;");
    }

    #[test]
    fn test_sanitize_replacement_fence_no_language() {
        let input = "```\nlet x = 1;\n```";
        assert_eq!(sanitize_replacement(input), "let x = 1;");
    }

    #[test]
    fn test_sanitize_replacement_indented_with_fences() {
        // Indentation inside fences should be preserved
        let input = "```rust\n    let x = 1;\n```";
        assert_eq!(sanitize_replacement(input), "    let x = 1;");
    }

    #[test]
    fn test_sanitize_replacement_multiline_indented() {
        let input = "    fn foo() {\n        bar();\n    }";
        assert_eq!(
            sanitize_replacement(input),
            "    fn foo() {\n        bar();\n    }"
        );
    }

    #[test]
    fn test_validate_empty_original_small_replacement() {
        let original = "";
        let replacement = "x";
        let result = validate_replacement(original, replacement).unwrap();
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_validate_empty_original_large_replacement() {
        let original = "";
        let replacement = "x".repeat(1000);
        let result = validate_replacement(original, &replacement);
        assert!(matches!(
            result,
            Err(ValidationError::LengthExceeded { .. })
        ));
    }
}
