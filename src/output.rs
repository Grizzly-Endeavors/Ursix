//! Output formatting for CLI commands
//!
//! Provides structured output types that can be rendered as either
//! human-readable text or JSON for scripting/automation.

use serde::Serialize;

/// Output mode for command results
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    /// Human-readable text output
    Human,
    /// JSON output for scripting
    Json,
}

/// Trait for command outputs that can be rendered in multiple formats
pub trait CommandOutput: Serialize {
    /// Render as human-readable text
    fn render_human(&self) -> String;

    /// Render as JSON
    fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Render in the specified output mode
    fn render(&self, mode: OutputMode) -> String {
        match mode {
            OutputMode::Human => self.render_human(),
            OutputMode::Json => self.render_json(),
        }
    }
}

/// Generic wrapper for command results
#[derive(Debug, Serialize)]
pub struct CommandResult<T: Serialize> {
    /// Whether the command succeeded
    pub success: bool,
    /// The result data (if successful)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    /// Error message (if failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T: Serialize> CommandResult<T> {
    /// Create a successful result
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    /// Create a failed result
    pub fn err(error: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error.into()),
        }
    }
}

/// Result from the `ask` command
#[derive(Debug, Serialize)]
pub struct AskResult {
    /// The LLM's response
    pub response: String,
    /// Number of agent turns used
    pub turns: usize,
}

impl CommandOutput for AskResult {
    fn render_human(&self) -> String {
        self.response.clone()
    }
}

/// Result from the `config` command
#[derive(Debug, Serialize)]
pub struct ConfigResult {
    /// Configuration entries
    pub entries: Vec<ConfigEntry>,
}

/// A single configuration entry
#[derive(Debug, Serialize)]
pub struct ConfigEntry {
    /// Configuration key
    pub key: String,
    /// Configuration value
    pub value: String,
}

impl CommandOutput for ConfigResult {
    fn render_human(&self) -> String {
        self.entries
            .iter()
            .map(|e| format!("{} = {}", e.key, e.value))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Result from the `models` command
#[derive(Debug, Serialize)]
pub struct ModelsResult {
    /// Available models
    pub models: Vec<String>,
}

impl CommandOutput for ModelsResult {
    fn render_human(&self) -> String {
        self.models.join("\n")
    }
}

/// Result from the `commit` command
#[derive(Debug, Serialize)]
pub struct CommitResult {
    /// The generated commit message
    pub message: String,
    /// Commit title (first line)
    pub title: String,
    /// Commit body (remaining lines)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

impl CommandOutput for CommitResult {
    fn render_human(&self) -> String {
        self.message.clone()
    }
}

/// Result from the `review` command
#[derive(Debug, Serialize)]
pub struct ReviewResult {
    /// The review summary
    pub summary: String,
    /// Identified issues
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<ReviewIssue>,
}

/// An issue identified during code review
#[derive(Debug, Serialize)]
pub struct ReviewIssue {
    /// Issue severity (error, warning, info)
    pub severity: String,
    /// File path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Line number
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    /// Issue description
    pub message: String,
}

impl CommandOutput for ReviewResult {
    fn render_human(&self) -> String {
        self.summary.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ask_result_human_output() {
        let result = AskResult {
            response: "Hello, world!".to_string(),
            turns: 1,
        };
        assert_eq!(result.render_human(), "Hello, world!");
    }

    #[test]
    fn test_ask_result_json_output() {
        let result = AskResult {
            response: "Hello".to_string(),
            turns: 2,
        };
        let json = result.render_json();
        assert!(json.contains("\"response\": \"Hello\""));
        assert!(json.contains("\"turns\": 2"));
    }

    #[test]
    fn test_config_result_human_output() {
        let result = ConfigResult {
            entries: vec![
                ConfigEntry {
                    key: "model".to_string(),
                    value: "llama3.2".to_string(),
                },
                ConfigEntry {
                    key: "max_turns".to_string(),
                    value: "50".to_string(),
                },
            ],
        };
        let output = result.render_human();
        assert!(output.contains("model = llama3.2"));
        assert!(output.contains("max_turns = 50"));
    }

    #[test]
    fn test_models_result_human_output() {
        let result = ModelsResult {
            models: vec!["llama3.2".to_string(), "qwen2.5-coder".to_string()],
        };
        let output = result.render_human();
        assert_eq!(output, "llama3.2\nqwen2.5-coder");
    }

    #[test]
    fn test_command_result_ok() {
        let result = CommandResult::ok("success".to_string());
        assert!(result.success);
        assert_eq!(result.data, Some("success".to_string()));
        assert!(result.error.is_none());
    }

    #[test]
    fn test_command_result_err() {
        let result: CommandResult<String> = CommandResult::err("failed");
        assert!(!result.success);
        assert!(result.data.is_none());
        assert_eq!(result.error, Some("failed".to_string()));
    }
}
