//! Output types for the `status` command

use super::{CommandOutput, ExitCode, ExitStatus};
use serde::Serialize;

/// A single status check result
#[derive(Debug, Clone, Serialize)]
pub(crate) struct StatusCheck {
    /// Name of the check
    pub name: String,
    /// Whether the check passed
    pub passed: bool,
    /// Human-readable message describing the result
    pub message: String,
    /// Additional details (only shown in verbose mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl StatusCheck {
    /// Create a passing check
    #[must_use]
    pub(crate) fn pass(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            passed: true,
            message: message.into(),
            details: None,
        }
    }

    /// Create a failing check
    #[must_use]
    pub(crate) fn fail(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            passed: false,
            message: message.into(),
            details: None,
        }
    }

    /// Add details to this check
    #[must_use]
    pub(crate) fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }
}

/// Result from the `status` command
#[derive(Debug, Clone, Serialize)]
pub(crate) struct StatusResult {
    /// Overall status: true if all checks passed
    pub ok: bool,
    /// Provider name (ollama, openai)
    pub provider: String,
    /// Provider URL
    pub provider_url: String,
    /// Configured model
    pub model: String,
    /// Whether an API key is set
    pub api_key_set: bool,
    /// Individual check results
    pub checks: Vec<StatusCheck>,
    /// Warnings (non-fatal issues)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// Verbose mode information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verbose: Option<VerboseInfo>,
}

/// Additional information shown in verbose mode
#[derive(Debug, Clone, Serialize)]
pub(crate) struct VerboseInfo {
    /// Response time in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_time_ms: Option<u64>,
    /// Available models (if retrieved)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_models: Option<Vec<String>>,
    /// Whether the configured model is available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_available: Option<bool>,
}

impl StatusResult {
    /// Create a new status result
    #[must_use]
    pub(crate) fn new(
        provider: impl Into<String>,
        provider_url: impl Into<String>,
        model: impl Into<String>,
        api_key_set: bool,
    ) -> Self {
        Self {
            ok: true,
            provider: provider.into(),
            provider_url: provider_url.into(),
            model: model.into(),
            api_key_set,
            checks: Vec::new(),
            warnings: Vec::new(),
            verbose: None,
        }
    }

    /// Add a check to the result
    pub(crate) fn add_check(&mut self, check: StatusCheck) {
        if !check.passed {
            self.ok = false;
        }
        self.checks.push(check);
    }

    /// Add a warning
    pub(crate) fn add_warning(&mut self, warning: impl Into<String>) {
        self.warnings.push(warning.into());
    }

    /// Set verbose information
    pub(crate) fn set_verbose(&mut self, verbose: VerboseInfo) {
        self.verbose = Some(verbose);
    }
}

impl CommandOutput for StatusResult {
    fn render_human(&self) -> String {
        let mut lines = Vec::new();

        // Header
        if self.ok {
            lines.push("Ursix Status: OK".to_string());
        } else {
            lines.push("Ursix Status: FAILED".to_string());
        }
        lines.push(String::new());

        // Provider info
        lines.push(format!(
            "Provider:  {} ({})",
            self.provider, self.provider_url
        ));
        lines.push(format!("Model:     {}", self.model));
        lines.push(format!(
            "API Key:   {}",
            if self.api_key_set { "set" } else { "not set" }
        ));
        lines.push(String::new());

        // Checks
        lines.push("Checks:".to_string());
        for check in &self.checks {
            let status = if check.passed { "[ok]" } else { "[FAIL]" };
            lines.push(format!("  {} {}: {}", status, check.name, check.message));
            if let Some(ref details) = check.details {
                // Indent details
                for detail_line in details.lines() {
                    lines.push(format!("      {detail_line}"));
                }
            }
        }

        // Warnings
        if !self.warnings.is_empty() {
            lines.push(String::new());
            lines.push("Warnings:".to_string());
            for warning in &self.warnings {
                lines.push(format!("  - {warning}"));
            }
        }

        // Verbose info
        if let Some(ref verbose) = self.verbose {
            lines.push(String::new());
            lines.push("Details:".to_string());
            if let Some(time_ms) = verbose.response_time_ms {
                lines.push(format!("  Response time: {time_ms}ms"));
            }
            if let Some(available) = verbose.model_available {
                let status = if available { "available" } else { "not found" };
                lines.push(format!("  Model status: {status}"));
            }
            if let Some(ref models) = verbose.available_models {
                let display_count = 10;
                let model_list = if models.len() <= display_count {
                    models.join(", ")
                } else {
                    let first_models = models
                        .get(..display_count)
                        .map(|slice| slice.join(", "))
                        .unwrap_or_default();
                    format!(
                        "{}, ... and {} more",
                        first_models,
                        models.len() - display_count
                    )
                };
                lines.push(format!("  Available models: {model_list}"));
            }
        }

        lines.join("\n")
    }
}

impl ExitStatus for StatusResult {
    fn exit_code(&self) -> ExitCode {
        if self.ok {
            ExitCode::Success
        } else {
            // Determine the appropriate error code based on failed checks
            for check in &self.checks {
                if !check.passed {
                    // API key issues and config problems are user errors
                    if check.name == "api_key" || check.name == "config" {
                        return ExitCode::UserError;
                    }
                    // Connectivity issues are transient
                    if check.name == "connectivity" {
                        return ExitCode::TransientError;
                    }
                }
            }
            // Default to user error
            ExitCode::UserError
        }
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code uses unwrap for clarity")]
mod tests {
    use super::*;

    #[test]
    fn test_status_check_pass() {
        let check = StatusCheck::pass("config", "loaded successfully");
        assert!(check.passed);
        assert_eq!(check.name, "config");
        assert_eq!(check.message, "loaded successfully");
        assert!(check.details.is_none());
    }

    #[test]
    fn test_status_check_fail_with_details() {
        let check = StatusCheck::fail("api_key", "required for OpenAI")
            .with_details("Set URSIX_API_KEY environment variable");
        assert!(!check.passed);
        assert_eq!(check.name, "api_key");
        assert!(check.details.is_some());
    }

    #[test]
    fn test_status_result_ok_when_all_pass() {
        let mut result = StatusResult::new("ollama", "http://localhost:11434", "llama3.2", false);
        result.add_check(StatusCheck::pass("config", "ok"));
        result.add_check(StatusCheck::pass("connectivity", "ok"));
        assert!(result.ok);
    }

    #[test]
    fn test_status_result_not_ok_when_check_fails() {
        let mut result = StatusResult::new("openai", "https://api.openai.com/v1", "gpt-4", false);
        result.add_check(StatusCheck::pass("config", "ok"));
        result.add_check(StatusCheck::fail("api_key", "required"));
        assert!(!result.ok);
    }

    #[test]
    fn test_status_result_exit_code_success() {
        let mut result = StatusResult::new("ollama", "http://localhost:11434", "llama3.2", false);
        result.add_check(StatusCheck::pass("config", "ok"));
        assert_eq!(result.exit_code(), ExitCode::Success);
    }

    #[test]
    fn test_status_result_exit_code_user_error_for_api_key() {
        let mut result = StatusResult::new("openai", "https://api.openai.com/v1", "gpt-4", false);
        result.add_check(StatusCheck::fail("api_key", "required"));
        assert_eq!(result.exit_code(), ExitCode::UserError);
    }

    #[test]
    fn test_status_result_exit_code_transient_for_connectivity() {
        let mut result = StatusResult::new("ollama", "http://localhost:11434", "llama3.2", false);
        result.add_check(StatusCheck::pass("api_key", "not required"));
        result.add_check(StatusCheck::fail("connectivity", "connection refused"));
        assert_eq!(result.exit_code(), ExitCode::TransientError);
    }

    #[test]
    fn test_status_result_human_output() {
        let mut result = StatusResult::new("ollama", "http://localhost:11434", "llama3.2", false);
        result.add_check(StatusCheck::pass(
            "config",
            "configuration loaded successfully",
        ));
        result.add_check(StatusCheck::pass(
            "connectivity",
            "connected, model available",
        ));

        let output = result.render_human();
        assert!(output.contains("Ursix Status: OK"));
        assert!(output.contains("Provider:  ollama"));
        assert!(output.contains("[ok] config"));
        assert!(output.contains("[ok] connectivity"));
    }

    #[test]
    fn test_status_result_human_output_failed() {
        let mut result = StatusResult::new("openai", "https://api.openai.com/v1", "gpt-4", false);
        result.add_check(StatusCheck::pass("config", "ok"));
        result.add_check(
            StatusCheck::fail("api_key", "required for remote OpenAI endpoint")
                .with_details("Set URSIX_API_KEY environment variable"),
        );

        let output = result.render_human();
        assert!(output.contains("Ursix Status: FAILED"));
        assert!(output.contains("[FAIL] api_key"));
        assert!(output.contains("Set URSIX_API_KEY"));
    }

    #[test]
    fn test_status_result_json_output() {
        let mut result = StatusResult::new("ollama", "http://localhost:11434", "llama3.2", false);
        result.add_check(StatusCheck::pass("config", "ok"));

        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json.get("ok"), Some(&serde_json::json!(true)));
        assert_eq!(json.get("provider"), Some(&serde_json::json!("ollama")));
        assert_eq!(
            json.get("checks")
                .and_then(|checks| checks.get(0))
                .and_then(|c| c.get("name")),
            Some(&serde_json::json!("config"))
        );
        assert_eq!(
            json.get("checks")
                .and_then(|checks| checks.get(0))
                .and_then(|c| c.get("passed")),
            Some(&serde_json::json!(true))
        );
    }

    #[test]
    fn test_status_result_with_verbose() {
        let mut result = StatusResult::new("ollama", "http://localhost:11434", "llama3.2", false);
        result.set_verbose(VerboseInfo {
            response_time_ms: Some(150),
            available_models: Some(vec!["llama3.2".to_string(), "codellama".to_string()]),
            model_available: Some(true),
        });

        let output = result.render_human();
        assert!(output.contains("Response time: 150ms"));
        assert!(output.contains("Model status: available"));
        assert!(output.contains("Available models:"));
    }
}
