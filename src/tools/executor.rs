use std::path::Path;
use std::time::Duration;

use crate::llm::ToolDefinition;

use super::{ToolResult, bash, file, search};

/// Returns tool definitions for all available tools
pub fn all_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        bash_definition(),
        read_definition(),
        write_definition(),
        edit_definition(),
        glob_definition(),
        grep_definition(),
    ]
}

/// Execute a tool by name with JSON arguments
pub async fn execute_tool(
    name: &str,
    arguments: &serde_json::Value,
    working_dir: &Path,
) -> ToolResult {
    match name {
        "bash" => execute_bash(arguments).await,
        "read" => execute_read(arguments, working_dir).await,
        "write" => execute_write(arguments, working_dir).await,
        "edit" => execute_edit(arguments, working_dir).await,
        "glob" => execute_glob(arguments, working_dir).await,
        "grep" => execute_grep(arguments, working_dir).await,
        _ => ToolResult::failure(format!("unknown tool: {name}")),
    }
}

fn bash_definition() -> ToolDefinition {
    ToolDefinition {
        name: "bash".to_string(),
        description: "Execute a bash command. Use for running shell commands, git operations, \
                      build tools, etc. Commands run in the working directory."
            .to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The bash command to execute"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Optional timeout in seconds (default: 120)"
                }
            },
            "required": ["command"]
        }),
    }
}

fn read_definition() -> ToolDefinition {
    ToolDefinition {
        name: "read".to_string(),
        description: "Read the contents of a file. Returns the full file contents.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to read (relative to working directory)"
                },
                "line_numbers": {
                    "type": "boolean",
                    "description": "Whether to include line numbers (default: false)"
                }
            },
            "required": ["path"]
        }),
    }
}

fn write_definition() -> ToolDefinition {
    ToolDefinition {
        name: "write".to_string(),
        description: "Write contents to a file, creating it if it doesn't exist or overwriting \
                      if it does. Parent directories are created automatically."
            .to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to write (relative to working directory)"
                },
                "contents": {
                    "type": "string",
                    "description": "The contents to write to the file"
                }
            },
            "required": ["path", "contents"]
        }),
    }
}

fn edit_definition() -> ToolDefinition {
    ToolDefinition {
        name: "edit".to_string(),
        description: "Edit a file by replacing one string with another. The old_string must \
                      appear exactly once in the file to ensure precise edits."
            .to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to edit (relative to working directory)"
                },
                "old_string": {
                    "type": "string",
                    "description": "The exact string to find and replace (must be unique in file)"
                },
                "new_string": {
                    "type": "string",
                    "description": "The string to replace it with"
                }
            },
            "required": ["path", "old_string", "new_string"]
        }),
    }
}

fn glob_definition() -> ToolDefinition {
    ToolDefinition {
        name: "glob".to_string(),
        description: "Find files matching a glob pattern. Returns paths sorted by modification \
                      time (newest first). Use patterns like '**/*.rs' for recursive search."
            .to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern to match (e.g., '**/*.rs', 'src/*.txt')"
                }
            },
            "required": ["pattern"]
        }),
    }
}

fn grep_definition() -> ToolDefinition {
    ToolDefinition {
        name: "grep".to_string(),
        description: "Search file contents using a regex pattern. Returns matching lines with \
                      file paths and line numbers. Limited to 100 matches."
            .to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regex pattern to search for"
                },
                "file_pattern": {
                    "type": "string",
                    "description": "Optional glob pattern to filter files (default: '**/*')"
                }
            },
            "required": ["pattern"]
        }),
    }
}

async fn execute_bash(arguments: &serde_json::Value) -> ToolResult {
    let Some(command) = arguments.get("command").and_then(|v| v.as_str()) else {
        return ToolResult::failure("missing required argument: command");
    };

    let timeout = arguments
        .get("timeout_secs")
        .and_then(serde_json::Value::as_u64)
        .map(Duration::from_secs);

    match bash::execute(command, timeout).await {
        Ok(result) => result,
        Err(e) => ToolResult::failure(format!("execution error: {e}")),
    }
}

async fn execute_read(arguments: &serde_json::Value, working_dir: &Path) -> ToolResult {
    let Some(path_str) = arguments.get("path").and_then(|v| v.as_str()) else {
        return ToolResult::failure("missing required argument: path");
    };

    let line_numbers = arguments
        .get("line_numbers")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    let path = working_dir.join(path_str);

    match file::read(&path, line_numbers).await {
        Ok(result) => result,
        Err(e) => ToolResult::failure(format!("execution error: {e}")),
    }
}

async fn execute_write(arguments: &serde_json::Value, working_dir: &Path) -> ToolResult {
    let Some(path_str) = arguments.get("path").and_then(|v| v.as_str()) else {
        return ToolResult::failure("missing required argument: path");
    };

    let Some(contents) = arguments.get("contents").and_then(|v| v.as_str()) else {
        return ToolResult::failure("missing required argument: contents");
    };

    let path = working_dir.join(path_str);

    match file::write(&path, contents).await {
        Ok(result) => result,
        Err(e) => ToolResult::failure(format!("execution error: {e}")),
    }
}

async fn execute_edit(arguments: &serde_json::Value, working_dir: &Path) -> ToolResult {
    let Some(path_str) = arguments.get("path").and_then(|v| v.as_str()) else {
        return ToolResult::failure("missing required argument: path");
    };

    let Some(old_string) = arguments.get("old_string").and_then(|v| v.as_str()) else {
        return ToolResult::failure("missing required argument: old_string");
    };

    let Some(new_string) = arguments.get("new_string").and_then(|v| v.as_str()) else {
        return ToolResult::failure("missing required argument: new_string");
    };

    let path = working_dir.join(path_str);

    match file::edit(&path, old_string, new_string).await {
        Ok(result) => result,
        Err(e) => ToolResult::failure(format!("execution error: {e}")),
    }
}

async fn execute_glob(arguments: &serde_json::Value, working_dir: &Path) -> ToolResult {
    let Some(pattern) = arguments.get("pattern").and_then(|v| v.as_str()) else {
        return ToolResult::failure("missing required argument: pattern");
    };

    match search::glob_search(working_dir, pattern).await {
        Ok(result) => result,
        Err(e) => ToolResult::failure(format!("execution error: {e}")),
    }
}

async fn execute_grep(arguments: &serde_json::Value, working_dir: &Path) -> ToolResult {
    let Some(pattern) = arguments.get("pattern").and_then(|v| v.as_str()) else {
        return ToolResult::failure("missing required argument: pattern");
    };

    let file_pattern = arguments.get("file_pattern").and_then(|v| v.as_str());

    match search::grep_search(working_dir, pattern, file_pattern).await {
        Ok(result) => result,
        Err(e) => ToolResult::failure(format!("execution error: {e}")),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_bash_execution() {
        let args = serde_json::json!({"command": "echo hello"});
        let temp = TempDir::new().unwrap();
        let result = execute_tool("bash", &args, temp.path()).await;
        assert!(result.success);
        assert!(result.output.contains("hello"));
    }

    #[tokio::test]
    async fn test_unknown_tool() {
        let args = serde_json::json!({});
        let temp = TempDir::new().unwrap();
        let result = execute_tool("nonexistent", &args, temp.path()).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("unknown tool"));
    }

    #[tokio::test]
    async fn test_missing_required_arg() {
        let args = serde_json::json!({});
        let temp = TempDir::new().unwrap();
        let result = execute_tool("bash", &args, temp.path()).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("missing required argument"));
    }

    #[tokio::test]
    async fn test_read_file() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.txt");
        tokio::fs::write(&file_path, "test content").await.unwrap();

        let args = serde_json::json!({"path": "test.txt"});
        let result = execute_tool("read", &args, temp.path()).await;
        assert!(result.success);
        assert_eq!(result.output, "test content");
    }

    #[tokio::test]
    async fn test_write_file() {
        let temp = TempDir::new().unwrap();

        let args = serde_json::json!({
            "path": "new_file.txt",
            "contents": "new content"
        });
        let result = execute_tool("write", &args, temp.path()).await;
        assert!(result.success);

        let content = tokio::fs::read_to_string(temp.path().join("new_file.txt"))
            .await
            .unwrap();
        assert_eq!(content, "new content");
    }

    #[tokio::test]
    async fn test_glob_search() {
        let temp = TempDir::new().unwrap();
        tokio::fs::write(temp.path().join("test.rs"), "fn main() {}")
            .await
            .unwrap();

        let args = serde_json::json!({"pattern": "*.rs"});
        let result = execute_tool("glob", &args, temp.path()).await;
        assert!(result.success);
        assert!(result.output.contains("test.rs"));
    }

    #[tokio::test]
    async fn test_all_tool_definitions() {
        let definitions = all_tool_definitions();
        assert_eq!(definitions.len(), 6);

        let names: Vec<_> = definitions.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"bash"));
        assert!(names.contains(&"read"));
        assert!(names.contains(&"write"));
        assert!(names.contains(&"edit"));
        assert!(names.contains(&"glob"));
        assert!(names.contains(&"grep"));
    }
}
