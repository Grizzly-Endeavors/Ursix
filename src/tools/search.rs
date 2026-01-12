use std::fmt::Write as _;
use std::path::Path;

use regex::Regex;
use tokio::fs;

use super::{ToolError, ToolResult};

/// Maximum number of grep matches to return
const MAX_MATCHES: usize = 100;

/// Find files matching a glob pattern, sorted by modification time (newest first)
pub async fn glob_search(base_path: &Path, pattern: &str) -> Result<ToolResult, ToolError> {
    let full_pattern = base_path.join(pattern);
    let pattern_str = full_pattern.to_string_lossy();

    let paths: Result<Vec<_>, _> = glob::glob(&pattern_str)
        .map_err(|e| ToolError::InvalidArgument(format!("Invalid glob pattern: {e}")))?
        .collect();

    let mut paths = paths.map_err(|e| ToolError::ExecutionFailed(format!("Glob error: {e}")))?;

    // Sort by modification time (newest first)
    paths.sort_by(|a, b| {
        let time_a = a.metadata().and_then(|m| m.modified()).ok();
        let time_b = b.metadata().and_then(|m| m.modified()).ok();
        time_b.cmp(&time_a)
    });

    if paths.is_empty() {
        return Ok(ToolResult::success("No files found matching pattern"));
    }

    let output = paths
        .iter()
        .filter_map(|p| {
            p.strip_prefix(base_path)
                .ok()
                .map(|p| p.display().to_string())
        })
        .collect::<Vec<_>>()
        .join("\n");

    Ok(ToolResult::success(output))
}

/// Search for a regex pattern in files, returning matching lines with file paths and line numbers
pub async fn grep_search(
    base_path: &Path,
    pattern: &str,
    file_pattern: Option<&str>,
) -> Result<ToolResult, ToolError> {
    let regex = Regex::new(pattern)
        .map_err(|e| ToolError::InvalidArgument(format!("Invalid regex: {e}")))?;

    let file_glob = file_pattern.unwrap_or("**/*");
    let full_pattern = base_path.join(file_glob);
    let pattern_str = full_pattern.to_string_lossy();

    let paths: Vec<_> = glob::glob(&pattern_str)
        .map_err(|e| ToolError::InvalidArgument(format!("Invalid glob pattern: {e}")))?
        .filter_map(Result::ok)
        .filter(|p| p.is_file())
        .collect();

    let mut matches = Vec::new();

    for path in paths {
        if matches.len() >= MAX_MATCHES {
            break;
        }

        // Skip binary files (simple heuristic: check first 1KB for null bytes)
        let Ok(contents) = fs::read(&path).await else {
            continue;
        };

        let check_len = contents.len().min(1024);
        if contents[..check_len].contains(&0) {
            continue;
        }

        let Ok(text) = String::from_utf8(contents) else {
            continue;
        };

        let relative_path = path
            .strip_prefix(base_path)
            .unwrap_or(&path)
            .display()
            .to_string();

        for (line_num, line) in text.lines().enumerate() {
            if regex.is_match(line) {
                matches.push(format!("{}:{}: {}", relative_path, line_num + 1, line));
                if matches.len() >= MAX_MATCHES {
                    break;
                }
            }
        }
    }

    if matches.is_empty() {
        return Ok(ToolResult::success("No matches found"));
    }

    let mut output = matches.join("\n");
    if matches.len() >= MAX_MATCHES {
        let _ = write!(output, "\n... (limited to {MAX_MATCHES} matches)");
    }

    Ok(ToolResult::success(output))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn setup_test_files(temp_dir: &TempDir) {
        let src_dir = temp_dir.path().join("src");
        fs::create_dir_all(&src_dir).await.unwrap();

        fs::write(
            temp_dir.path().join("README.md"),
            "# Test Project\nThis is a test.",
        )
        .await
        .unwrap();
        fs::write(
            src_dir.join("main.rs"),
            "fn main() {\n    println!(\"hello\");\n}",
        )
        .await
        .unwrap();
        fs::write(
            src_dir.join("lib.rs"),
            "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}",
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_glob_find_rust_files() {
        let temp_dir = TempDir::new().unwrap();
        setup_test_files(&temp_dir).await;

        let result = glob_search(temp_dir.path(), "**/*.rs").await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("main.rs"));
        assert!(result.output.contains("lib.rs"));
    }

    #[tokio::test]
    async fn test_glob_no_matches() {
        let temp_dir = TempDir::new().unwrap();
        setup_test_files(&temp_dir).await;

        let result = glob_search(temp_dir.path(), "**/*.py").await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("No files found"));
    }

    #[tokio::test]
    async fn test_grep_find_pattern() {
        let temp_dir = TempDir::new().unwrap();
        setup_test_files(&temp_dir).await;

        let result = grep_search(temp_dir.path(), "fn main", None).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("main.rs"));
        assert!(result.output.contains("fn main"));
    }

    #[tokio::test]
    async fn test_grep_with_file_filter() {
        let temp_dir = TempDir::new().unwrap();
        setup_test_files(&temp_dir).await;

        let result = grep_search(temp_dir.path(), "fn", Some("**/*.rs"))
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.output.contains(".rs"));
        assert!(!result.output.contains(".md"));
    }

    #[tokio::test]
    async fn test_grep_no_matches() {
        let temp_dir = TempDir::new().unwrap();
        setup_test_files(&temp_dir).await;

        let result = grep_search(temp_dir.path(), "nonexistent_pattern", None)
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.output.contains("No matches found"));
    }
}
