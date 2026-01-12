use std::path::Path;

use tokio::fs;

use super::{ToolError, ToolResult};

/// Read a file's contents, optionally with line numbers
pub async fn read(path: &Path, add_line_numbers: bool) -> Result<ToolResult, ToolError> {
    match fs::read_to_string(path).await {
        Ok(contents) => {
            let output = if add_line_numbers {
                contents
                    .lines()
                    .enumerate()
                    .map(|(i, line)| format!("{:>6}\t{}", i + 1, line))
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                contents
            };
            Ok(ToolResult::success(output))
        }
        Err(e) => Ok(ToolResult::failure(format!("Failed to read file: {e}"))),
    }
}

/// Write contents to a file (creates or overwrites)
pub async fn write(path: &Path, contents: &str) -> Result<ToolResult, ToolError> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).await?;
    }

    match fs::write(path, contents).await {
        Ok(()) => Ok(ToolResult::success(format!(
            "Successfully wrote {} bytes to {}",
            contents.len(),
            path.display()
        ))),
        Err(e) => Ok(ToolResult::failure(format!("Failed to write file: {e}"))),
    }
}

/// Edit a file by replacing `old_string` with `new_string`
pub async fn edit(
    path: &Path,
    old_string: &str,
    new_string: &str,
) -> Result<ToolResult, ToolError> {
    if old_string.is_empty() {
        return Ok(ToolResult::failure("old_string cannot be empty"));
    }

    let contents = match fs::read_to_string(path).await {
        Ok(c) => c,
        Err(e) => return Ok(ToolResult::failure(format!("Failed to read file: {e}"))),
    };

    let count = contents.matches(old_string).count();
    if count == 0 {
        return Ok(ToolResult::failure(
            "old_string not found in file. Make sure to match exact whitespace and content.",
        ));
    }
    if count > 1 {
        return Ok(ToolResult::failure(format!(
            "old_string found {count} times in file. It must be unique. Add more context to make it unique."
        )));
    }

    let new_contents = contents.replace(old_string, new_string);
    fs::write(path, &new_contents).await?;

    Ok(ToolResult::success(format!(
        "Successfully edited {}",
        path.display()
    )))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_write_and_read() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let write_result = write(&file_path, "hello world").await.unwrap();
        assert!(write_result.success);

        let read_result = read(&file_path, false).await.unwrap();
        assert!(read_result.success);
        assert_eq!(read_result.output, "hello world");
    }

    #[tokio::test]
    async fn test_read_with_line_numbers() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        write(&file_path, "line1\nline2\nline3").await.unwrap();

        let result = read(&file_path, true).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("     1\tline1"));
        assert!(result.output.contains("     2\tline2"));
        assert!(result.output.contains("     3\tline3"));
    }

    #[tokio::test]
    async fn test_edit() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        write(&file_path, "hello world").await.unwrap();

        let edit_result = edit(&file_path, "world", "rust").await.unwrap();
        assert!(edit_result.success);

        let read_result = read(&file_path, false).await.unwrap();
        assert_eq!(read_result.output, "hello rust");
    }

    #[tokio::test]
    async fn test_edit_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        write(&file_path, "hello world").await.unwrap();

        let result = edit(&file_path, "missing", "replacement").await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_edit_multiple_matches() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        write(&file_path, "hello hello").await.unwrap();

        let result = edit(&file_path, "hello", "hi").await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("2 times"));
    }
}
