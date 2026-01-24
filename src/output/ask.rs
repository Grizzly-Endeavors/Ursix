//! Output types for generic text responses

use super::{CommandOutput, ExitCode, ExitStatus};
use serde::Serialize;

/// Result from a generic text response
#[derive(Debug, Serialize)]
pub struct AskResult {
    /// The LLM's response
    pub response: String,
    /// Number of LLM calls made (for internal use)
    pub turns: usize,
}

impl CommandOutput for AskResult {
    fn render_human(&self) -> String {
        self.response.clone()
    }
}

impl ExitStatus for AskResult {
    fn exit_code(&self) -> ExitCode {
        ExitCode::Success
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
        let json = result.render_json().unwrap();
        assert!(json.contains("\"response\": \"Hello\""));
        assert!(json.contains("\"turns\": 2"));
    }

    #[test]
    fn test_render_json_returns_result() {
        let result = AskResult {
            response: "test".to_string(),
            turns: 1,
        };
        // render_json should return Ok for valid serializable types
        assert!(result.render_json().is_ok());
    }

    #[test]
    fn test_ask_result_exit_status() {
        let result = AskResult {
            response: "test".to_string(),
            turns: 1,
        };
        assert_eq!(result.exit_code(), ExitCode::Success);
    }
}
