use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;
use tokio::time::timeout;

use super::{ToolError, ToolResult};

/// Default timeout for bash commands (2 minutes)
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// Maximum output size in bytes (64KB)
const MAX_OUTPUT_SIZE: usize = 64 * 1024;

/// Execute a bash command with timeout and output limits
pub async fn execute(
    command: &str,
    timeout_duration: Option<Duration>,
) -> Result<ToolResult, ToolError> {
    let timeout_duration = timeout_duration.unwrap_or(DEFAULT_TIMEOUT);

    let result = timeout(timeout_duration, async {
        Command::new("sh")
            .arg("-c")
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
    })
    .await;

    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            let mut combined_output = String::new();
            if !stdout.is_empty() {
                combined_output.push_str(&stdout);
            }
            if !stderr.is_empty() {
                if !combined_output.is_empty() {
                    combined_output.push_str("\n--- stderr ---\n");
                }
                combined_output.push_str(&stderr);
            }

            // Truncate if too large
            if combined_output.len() > MAX_OUTPUT_SIZE {
                combined_output.truncate(MAX_OUTPUT_SIZE);
                combined_output.push_str("\n... (output truncated)");
            }

            if output.status.success() {
                Ok(ToolResult::success(combined_output))
            } else {
                Ok(ToolResult {
                    success: false,
                    output: combined_output,
                    error: Some(format!("Command exited with status: {}", output.status)),
                })
            }
        }
        Ok(Err(e)) => Err(ToolError::Io(e)),
        Err(_) => Ok(ToolResult::failure(format!(
            "Command timed out after {} seconds",
            timeout_duration.as_secs()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_echo_command() {
        let result = execute("echo 'hello world'", None).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output.trim(), "hello world");
    }

    #[tokio::test]
    async fn test_failing_command() {
        let result = execute("exit 1", None).await.unwrap();
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn test_timeout() {
        let result = execute("sleep 10", Some(Duration::from_millis(100)))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("timed out"));
    }
}
