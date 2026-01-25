//! Output formatting for CLI commands
//!
//! Provides structured output types that can be rendered as either
//! human-readable text or JSON for scripting/automation.

mod commit;
mod config;
mod derive;
mod dry_run;
pub mod error;
mod explain;
mod fix;
mod review;

pub use commit::CommitResult;
pub use config::{ConfigEntry, ConfigResult};
pub use derive::DeriveResult;
pub use dry_run::{ChunkPlan, DryRunResult};
pub use error::{ErrorResponse, TypedError};
pub use explain::ExplainResult;
pub use fix::{Fix, FixResult};
pub use review::{ReviewIssue, ReviewResult};

use serde::Serialize;

/// Current schema version for JSON output
pub const SCHEMA_VERSION: &str = "1";

/// Exit codes for CLI commands
///
/// Simplified 5-category system for reliable automation:
/// - 0: Success (command completed, no issues)
/// - 1: Issues found (review/fix found problems to report)
/// - 2: User error (bad args, config, input - user can fix)
/// - 3: Transient error (network, rate limit - retry may help)
/// - 4: Permanent error (auth, parse, internal - retry won't help)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExitCode {
    /// Command completed successfully with no issues
    Success = 0,
    /// Command completed but found issues (e.g., review found problems)
    IssuesFound = 1,
    /// User-fixable errors: bad arguments, config, missing files
    UserError = 2,
    /// Transient errors: network issues, rate limits, timeouts (retry may help)
    TransientError = 3,
    /// Permanent errors: auth failures, parse errors, internal bugs (retry won't help)
    PermanentError = 4,
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

/// Metadata included with all JSON output
///
/// Provides consistent context for automation and debugging.
#[derive(Debug, Clone, Serialize)]
pub struct OutputMeta {
    /// Schema version for output format compatibility
    pub schema_version: &'static str,
    /// Model used for generation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Provider used (ollama, openai)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// Total tokens used across all calls
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_used: Option<usize>,
    /// Total duration in milliseconds
    pub duration_ms: u64,
    /// Number of chunks processed (1 for non-chunked)
    pub chunks: usize,
}

impl OutputMeta {
    /// Create metadata with just schema version (for errors)
    #[must_use]
    pub fn minimal() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            model: None,
            provider: None,
            tokens_used: None,
            duration_ms: 0,
            chunks: 0,
        }
    }

    /// Create full metadata for successful operations
    #[must_use]
    pub fn new(
        model: impl Into<String>,
        provider: impl Into<String>,
        duration_ms: u64,
        chunks: usize,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            model: Some(model.into()),
            provider: Some(provider.into()),
            tokens_used: None,
            duration_ms,
            chunks,
        }
    }

    /// Add token usage information
    #[must_use]
    pub fn with_tokens(mut self, tokens: usize) -> Self {
        self.tokens_used = Some(tokens);
        self
    }
}

impl Default for OutputMeta {
    fn default() -> Self {
        Self::minimal()
    }
}

/// Wrapper that adds `_meta` field to any serializable result
///
/// Uses `#[serde(flatten)]` to merge the result fields at the top level.
#[derive(Debug, Clone, Serialize)]
pub struct WithMeta<T: Serialize> {
    /// Metadata about the operation
    #[serde(rename = "_meta")]
    pub meta: OutputMeta,
    /// The actual result (flattened into parent object)
    #[serde(flatten)]
    pub result: T,
}

impl<T: Serialize> WithMeta<T> {
    /// Wrap a result with metadata
    #[must_use]
    pub fn new(result: T, meta: OutputMeta) -> Self {
        Self { meta, result }
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
        assert_eq!(u8::from(ExitCode::UserError), 2);
        assert_eq!(u8::from(ExitCode::TransientError), 3);
        assert_eq!(u8::from(ExitCode::PermanentError), 4);
    }

    #[test]
    fn test_exit_code_to_i32() {
        assert_eq!(i32::from(ExitCode::Success), 0);
        assert_eq!(i32::from(ExitCode::IssuesFound), 1);
        assert_eq!(i32::from(ExitCode::UserError), 2);
        assert_eq!(i32::from(ExitCode::TransientError), 3);
        assert_eq!(i32::from(ExitCode::PermanentError), 4);
    }

    #[test]
    fn test_output_meta_minimal() {
        let meta = OutputMeta::minimal();
        assert_eq!(meta.schema_version, SCHEMA_VERSION);
        assert!(meta.model.is_none());
        assert!(meta.provider.is_none());
        assert!(meta.tokens_used.is_none());
        assert_eq!(meta.duration_ms, 0);
        assert_eq!(meta.chunks, 0);
    }

    #[test]
    fn test_output_meta_new() {
        let meta = OutputMeta::new("gpt-4", "openai", 1500, 3);
        assert_eq!(meta.schema_version, SCHEMA_VERSION);
        assert_eq!(meta.model, Some("gpt-4".to_string()));
        assert_eq!(meta.provider, Some("openai".to_string()));
        assert_eq!(meta.duration_ms, 1500);
        assert_eq!(meta.chunks, 3);
    }

    #[test]
    fn test_output_meta_with_tokens() {
        let meta = OutputMeta::new("model", "provider", 100, 1).with_tokens(500);
        assert_eq!(meta.tokens_used, Some(500));
    }

    #[test]
    fn test_with_meta_serialization() {
        #[derive(Debug, Serialize)]
        struct TestResult {
            value: i32,
            name: String,
        }

        let result = TestResult {
            value: 42,
            name: "test".to_string(),
        };
        let meta = OutputMeta::new("model", "provider", 100, 1);
        let with_meta = WithMeta::new(result, meta);

        let json = serde_json::to_value(&with_meta).unwrap();

        // Check that _meta is present
        assert!(json.get("_meta").is_some());
        assert_eq!(json["_meta"]["schema_version"], SCHEMA_VERSION);

        // Check that result fields are flattened (not nested)
        assert_eq!(json["value"], 42);
        assert_eq!(json["name"], "test");
    }

    #[test]
    fn test_with_meta_minimal_meta() {
        #[derive(Debug, Serialize)]
        struct SimpleResult {
            ok: bool,
        }

        let result = SimpleResult { ok: true };
        let with_meta = WithMeta::new(result, OutputMeta::minimal());

        let json = serde_json::to_value(&with_meta).unwrap();

        // Minimal meta should not include optional fields
        assert_eq!(json["_meta"]["schema_version"], SCHEMA_VERSION);
        assert!(json["_meta"].get("model").is_none());
        assert!(json["_meta"].get("provider").is_none());
        assert!(json["_meta"].get("tokens_used").is_none());
    }
}
