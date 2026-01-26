//! Error conversion utilities
//!
//! This module provides functions to convert various error types into
//! typed error representations for structured JSON output.

use crate::cli::CliError;
use crate::llm::LlmError;
use crate::output::{ExitCode, ToExitCode, TypedError};
use crate::pipeline::PipelineError;

/// Convert an anyhow error to a [`TypedError`] for JSON output
pub(crate) fn to_typed_error(error: &anyhow::Error) -> TypedError {
    // Try downcasting to known error types in order of specificity
    if let Some(e) = error.downcast_ref::<CliError>() {
        return cli_error_to_typed(e);
    }
    if let Some(e) = error.downcast_ref::<PipelineError>() {
        return pipeline_error_to_typed(e);
    }
    if let Some(e) = error.downcast_ref::<LlmError>() {
        return llm_error_to_typed(e);
    }

    // Check error chain for known types
    for cause in error.chain().skip(1) {
        if let Some(e) = cause.downcast_ref::<CliError>() {
            return cli_error_to_typed(e);
        }
        if let Some(e) = cause.downcast_ref::<PipelineError>() {
            return pipeline_error_to_typed(e);
        }
        if let Some(e) = cause.downcast_ref::<LlmError>() {
            return llm_error_to_typed(e);
        }
    }

    // Fallback to internal error with full error chain
    TypedError::internal(format!("{error:#}"))
}

fn cli_error_to_typed(e: &CliError) -> TypedError {
    match e {
        CliError::Config(inner) => TypedError::ConfigError {
            message: inner.to_string(),
            path: None,
        },
        CliError::Input(inner) => TypedError::InputError {
            message: inner.to_string(),
            path: None,
        },
        CliError::Git(inner) => TypedError::GitError {
            message: inner.to_string(),
            command: None,
        },
        CliError::Parse(inner) => TypedError::ParseError {
            message: inner.to_string(),
            context: None,
        },
        CliError::Pipeline(pe) => pipeline_error_to_typed(pe),
    }
}

fn pipeline_error_to_typed(e: &PipelineError) -> TypedError {
    match e {
        PipelineError::Llm(le) => llm_error_to_typed(le),
        PipelineError::TokenLimit(msg) => TypedError::TokenLimitError {
            message: msg.clone(),
            tokens: None,
            limit: None,
        },
    }
}

fn llm_error_to_typed(e: &LlmError) -> TypedError {
    match e {
        LlmError::Request(re) => TypedError::NetworkError {
            message: re.to_string(),
            url: re.url().map(ToString::to_string),
            retryable: true,
        },
        LlmError::Parse(msg) => TypedError::ParseError {
            message: msg.clone(),
            context: None,
        },
        LlmError::Api(msg) => {
            let retryable = e.is_retryable();
            TypedError::ApiError {
                message: msg.clone(),
                provider: None,
                status_code: None,
                retryable,
            }
        }
        LlmError::Timeout(secs) => TypedError::NetworkError {
            message: format!("request timed out after {secs} seconds"),
            url: None,
            retryable: true,
        },
    }
}

/// Extract an appropriate exit code from an anyhow error by downcasting
pub(crate) fn extract_exit_code(error: &anyhow::Error) -> ExitCode {
    // Try downcasting to known error types in order of specificity
    if let Some(e) = error.downcast_ref::<CliError>() {
        return e.to_exit_code();
    }
    if let Some(e) = error.downcast_ref::<PipelineError>() {
        return e.to_exit_code();
    }
    if let Some(e) = error.downcast_ref::<LlmError>() {
        return e.to_exit_code();
    }

    // Check error chain for known types
    for cause in error.chain().skip(1) {
        if let Some(e) = cause.downcast_ref::<CliError>() {
            return e.to_exit_code();
        }
        if let Some(e) = cause.downcast_ref::<PipelineError>() {
            return e.to_exit_code();
        }
        if let Some(e) = cause.downcast_ref::<LlmError>() {
            return e.to_exit_code();
        }
    }

    ExitCode::PermanentError
}
