//! Output types for the `init` command

use super::{CommandOutput, ExitCode, ExitStatus};
use serde::Serialize;

/// Result from the `init` command
#[derive(Debug, Serialize)]
pub(crate) struct InitResult {
    /// Whether initialization succeeded
    pub success: bool,
    /// Files that were created
    pub files_created: Vec<String>,
    /// Provider that was configured
    pub provider: String,
    /// Model that was configured
    pub model: String,
    /// Whether connectivity test passed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connectivity_test: Option<bool>,
    /// Warning messages (non-fatal issues)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

impl CommandOutput for InitResult {
    fn render_human(&self) -> String {
        let mut lines = Vec::new();

        if self.success {
            lines.push("Ursix initialized successfully!".to_string());
        } else {
            lines.push("Ursix initialization completed with issues.".to_string());
        }

        lines.push(String::new());

        if !self.files_created.is_empty() {
            lines.push("Created files:".to_string());
            for file in &self.files_created {
                lines.push(format!("  - {file}"));
            }
            lines.push(String::new());
        }

        lines.push(format!("Provider: {}", self.provider));
        lines.push(format!("Model: {}", self.model));

        if let Some(test_passed) = self.connectivity_test {
            if test_passed {
                lines.push("Connectivity test: passed".to_string());
            } else {
                lines.push("Connectivity test: failed".to_string());
            }
        }

        if !self.warnings.is_empty() {
            lines.push(String::new());
            lines.push("Warnings:".to_string());
            for warning in &self.warnings {
                lines.push(format!("  - {warning}"));
            }
        }

        lines.join("\n")
    }
}

impl ExitStatus for InitResult {
    fn exit_code(&self) -> ExitCode {
        if self.success {
            ExitCode::Success
        } else {
            ExitCode::UserError
        }
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code uses unwrap for clarity")]
mod tests {
    use super::*;

    #[test]
    fn test_init_result_human_output_success() {
        let result = InitResult {
            success: true,
            files_created: vec![".ursix.toml".to_string(), ".ursix/rules.yml".to_string()],
            provider: "ollama".to_string(),
            model: "llama3.2".to_string(),
            connectivity_test: Some(true),
            warnings: vec![],
        };
        let output = result.render_human();
        assert!(output.contains("initialized successfully"));
        assert!(output.contains(".ursix.toml"));
        assert!(output.contains(".ursix/rules.yml"));
        assert!(output.contains("Provider: ollama"));
        assert!(output.contains("Model: llama3.2"));
        assert!(output.contains("Connectivity test: passed"));
    }

    #[test]
    fn test_init_result_human_output_with_warnings() {
        let result = InitResult {
            success: true,
            files_created: vec![".ursix.toml".to_string()],
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            connectivity_test: None,
            warnings: vec!["URSIX_API_KEY not set".to_string()],
        };
        let output = result.render_human();
        assert!(output.contains("Warnings:"));
        assert!(output.contains("URSIX_API_KEY not set"));
    }

    #[test]
    fn test_init_result_exit_status() {
        let success = InitResult {
            success: true,
            files_created: vec![],
            provider: "ollama".to_string(),
            model: "llama3.2".to_string(),
            connectivity_test: None,
            warnings: vec![],
        };
        assert_eq!(success.exit_code(), ExitCode::Success);

        let failure = InitResult {
            success: false,
            files_created: vec![],
            provider: "ollama".to_string(),
            model: "llama3.2".to_string(),
            connectivity_test: None,
            warnings: vec![],
        };
        assert_eq!(failure.exit_code(), ExitCode::UserError);
    }

    #[test]
    fn test_init_result_json_output() {
        let result = InitResult {
            success: true,
            files_created: vec![".ursix.toml".to_string()],
            provider: "ollama".to_string(),
            model: "llama3.2".to_string(),
            connectivity_test: Some(true),
            warnings: vec![],
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"provider\":\"ollama\""));
        assert!(json.contains("\"connectivity_test\":true"));
    }
}
