//! Context gathering for LLM calls
//!
//! This module provides functions to gather relevant context (files, git state, etc.)
//! before making LLM calls. Context is gathered asynchronously using tokio.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tokio::fs;
use tokio::process::Command;

/// Represents a file's path and contents
#[derive(Debug, Clone)]
pub struct FileContext {
    /// Path to the file
    pub path: PathBuf,
    /// Contents of the file
    pub content: String,
}

/// Collected context for an LLM call
#[derive(Debug, Clone, Default)]
pub struct GatheredContext {
    /// Files and their contents
    pub files: Vec<FileContext>,
    /// Git diff output (unstaged or staged changes)
    pub git_diff: Option<String>,
    /// Git status output
    pub git_status: Option<String>,
    /// Additional context (e.g., issues, lint output)
    pub additional_context: Option<String>,
}

/// Run a git command and return its output
async fn run_git_command(working_dir: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(working_dir)
        .output()
        .await
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git {} failed: {}", args.join(" "), stderr.trim())
    }
}

/// Read a file's contents asynchronously
async fn read_file(path: &Path) -> Result<FileContext> {
    let content = fs::read_to_string(path)
        .await
        .with_context(|| format!("failed to read file at {}", path.display()))?;

    Ok(FileContext {
        path: path.to_path_buf(),
        content,
    })
}

/// Read multiple files concurrently
async fn read_files(paths: &[PathBuf]) -> Vec<FileContext> {
    let futures: Vec<_> = paths.iter().map(|p| read_file(p)).collect();
    let results = futures::future::join_all(futures).await;

    results
        .into_iter()
        .filter_map(|r| match r {
            Ok(ctx) => Some(ctx),
            Err(e) => {
                tracing::warn!(error = %e, "failed to read file for context");
                None
            }
        })
        .collect()
}

/// Gather context for code review
///
/// # Arguments
/// * `working_dir` - The working directory (git repository root)
/// * `diff_only` - If true, only gather the diff (staged changes); if false, unstaged changes
/// * `files` - Additional files to read
///
/// # Errors
/// Returns error if git commands fail or working directory is invalid
pub async fn gather_review_context(
    working_dir: &Path,
    diff_only: bool,
    files: &[PathBuf],
) -> Result<GatheredContext> {
    let diff_args = if diff_only {
        vec!["diff", "--cached"]
    } else {
        vec!["diff"]
    };

    let git_diff = run_git_command(working_dir, &diff_args)
        .await
        .context("failed to get git diff for review")?;

    let file_contexts = read_files(files).await;

    Ok(GatheredContext {
        files: file_contexts,
        git_diff: Some(git_diff),
        git_status: None,
        additional_context: None,
    })
}

/// Gather context for generating commit messages
///
/// # Arguments
/// * `working_dir` - The working directory (git repository root)
///
/// # Errors
/// Returns error if git commands fail or working directory is invalid
pub async fn gather_commit_context(working_dir: &Path) -> Result<GatheredContext> {
    // Run git diff --cached and git status concurrently
    let (diff_result, status_result) = tokio::join!(
        run_git_command(working_dir, &["diff", "--cached"]),
        run_git_command(working_dir, &["status", "--short"])
    );

    let git_diff = diff_result.context("failed to get staged changes for commit")?;
    let git_status = status_result.context("failed to get git status for commit")?;

    Ok(GatheredContext {
        files: Vec::new(),
        git_diff: Some(git_diff),
        git_status: Some(git_status),
        additional_context: None,
    })
}

/// Gather context for explaining code
///
/// # Arguments
/// * `working_dir` - The working directory
/// * `files` - Files to read and explain
///
/// # Errors
/// Returns error if file reading fails
pub async fn gather_explain_context(
    working_dir: &Path,
    files: &[PathBuf],
) -> Result<GatheredContext> {
    // Resolve relative paths against working directory
    let resolved_paths: Vec<PathBuf> = files
        .iter()
        .map(|p| {
            if p.is_absolute() {
                p.clone()
            } else {
                working_dir.join(p)
            }
        })
        .collect();

    let file_contexts = read_files(&resolved_paths).await;

    if file_contexts.is_empty() && !files.is_empty() {
        anyhow::bail!("could not read any of the specified files");
    }

    Ok(GatheredContext {
        files: file_contexts,
        git_diff: None,
        git_status: None,
        additional_context: None,
    })
}

/// Gather context for fixing code issues
///
/// # Arguments
/// * `working_dir` - The working directory
/// * `files` - Files to read
/// * `issues` - Optional issues string (e.g., from lint output or --from flag)
///
/// # Errors
/// Returns error if file reading fails
pub async fn gather_fix_context(
    working_dir: &Path,
    files: &[PathBuf],
    issues: Option<&str>,
) -> Result<GatheredContext> {
    // Resolve relative paths against working directory
    let resolved_paths: Vec<PathBuf> = files
        .iter()
        .map(|p| {
            if p.is_absolute() {
                p.clone()
            } else {
                working_dir.join(p)
            }
        })
        .collect();

    let file_contexts = read_files(&resolved_paths).await;

    Ok(GatheredContext {
        files: file_contexts,
        git_diff: None,
        git_status: None,
        additional_context: issues.map(String::from),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_read_file_success() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "test content").await.unwrap();

        let result = read_file(&file_path).await.unwrap();
        assert_eq!(result.content, "test content");
        assert_eq!(result.path, file_path);
    }

    #[tokio::test]
    async fn test_read_file_not_found() {
        let result = read_file(Path::new("/nonexistent/file.txt")).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("failed to read file")
        );
    }

    #[tokio::test]
    async fn test_read_files_partial_success() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("exists.txt");
        fs::write(&file_path, "content").await.unwrap();

        let paths = vec![file_path.clone(), PathBuf::from("/nonexistent/file.txt")];

        let results = read_files(&paths).await;
        // Should only contain the file that exists
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, file_path);
    }

    #[tokio::test]
    async fn test_gathered_context_new() {
        let ctx = GatheredContext::default();
        assert!(ctx.files.is_empty());
        assert!(ctx.git_diff.is_none());
        assert!(ctx.git_status.is_none());
        assert!(ctx.additional_context.is_none());
    }

    #[tokio::test]
    async fn test_gather_explain_context() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("code.rs");
        fs::write(&file_path, "fn main() {}").await.unwrap();

        let result = gather_explain_context(temp_dir.path(), std::slice::from_ref(&file_path))
            .await
            .unwrap();

        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].content, "fn main() {}");
        assert!(result.git_diff.is_none());
    }

    #[tokio::test]
    async fn test_gather_explain_context_relative_path() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("src").join("code.rs");
        fs::create_dir_all(file_path.parent().unwrap())
            .await
            .unwrap();
        fs::write(&file_path, "fn main() {}").await.unwrap();

        // Use relative path
        let relative_path = PathBuf::from("src/code.rs");
        let result = gather_explain_context(temp_dir.path(), &[relative_path])
            .await
            .unwrap();

        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].content, "fn main() {}");
    }

    #[tokio::test]
    async fn test_gather_explain_context_no_files() {
        let temp_dir = TempDir::new().unwrap();
        let result =
            gather_explain_context(temp_dir.path(), &[PathBuf::from("nonexistent.rs")]).await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("could not read any")
        );
    }

    #[tokio::test]
    async fn test_gather_fix_context_with_issues() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("buggy.rs");
        fs::write(&file_path, "let x = 1;").await.unwrap();

        let issues = "error: unused variable `x`";
        let result = gather_fix_context(temp_dir.path(), &[file_path], Some(issues))
            .await
            .unwrap();

        assert_eq!(result.files.len(), 1);
        assert_eq!(result.additional_context, Some(issues.to_string()));
    }

    #[tokio::test]
    async fn test_gather_fix_context_without_issues() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("code.rs");
        fs::write(&file_path, "fn main() {}").await.unwrap();

        let result = gather_fix_context(temp_dir.path(), &[file_path], None)
            .await
            .unwrap();

        assert!(result.additional_context.is_none());
    }

    // Git-related tests require an actual git repository
    // These tests are marked with a helper that creates a temporary git repo

    async fn create_temp_git_repo() -> TempDir {
        let temp_dir = TempDir::new().unwrap();

        // Initialize git repo
        Command::new("git")
            .args(["init"])
            .current_dir(temp_dir.path())
            .output()
            .await
            .unwrap();

        // Configure git user for commits
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(temp_dir.path())
            .output()
            .await
            .unwrap();

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(temp_dir.path())
            .output()
            .await
            .unwrap();

        temp_dir
    }

    #[tokio::test]
    async fn test_gather_commit_context() {
        let temp_dir = create_temp_git_repo().await;

        // Create and stage a file
        let file_path = temp_dir.path().join("new_file.rs");
        fs::write(&file_path, "fn main() {}").await.unwrap();

        Command::new("git")
            .args(["add", "new_file.rs"])
            .current_dir(temp_dir.path())
            .output()
            .await
            .unwrap();

        let result = gather_commit_context(temp_dir.path()).await.unwrap();

        assert!(result.git_diff.is_some());
        assert!(result.git_status.is_some());
        // Staged file should appear in diff
        assert!(result.git_diff.as_ref().unwrap().contains("fn main()"));
    }

    #[tokio::test]
    async fn test_gather_review_context_staged() {
        let temp_dir = create_temp_git_repo().await;

        // Create and stage a file
        let file_path = temp_dir.path().join("review.rs");
        fs::write(&file_path, "fn review() {}").await.unwrap();

        Command::new("git")
            .args(["add", "review.rs"])
            .current_dir(temp_dir.path())
            .output()
            .await
            .unwrap();

        let result = gather_review_context(temp_dir.path(), true, &[])
            .await
            .unwrap();

        assert!(result.git_diff.is_some());
        assert!(result.git_diff.as_ref().unwrap().contains("fn review()"));
    }

    #[tokio::test]
    async fn test_gather_review_context_with_files() {
        let temp_dir = create_temp_git_repo().await;

        // Create files
        let file1 = temp_dir.path().join("file1.rs");
        let file2 = temp_dir.path().join("file2.rs");
        fs::write(&file1, "fn one() {}").await.unwrap();
        fs::write(&file2, "fn two() {}").await.unwrap();

        let result = gather_review_context(temp_dir.path(), false, &[file1.clone(), file2.clone()])
            .await
            .unwrap();

        assert_eq!(result.files.len(), 2);
    }

    #[tokio::test]
    async fn test_run_git_command_failure() {
        let temp_dir = TempDir::new().unwrap();
        // Not a git repo, so git status should fail
        let result = run_git_command(temp_dir.path(), &["status"]).await;
        assert!(result.is_err());
    }
}
