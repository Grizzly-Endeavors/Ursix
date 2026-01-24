mod chunk;
mod cli;
mod commands;
mod config;
mod context;
mod input;
mod llm;
mod output;
mod parsers;
mod pipeline;
mod prompts;
mod rules;
mod tokens;

use output::{ExitCode, ToExitCode};
use tracing_subscriber::EnvFilter;

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

    ExitCode::InternalError
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
            eprintln!("Error: {e:?}");
            i32::from(extract_exit_code(&e))
        }
    };
    std::process::exit(exit_code);
}
