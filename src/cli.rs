use anyhow::Result;
use clap::Parser;

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

    /// Initial prompt (if not provided, starts interactive mode)
    pub prompt: Option<String>,
}

pub async fn run() -> Result<()> {
    let args = Args::parse();

    println!("rust-code v{}", env!("CARGO_PKG_VERSION"));
    println!("Model: {}", args.model);
    println!("Ollama URL: {}", args.ollama_url);

    if let Some(prompt) = &args.prompt {
        println!("Prompt: {prompt}");
    } else {
        println!("Interactive mode (not yet implemented)");
    }

    Ok(())
}
