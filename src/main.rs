mod chunk;
mod cli;
mod commands;
mod config;
mod context;
mod error;
mod input;
mod json_repair;
mod llm;
mod output;
mod parsers;
mod pipeline;
mod prompts;
mod rules;
mod tokens;

use error::{extract_exit_code, to_typed_error};
use output::ErrorResponse;
use tracing_subscriber::EnvFilter;

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
                    // Fallback to text if JSON serialization fails
                    eprintln!("Error: {e:#}");
                    eprintln!("(JSON serialization failed: {json_err})");
                }
            }

            i32::from(extract_exit_code(&e))
        }
    };
    std::process::exit(exit_code);
}
