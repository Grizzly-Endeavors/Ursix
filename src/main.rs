mod agent;
mod cli;
mod config;
mod context;
mod llm;
mod output;
mod pipeline;
mod prompts;
mod rules;
mod tools;

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
            eprintln!("Error: {e:?}");
            2 // ExitCode::Error
        }
    };
    std::process::exit(exit_code);
}
