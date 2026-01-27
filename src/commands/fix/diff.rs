//! Unified diff generation for the fix command
//!
//! Uses the `similar` crate to generate clean unified diffs
//! programmatically, avoiding LLM formatting issues.

use similar::{ChangeTag, TextDiff};

/// Result of applying a replacement and generating a diff
#[derive(Debug, Clone)]
pub struct DiffResult {
    /// The unified diff string
    pub diff: String,
    /// The modified file content
    pub modified_content: String,
    /// Number of lines added
    pub lines_added: usize,
    /// Number of lines removed
    pub lines_removed: usize,
}

/// Apply a replacement to file content at the specified line range
///
/// # Arguments
/// - `file_content`: The original file content
/// - `replacement`: The replacement code
/// - `line_range`: (start, end) 1-indexed inclusive line range
///
/// # Returns
/// The modified file content with the replacement applied
#[must_use]
pub fn apply_replacement(
    file_content: &str,
    replacement: &str,
    line_range: (usize, usize),
) -> String {
    let (start, end) = line_range;
    let lines: Vec<&str> = file_content.lines().collect();

    let mut result = String::new();

    // Add lines before the replacement (1-indexed, so start-1 is exclusive upper bound)
    for line in lines.iter().take(start - 1) {
        result.push_str(line);
        result.push('\n');
    }

    // Add replacement
    result.push_str(replacement);
    if !replacement.ends_with('\n') && !replacement.is_empty() {
        result.push('\n');
    }

    // Add lines after the replacement
    for line in lines.iter().skip(end) {
        result.push_str(line);
        result.push('\n');
    }

    // Preserve original trailing newline behavior
    if !file_content.ends_with('\n') && result.ends_with('\n') {
        result.pop();
    }

    result
}

/// Generate a unified diff between original and modified content
///
/// # Arguments
/// - `file_path`: Path to display in diff header
/// - `original`: Original file content
/// - `modified`: Modified file content
/// - `context_lines`: Number of context lines around changes
#[must_use]
pub fn generate_unified_diff(
    file_path: &str,
    original: &str,
    modified: &str,
    context_lines: usize,
) -> DiffResult {
    let diff = TextDiff::from_lines(original, modified);

    let mut lines_added = 0;
    let mut lines_removed = 0;

    // Count changes
    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Insert => lines_added += 1,
            ChangeTag::Delete => lines_removed += 1,
            ChangeTag::Equal => {}
        }
    }

    // Generate unified diff with headers
    let unified = diff
        .unified_diff()
        .context_radius(context_lines)
        .header(&format!("a/{file_path}"), &format!("b/{file_path}"))
        .to_string();

    DiffResult {
        diff: unified,
        modified_content: modified.to_string(),
        lines_added,
        lines_removed,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_replacement_single_line() {
        let content = "line 1\nline 2\nline 3";
        let replacement = "modified line 2";
        let result = apply_replacement(content, replacement, (2, 2));
        assert_eq!(result, "line 1\nmodified line 2\nline 3");
    }

    #[test]
    fn test_apply_replacement_multiline() {
        let content = "line 1\nline 2\nline 3\nline 4";
        let replacement = "new line A\nnew line B";
        let result = apply_replacement(content, replacement, (2, 3));
        assert_eq!(result, "line 1\nnew line A\nnew line B\nline 4");
    }

    #[test]
    fn test_apply_replacement_first_line() {
        let content = "line 1\nline 2";
        let replacement = "new first";
        let result = apply_replacement(content, replacement, (1, 1));
        assert_eq!(result, "new first\nline 2");
    }

    #[test]
    fn test_apply_replacement_last_line() {
        let content = "line 1\nline 2";
        let replacement = "new last";
        let result = apply_replacement(content, replacement, (2, 2));
        assert_eq!(result, "line 1\nnew last");
    }

    #[test]
    fn test_apply_replacement_with_trailing_newline() {
        let content = "line 1\nline 2\n";
        let replacement = "modified";
        let result = apply_replacement(content, replacement, (2, 2));
        assert_eq!(result, "line 1\nmodified\n");
    }

    #[test]
    fn test_apply_replacement_entire_file() {
        let content = "line 1\nline 2";
        let replacement = "completely new";
        let result = apply_replacement(content, replacement, (1, 2));
        assert_eq!(result, "completely new");
    }

    #[test]
    fn test_apply_replacement_empty_replacement() {
        let content = "line 1\nline 2\nline 3";
        let replacement = "";
        let result = apply_replacement(content, replacement, (2, 2));
        assert_eq!(result, "line 1\nline 3");
    }

    #[test]
    fn test_generate_unified_diff_basic() {
        let original = "line 1\nline 2\nline 3";
        let modified = "line 1\nmodified line 2\nline 3";
        let result = generate_unified_diff("test.rs", original, modified, 3);

        assert!(result.diff.contains("--- a/test.rs"));
        assert!(result.diff.contains("+++ b/test.rs"));
        assert!(result.diff.contains("-line 2"));
        assert!(result.diff.contains("+modified line 2"));
        assert_eq!(result.lines_added, 1);
        assert_eq!(result.lines_removed, 1);
    }

    #[test]
    fn test_generate_unified_diff_multiline_change() {
        let original = "fn main() {\n    let x = 1;\n    let y = 2;\n}";
        let modified = "fn main() {\n    let (x, y) = (1, 2);\n}";
        let result = generate_unified_diff("main.rs", original, modified, 3);

        assert!(result.diff.contains("--- a/main.rs"));
        assert!(result.diff.contains("+++ b/main.rs"));
        assert_eq!(result.lines_removed, 2);
        assert_eq!(result.lines_added, 1);
    }

    #[test]
    fn test_generate_unified_diff_no_changes() {
        let content = "unchanged content";
        let result = generate_unified_diff("file.txt", content, content, 3);

        // No changes means empty diff
        assert!(result.diff.is_empty() || !result.diff.contains("@@"));
        assert_eq!(result.lines_added, 0);
        assert_eq!(result.lines_removed, 0);
    }

    #[test]
    fn test_generate_unified_diff_context_lines() {
        let original = "1\n2\n3\n4\n5\n6\n7\n8\n9\n10";
        let modified = "1\n2\n3\n4\nMODIFIED\n6\n7\n8\n9\n10";
        let result = generate_unified_diff("file.txt", original, modified, 2);

        // With context of 2, should show lines 3,4 and 6,7
        assert!(result.diff.contains(" 3"));
        assert!(result.diff.contains(" 4"));
        assert!(result.diff.contains("-5"));
        assert!(result.diff.contains("+MODIFIED"));
        assert!(result.diff.contains(" 6"));
        assert!(result.diff.contains(" 7"));
    }

    #[test]
    fn test_integration_apply_and_diff() {
        let original = "fn example() {\n    let x = 1;\n    x\n}";
        let replacement = "    let _x = 1;";
        let modified = apply_replacement(original, replacement, (2, 2));
        let diff_result = generate_unified_diff("example.rs", original, &modified, 3);

        assert!(diff_result.diff.contains("-    let x = 1;"));
        assert!(diff_result.diff.contains("+    let _x = 1;"));
        assert_eq!(diff_result.lines_added, 1);
        assert_eq!(diff_result.lines_removed, 1);
    }
}
