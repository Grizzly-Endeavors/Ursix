//! Output formatting for CLI commands
//!
//! Provides structured output types that can be rendered as either
//! human-readable text or JSON for scripting/automation.

mod ask;
mod commit;
mod config;
mod explain;
mod fix;
mod review;

pub use ask::AskResult;
pub use commit::CommitResult;
pub use config::{ConfigEntry, ConfigResult};
pub use explain::ExplainResult;
pub use fix::{AppliedFix, ApplyResults, ApplyStatus, Fix, FixResult};
pub use review::{ReviewIssue, ReviewResult};

use serde::Serialize;

/// Exit codes for CLI commands
///
/// Follows Unix conventions with granular error categorization:
/// - 0 for success
/// - 1 for issues/warnings found (e.g., review found problems)
/// - 2+ for specific error categories
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExitCode {
    /// Command completed successfully with no issues
    Success = 0,
    /// Command completed but found issues (e.g., review found problems)
    IssuesFound = 1,
    /// Invalid CLI arguments (handled by clap)
    UsageError = 2,
    /// Configuration file errors, invalid settings
    ConfigError = 3,
    /// File not found, cannot read input, stdin errors
    InputError = 4,
    /// Git command failures, not a git repo
    GitError = 5,
    /// HTTP request failures, connection timeouts
    NetworkError = 6,
    /// LLM API errors (auth, rate limits, bad responses)
    ApiError = 7,
    /// Failed to parse LLM response
    ParseError = 8,
    /// Unexpected internal errors (catch-all)
    InternalError = 9,
    /// Input exceeds token limit (suggest --chunk)
    TokenLimitError = 10,
}

/// Trait for error types that can map to an exit code
pub trait ToExitCode {
    /// Returns the appropriate exit code for this error
    fn to_exit_code(&self) -> ExitCode;
}

impl From<ExitCode> for u8 {
    fn from(code: ExitCode) -> Self {
        code as u8
    }
}

impl From<ExitCode> for i32 {
    fn from(code: ExitCode) -> Self {
        i32::from(code as u8)
    }
}

/// Trait for types that can report an exit status
pub trait ExitStatus {
    /// Returns the exit code for this result
    fn exit_code(&self) -> ExitCode;
}

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
    ///
    /// # Errors
    /// Returns an error if serialization fails.
    fn render_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Render in the specified output mode
    ///
    /// For JSON mode, serialization errors are reported in the output string
    /// rather than silently returning an empty object.
    fn render(&self, mode: OutputMode) -> String {
        match mode {
            OutputMode::Human => self.render_human(),
            OutputMode::Json => self
                .render_json()
                .unwrap_or_else(|e| format!("{{\"error\": \"serialization failed: {e}\"}}")),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_exit_code_values() {
        assert_eq!(u8::from(ExitCode::Success), 0);
        assert_eq!(u8::from(ExitCode::IssuesFound), 1);
        assert_eq!(u8::from(ExitCode::UsageError), 2);
        assert_eq!(u8::from(ExitCode::ConfigError), 3);
        assert_eq!(u8::from(ExitCode::InputError), 4);
        assert_eq!(u8::from(ExitCode::GitError), 5);
        assert_eq!(u8::from(ExitCode::NetworkError), 6);
        assert_eq!(u8::from(ExitCode::ApiError), 7);
        assert_eq!(u8::from(ExitCode::ParseError), 8);
        assert_eq!(u8::from(ExitCode::InternalError), 9);
        assert_eq!(u8::from(ExitCode::TokenLimitError), 10);
    }

    #[test]
    fn test_exit_code_to_i32() {
        assert_eq!(i32::from(ExitCode::Success), 0);
        assert_eq!(i32::from(ExitCode::IssuesFound), 1);
        assert_eq!(i32::from(ExitCode::UsageError), 2);
        assert_eq!(i32::from(ExitCode::ConfigError), 3);
        assert_eq!(i32::from(ExitCode::InputError), 4);
        assert_eq!(i32::from(ExitCode::GitError), 5);
        assert_eq!(i32::from(ExitCode::NetworkError), 6);
        assert_eq!(i32::from(ExitCode::ApiError), 7);
        assert_eq!(i32::from(ExitCode::ParseError), 8);
        assert_eq!(i32::from(ExitCode::InternalError), 9);
        assert_eq!(i32::from(ExitCode::TokenLimitError), 10);
    }
}
