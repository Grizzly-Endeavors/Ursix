mod chunk;
mod cli;
mod commands;
mod config;
mod context;
mod input;
mod json_repair;
mod llm;
mod output;
mod parsers;
mod pipeline;
mod prompts;
mod rules;
mod tokens;

use output::{ErrorResponse, ExitCode, ToExitCode, TypedError};
use tracing_subscriber::EnvFilter;

/// Convert an anyhow error to a [`TypedError`] for JSON output
fn to_typed_error(error: &anyhow::Error) -> TypedError {
    // Try downcasting to known error types in order of specificity
    if let Some(e) = error.downcast_ref::<cli::CliError>() {
        return cli_error_to_typed(e);
    }
    if let Some(e) = error.downcast_ref::<pipeline::PipelineError>() {
        return pipeline_error_to_typed(e);
    }
    if let Some(e) = error.downcast_ref::<llm::LlmError>() {
        return llm_error_to_typed(e);
    }

    // Check error chain for known types
    for cause in error.chain().skip(1) {
        if let Some(e) = cause.downcast_ref::<cli::CliError>() {
            return cli_error_to_typed(e);
        }
        if let Some(e) = cause.downcast_ref::<pipeline::PipelineError>() {
            return pipeline_error_to_typed(e);
        }
        if let Some(e) = cause.downcast_ref::<llm::LlmError>() {
            return llm_error_to_typed(e);
        }
    }

    // Fallback to internal error with full error chain
    TypedError::internal(format!("{error:#}"))
}

fn cli_error_to_typed(e: &cli::CliError) -> TypedError {
    match e {
        cli::CliError::Config(inner) => TypedError::ConfigError {
            message: inner.to_string(),
            path: None,
        },
        cli::CliError::Input(inner) => TypedError::InputError {
            message: inner.to_string(),
            path: None,
        },
        cli::CliError::Git(inner) => TypedError::GitError {
            message: inner.to_string(),
            command: None,
        },
        cli::CliError::Parse(inner) => TypedError::ParseError {
            message: inner.to_string(),
            context: None,
        },
        cli::CliError::Pipeline(pe) => pipeline_error_to_typed(pe),
    }
}

fn pipeline_error_to_typed(e: &pipeline::PipelineError) -> TypedError {
    match e {
        pipeline::PipelineError::Llm(le) => llm_error_to_typed(le),
        pipeline::PipelineError::TokenLimit(msg) => TypedError::TokenLimitError {
            message: msg.clone(),
            tokens: None,
            limit: None,
        },
    }
}

fn llm_error_to_typed(e: &llm::LlmError) -> TypedError {
    match e {
        llm::LlmError::Request(re) => TypedError::NetworkError {
            message: re.to_string(),
            url: re.url().map(ToString::to_string),
            retryable: true,
        },
        llm::LlmError::Parse(msg) => TypedError::ParseError {
            message: msg.clone(),
            context: None,
        },
        llm::LlmError::Api(msg) => {
            let retryable = e.is_retryable();
            TypedError::ApiError {
                message: msg.clone(),
                provider: None,
                status_code: None,
                retryable,
            }
        }
        llm::LlmError::Timeout(secs) => TypedError::NetworkError {
            message: format!("request timed out after {secs} seconds"),
            url: None,
            retryable: true,
        },
    }
}

/// Extract an appropriate exit code from an anyhow error by downcasting
fn extract_exit_code(error: &anyhow::Error) -> ExitCode {
    // Try downcasting to known error types in order of specificity
    if let Some(e) = error.downcast_ref::<cli::CliError>() {
        return e.to_exit_code();
    }
    if let Some(e) = error.downcast_ref::<pipeline::PipelineError>() {
        return e.to_exit_code();
    }
    if let Some(e) = error.downcast_ref::<llm::LlmError>() {
        return e.to_exit_code();
    }

    // Check error chain for known types
    for cause in error.chain().skip(1) {
        if let Some(e) = cause.downcast_ref::<cli::CliError>() {
            return e.to_exit_code();
        }
        if let Some(e) = cause.downcast_ref::<pipeline::PipelineError>() {
            return e.to_exit_code();
        }
        if let Some(e) = cause.downcast_ref::<llm::LlmError>() {
            return e.to_exit_code();
        }
    }

    ExitCode::PermanentError
}

#[tokio::main]
async fn main() {
    // Initialize tracing with RUST_LOG env filter (e.g., RUST_LOG=debug)
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let exit_code = match cli::run().await {
        Ok(code) => i32::from(code),
        Err(e) => {
            // Build typed error for structured JSON output
            let typed_error = to_typed_error(&e);
            let response = ErrorResponse::new(typed_error);

            // Output JSON error to stderr - keeps stdout clean for piping
            // Scripts can capture with 2>error.json
            match response.to_json() {
                Ok(json) => eprintln!("{json}"),
                Err(json_err) => {
                    // Fallback to text if JSON serialization fails (should never happen)
                    eprintln!("Error: {e:#}");
                    eprintln!("(JSON serialization failed: {json_err})");
                }
            }

            i32::from(extract_exit_code(&e))
        }
    };
    std::process::exit(exit_code);
}
