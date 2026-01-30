//! Input source handling for the CLI
//!
//! All commands read from stdin or `--from FILE`. No magic stdin detection.

use std::io::{self, Read};
use std::path::PathBuf;

use anyhow::{Context, Result};

/// Read content from stdin (blocking)
fn read_stdin() -> Result<String> {
    let mut buffer = String::new();
    io::stdin()
        .read_to_string(&mut buffer)
        .context("failed to read from stdin")?;
    Ok(buffer)
}

/// Check if content appears to be a git diff format
///
/// Detects unified diff format by looking for characteristic headers:
/// - `diff --git a/... b/...`
/// - `--- a/...` or `--- /dev/null`
/// - `+++ b/...` or `+++ /dev/null`
#[must_use]
pub(crate) fn is_diff_format(content: &str) -> bool {
    // Check for git diff header
    if content.contains("diff --git ") {
        return true;
    }

    // Check for unified diff markers (both must be present)
    let has_minus_marker =
        content.contains("\n--- ") || content.starts_with("--- ") || content.contains("--- a/");
    let has_plus_marker =
        content.contains("\n+++ ") || content.starts_with("+++ ") || content.contains("+++ b/");

    has_minus_marker && has_plus_marker
}

/// Read input from a source
///
/// # Arguments
/// * `from` - Optional path to read from. If Some("-"), reads from stdin.
///   If Some(path), reads from file. If None, blocks on stdin.
///
/// # Errors
/// Returns error if:
/// - File read fails
/// - stdin read fails
/// - Input is empty
pub(crate) async fn read_input(from: Option<&PathBuf>) -> Result<String> {
    let content = match from {
        Some(path) if path.as_os_str() == "-" => read_stdin()?,
        Some(path) => tokio::fs::read_to_string(path)
            .await
            .with_context(|| format!("failed to read from {}", path.display()))?,
        None => read_stdin()?,
    };

    if content.trim().is_empty() {
        anyhow::bail!("input is empty; provide content to process");
    }

    Ok(content)
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code uses unwrap for clarity")]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::fs;

    #[tokio::test]
    async fn test_read_input_from_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "test content").await.unwrap();

        let result = read_input(Some(&file_path)).await.unwrap();
        assert_eq!(result, "test content");
    }

    #[tokio::test]
    async fn test_read_input_file_not_found() {
        let result = read_input(Some(&PathBuf::from("/nonexistent/file.txt"))).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("failed to read"));
    }

    #[tokio::test]
    async fn test_read_input_empty_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("empty.txt");
        fs::write(&file_path, "").await.unwrap();

        let result = read_input(Some(&file_path)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("input is empty"));
    }

    #[tokio::test]
    async fn test_read_input_whitespace_only() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("whitespace.txt");
        fs::write(&file_path, "   \n\t  ").await.unwrap();

        let result = read_input(Some(&file_path)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("input is empty"));
    }

    #[test]
    fn test_is_diff_format_git_diff() {
        let content = r#"diff --git a/src/main.rs b/src/main.rs
index 1234567..abcdefg 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 fn main() {
+    println!("hello");
 }
"#;
        assert!(is_diff_format(content));
    }

    #[test]
    fn test_is_diff_format_unified_diff() {
        let content = "--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,4 @@
 line1
+new line
 line2
";
        assert!(is_diff_format(content));
    }

    #[test]
    fn test_is_diff_format_dev_null() {
        let content = "diff --git a/new_file.rs b/new_file.rs
new file mode 100644
--- /dev/null
+++ b/new_file.rs
@@ -0,0 +1,3 @@
+fn new() {}
";
        assert!(is_diff_format(content));
    }

    #[test]
    fn test_is_diff_format_not_diff_rust_code() {
        let content = r#"fn main() {
    println!("Hello, world!");
}
"#;
        assert!(!is_diff_format(content));
    }

    #[test]
    fn test_is_diff_format_not_diff_text() {
        let content = "This is just some plain text content.";
        assert!(!is_diff_format(content));
    }

    #[test]
    fn test_is_diff_format_partial_markers_only_minus() {
        // Only --- marker, no +++ marker
        let content = "--- a/file.txt\nsome content";
        assert!(!is_diff_format(content));
    }

    #[test]
    fn test_is_diff_format_partial_markers_only_plus() {
        // Only +++ marker, no --- marker
        let content = "+++ b/file.txt\nsome content";
        assert!(!is_diff_format(content));
    }
}
