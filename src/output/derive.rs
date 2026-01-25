//! Output types for the `derive` command

use super::{CommandOutput, ExitCode, ExitStatus};
use serde::Serialize;

/// Result from the `derive` command
///
/// The result varies based on the derive type requested.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeriveResult {
    /// Result for commit-msg derive type
    CommitMsg {
        /// The full commit message (title + body)
        message: String,
        /// Commit title (first line, under 50 chars)
        title: String,
        /// Optional commit body
        #[serde(skip_serializing_if = "Option::is_none")]
        body: Option<String>,
    },
    /// Result for explanation derive type
    Explanation {
        /// The explanation of the code
        explanation: String,
    },
    /// Result for summary derive type
    Summary {
        /// The summary of the content
        summary: String,
    },
}

impl CommandOutput for DeriveResult {
    fn render_human(&self) -> String {
        match self {
            Self::CommitMsg { message, .. } => message.clone(),
            Self::Explanation { explanation } => explanation.clone(),
            Self::Summary { summary } => summary.clone(),
        }
    }
}

impl ExitStatus for DeriveResult {
    fn exit_code(&self) -> ExitCode {
        ExitCode::Success
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_result_commit_msg_exit_status() {
        let result = DeriveResult::CommitMsg {
            message: "feat: add feature".to_string(),
            title: "feat: add feature".to_string(),
            body: None,
        };
        assert_eq!(result.exit_code(), ExitCode::Success);
    }

    #[test]
    fn test_derive_result_commit_msg_human_output() {
        let result = DeriveResult::CommitMsg {
            message: "feat: add feature\n\nThis adds a new feature.".to_string(),
            title: "feat: add feature".to_string(),
            body: Some("This adds a new feature.".to_string()),
        };
        assert_eq!(
            result.render_human(),
            "feat: add feature\n\nThis adds a new feature."
        );
    }

    #[test]
    fn test_derive_result_explanation_exit_status() {
        let result = DeriveResult::Explanation {
            explanation: "This code does...".to_string(),
        };
        assert_eq!(result.exit_code(), ExitCode::Success);
    }

    #[test]
    fn test_derive_result_explanation_human_output() {
        let result = DeriveResult::Explanation {
            explanation: "This function calculates...".to_string(),
        };
        assert_eq!(result.render_human(), "This function calculates...");
    }

    #[test]
    fn test_derive_result_summary_exit_status() {
        let result = DeriveResult::Summary {
            summary: "A brief summary".to_string(),
        };
        assert_eq!(result.exit_code(), ExitCode::Success);
    }

    #[test]
    fn test_derive_result_summary_human_output() {
        let result = DeriveResult::Summary {
            summary: "This is a summary of the content.".to_string(),
        };
        assert_eq!(result.render_human(), "This is a summary of the content.");
    }

    #[test]
    fn test_derive_result_commit_msg_json_serialization() {
        let result = DeriveResult::CommitMsg {
            message: "feat: test".to_string(),
            title: "feat: test".to_string(),
            body: None,
        };
        let json = result.render_json().unwrap();
        assert!(json.contains("\"type\": \"commit_msg\""));
        assert!(json.contains("\"message\": \"feat: test\""));
    }

    #[test]
    fn test_derive_result_explanation_json_serialization() {
        let result = DeriveResult::Explanation {
            explanation: "Test explanation".to_string(),
        };
        let json = result.render_json().unwrap();
        assert!(json.contains("\"type\": \"explanation\""));
        assert!(json.contains("\"explanation\": \"Test explanation\""));
    }

    #[test]
    fn test_derive_result_summary_json_serialization() {
        let result = DeriveResult::Summary {
            summary: "Test summary".to_string(),
        };
        let json = result.render_json().unwrap();
        assert!(json.contains("\"type\": \"summary\""));
        assert!(json.contains("\"summary\": \"Test summary\""));
    }
}
