//! Typed error system for structured JSON error output
//!
//! Provides a unified error type that serializes to a flat JSON structure
//! with a `code` field for programmatic error handling.

use std::fmt;

use serde::Serialize;

use super::{ExitCode, OutputMeta};

/// Typed error variants with structured JSON output
///
/// Uses `#[serde(tag = "code")]` for flat JSON structure where the
/// discriminant becomes a `code` field alongside the error data.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub(crate) enum TypedError {
    /// Invalid CLI arguments
    InvalidArguments {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        argument: Option<String>,
    },

    /// Configuration file or settings error
    ConfigError {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
    },

    /// Input file or stdin error
    InputError {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
    },

    /// Git operation failed
    GitError {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        command: Option<String>,
    },

    /// Network/HTTP request failed (retryable)
    NetworkError {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        url: Option<String>,
        /// Whether this error is likely transient
        retryable: bool,
    },

    /// LLM API error
    ApiError {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status_code: Option<u16>,
        /// Whether this error is likely transient (rate limit, overload)
        retryable: bool,
    },

    /// Failed to parse LLM response
    ParseError {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        context: Option<String>,
    },

    /// Input exceeds token limit
    TokenLimitError {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        tokens: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        limit: Option<usize>,
    },

    /// Internal/unexpected error
    InternalError { message: String },
}

impl TypedError {
    /// Returns the exit code category for this error
    #[must_use]
    pub(crate) fn exit_code(&self) -> ExitCode {
        match self {
            // User-fixable errors (exit 2)
            Self::InvalidArguments { .. }
            | Self::ConfigError { .. }
            | Self::InputError { .. }
            | Self::GitError { .. }
            | Self::TokenLimitError { .. } => ExitCode::UserError,

            // Transient errors (exit 3) - check retryable flag
            Self::NetworkError {
                retryable: true, ..
            }
            | Self::ApiError {
                retryable: true, ..
            } => ExitCode::TransientError,

            // Permanent errors (exit 4)
            Self::NetworkError {
                retryable: false, ..
            }
            | Self::ApiError {
                retryable: false, ..
            }
            | Self::ParseError { .. }
            | Self::InternalError { .. } => ExitCode::PermanentError,
        }
    }

    /// Returns whether this error is retryable
    #[must_use]
    pub(crate) fn is_retryable(&self) -> bool {
        matches!(self.exit_code(), ExitCode::TransientError)
    }

    // Convenience constructors

    /// Create an invalid arguments error
    #[must_use]
    pub(crate) fn invalid_arguments(message: impl Into<String>) -> Self {
        Self::InvalidArguments {
            message: message.into(),
            argument: None,
        }
    }

    /// Create a config error
    #[must_use]
    pub(crate) fn config(message: impl Into<String>) -> Self {
        Self::ConfigError {
            message: message.into(),
            path: None,
        }
    }

    /// Create an input error
    #[must_use]
    pub(crate) fn input(message: impl Into<String>) -> Self {
        Self::InputError {
            message: message.into(),
            path: None,
        }
    }

    /// Create a git error
    #[must_use]
    pub(crate) fn git(message: impl Into<String>) -> Self {
        Self::GitError {
            message: message.into(),
            command: None,
        }
    }

    /// Create a network error (defaults to retryable)
    #[must_use]
    pub(crate) fn network(message: impl Into<String>) -> Self {
        Self::NetworkError {
            message: message.into(),
            url: None,
            retryable: true,
        }
    }

    /// Create an API error
    #[must_use]
    pub(crate) fn api(message: impl Into<String>, retryable: bool) -> Self {
        Self::ApiError {
            message: message.into(),
            provider: None,
            status_code: None,
            retryable,
        }
    }

    /// Create a parse error
    #[must_use]
    pub(crate) fn parse(message: impl Into<String>) -> Self {
        Self::ParseError {
            message: message.into(),
            context: None,
        }
    }

    /// Create a token limit error
    #[must_use]
    pub(crate) fn token_limit(message: impl Into<String>) -> Self {
        Self::TokenLimitError {
            message: message.into(),
            tokens: None,
            limit: None,
        }
    }

    /// Create an internal error
    #[must_use]
    pub(crate) fn internal(message: impl Into<String>) -> Self {
        Self::InternalError {
            message: message.into(),
        }
    }
}

impl fmt::Display for TypedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidArguments { message, .. }
            | Self::ConfigError { message, .. }
            | Self::InputError { message, .. }
            | Self::GitError { message, .. }
            | Self::NetworkError { message, .. }
            | Self::ApiError { message, .. }
            | Self::ParseError { message, .. }
            | Self::TokenLimitError { message, .. }
            | Self::InternalError { message } => message,
        };
        write!(f, "{message}")
    }
}

impl std::error::Error for TypedError {}

/// Structured error response for JSON output
///
/// Contains metadata and the typed error, serialized to stderr.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ErrorResponse {
    /// Metadata (minimal for errors)
    #[serde(rename = "_meta")]
    pub meta: OutputMeta,
    /// The error details
    pub error: TypedError,
}

impl ErrorResponse {
    /// Create a new error response with minimal metadata
    #[must_use]
    pub(crate) fn new(error: TypedError) -> Self {
        Self {
            meta: OutputMeta::minimal(),
            error,
        }
    }

    /// Create an error response with full metadata
    #[must_use]
    pub(crate) fn with_meta(error: TypedError, meta: OutputMeta) -> Self {
        Self { meta, error }
    }

    /// Get the exit code for this error
    #[must_use]
    pub(crate) fn exit_code(&self) -> ExitCode {
        self.error.exit_code()
    }

    /// Render as JSON string
    ///
    /// # Errors
    /// Returns error if JSON serialization fails (should not happen).
    pub(crate) fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code uses unwrap for clarity")]
mod tests {
    use super::*;

    #[test]
    fn test_typed_error_exit_codes() {
        // User errors (2)
        assert_eq!(
            TypedError::invalid_arguments("bad").exit_code(),
            ExitCode::UserError
        );
        assert_eq!(
            TypedError::config("bad config").exit_code(),
            ExitCode::UserError
        );
        assert_eq!(
            TypedError::input("missing file").exit_code(),
            ExitCode::UserError
        );
        assert_eq!(
            TypedError::git("not a repo").exit_code(),
            ExitCode::UserError
        );
        assert_eq!(
            TypedError::token_limit("too big").exit_code(),
            ExitCode::UserError
        );

        // Transient errors (3)
        assert_eq!(
            TypedError::network("timeout").exit_code(),
            ExitCode::TransientError
        );
        assert_eq!(
            TypedError::api("rate limited", true).exit_code(),
            ExitCode::TransientError
        );

        // Permanent errors (4)
        assert_eq!(
            TypedError::api("invalid key", false).exit_code(),
            ExitCode::PermanentError
        );
        assert_eq!(
            TypedError::parse("invalid json").exit_code(),
            ExitCode::PermanentError
        );
        assert_eq!(
            TypedError::internal("bug").exit_code(),
            ExitCode::PermanentError
        );
    }

    #[test]
    fn test_typed_error_is_retryable() {
        assert!(TypedError::network("timeout").is_retryable());
        assert!(TypedError::api("rate limited", true).is_retryable());

        assert!(!TypedError::api("invalid key", false).is_retryable());
        assert!(!TypedError::parse("bad json").is_retryable());
        assert!(!TypedError::config("missing").is_retryable());
    }

    #[test]
    fn test_typed_error_json_has_code_field() {
        let error = TypedError::NetworkError {
            message: "connection failed".to_string(),
            url: Some("http://example.com".to_string()),
            retryable: true,
        };

        let json = serde_json::to_value(&error).unwrap();

        // Should have flat structure with code field
        assert_eq!(json.get("code"), Some(&serde_json::json!("network_error")));
        assert_eq!(
            json.get("message"),
            Some(&serde_json::json!("connection failed"))
        );
        assert_eq!(
            json.get("url"),
            Some(&serde_json::json!("http://example.com"))
        );
        assert_eq!(json.get("retryable"), Some(&serde_json::json!(true)));
    }

    #[test]
    fn test_typed_error_json_all_variants() {
        let variants = vec![
            (TypedError::invalid_arguments("test"), "invalid_arguments"),
            (TypedError::config("test"), "config_error"),
            (TypedError::input("test"), "input_error"),
            (TypedError::git("test"), "git_error"),
            (TypedError::network("test"), "network_error"),
            (TypedError::api("test", false), "api_error"),
            (TypedError::parse("test"), "parse_error"),
            (TypedError::token_limit("test"), "token_limit_error"),
            (TypedError::internal("test"), "internal_error"),
        ];

        for (error, expected_code) in variants {
            let json = serde_json::to_value(&error).unwrap();
            assert_eq!(
                json.get("code"),
                Some(&serde_json::json!(expected_code)),
                "wrong code for {expected_code:?}"
            );
        }
    }

    #[test]
    fn test_error_response_structure() {
        let error = TypedError::api("rate limited", true);
        let response = ErrorResponse::new(error);

        let json = serde_json::to_value(&response).unwrap();

        // Should have _meta and error at top level
        assert!(json.get("_meta").is_some());
        assert!(json.get("error").is_some());

        // Meta should have schema_version
        assert_eq!(
            json.get("_meta").and_then(|m| m.get("schema_version")),
            Some(&serde_json::json!(super::super::SCHEMA_VERSION))
        );

        // Error should have code field
        assert_eq!(
            json.get("error").and_then(|e| e.get("code")),
            Some(&serde_json::json!("api_error"))
        );
        assert_eq!(
            json.get("error").and_then(|e| e.get("retryable")),
            Some(&serde_json::json!(true))
        );
    }

    #[test]
    fn test_error_response_exit_code() {
        let response_network = ErrorResponse::new(TypedError::network("timeout"));
        assert_eq!(response_network.exit_code(), ExitCode::TransientError);

        let response_parse = ErrorResponse::new(TypedError::parse("bad json"));
        assert_eq!(response_parse.exit_code(), ExitCode::PermanentError);
    }

    #[test]
    fn test_typed_error_display() {
        let error = TypedError::NetworkError {
            message: "connection refused".to_string(),
            url: Some("http://localhost:8080".to_string()),
            retryable: true,
        };

        assert_eq!(format!("{error}"), "connection refused");
    }

    #[test]
    fn test_error_response_to_json() {
        let error = TypedError::config("missing config file");
        let response = ErrorResponse::new(error);

        let json_str = response.to_json().unwrap();

        // Should be valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert!(parsed.get("_meta").is_some());
        assert!(parsed.get("error").is_some());
    }
}
