use anyhow::{Context, Result};
use clap::Parser;

use crate::agent::Agent;
use crate::config::Config;
use crate::llm::ollama::OllamaClient;
use crate::tui::TuiApp;

#[derive(Parser, Debug)]
#[command(name = "rust-code")]
#[command(about = "An extensible agentic CLI tool for testing LLM capabilities")]
#[command(version)]
pub struct Args {
    /// Model to use (e.g., "llama3.2", "qwen2.5-coder")
    #[arg(short, long, default_value = "llama3.2")]
    pub model: String,

    /// Ollama API base URL
    #[arg(long, default_value = "http://localhost:11434")]
    pub ollama_url: String,

    /// Maximum agent turns before stopping
    #[arg(long, default_value = "50")]
    pub max_turns: usize,

    /// Initial prompt (if not provided, starts interactive TUI mode)
    pub prompt: Option<String>,
}

/// Run the CLI application
///
/// # Errors
/// Returns error if agent execution fails or if working directory cannot be determined
pub async fn run() -> Result<()> {
    let args = Args::parse();

    let working_dir = std::env::current_dir().context("failed to get current directory")?;

    let config = Config {
        model: args.model.clone(),
        ollama_url: args.ollama_url.clone(),
        working_dir,
        max_turns: args.max_turns,
    };

    if let Some(prompt) = args.prompt {
        let client = OllamaClient::new(&config.ollama_url, &config.model);
        let agent = Agent::new(config, client);
        let result = agent.run(&prompt).await?;
        println!("{result}");
    } else {
        let mut app = TuiApp::new(config)?;
        app.run().await?;
    }

    Ok(())
}
