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
pub async fn read_input(from: Option<&PathBuf>) -> Result<String> {
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
#[allow(clippy::unwrap_used)]
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
}
