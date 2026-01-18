use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use crate::agent::Agent;
use crate::config::Config;
use crate::llm::ollama::OllamaClient;

#[derive(Parser, Debug)]
#[command(name = "ur")]
#[command(about = "Ursus.rs - An extensible agentic CLI for LLM-powered development")]
#[command(version)]
pub struct Cli {
    /// Output as JSON for scripting
    #[arg(long, global = true)]
    pub json: bool,

    /// Model to use (overrides config)
    #[arg(short, long, global = true)]
    pub model: Option<String>,

    /// Ollama API base URL (overrides config)
    #[arg(long, global = true)]
    pub ollama_url: Option<String>,

    /// Maximum agent turns before stopping
    #[arg(long, global = true)]
    pub max_turns: Option<usize>,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// General-purpose LLM query with tool access
    Ask {
        /// The prompt to send to the LLM
        #[arg(required = true)]
        prompt: Vec<String>,
    },

    /// Explain code, files, or concepts
    Explain {
        /// File path or concept to explain
        target: String,

        /// Include surrounding context lines
        #[arg(short, long)]
        context: Option<usize>,
    },

    /// Review code changes
    Review {
        /// Review specific git diff (e.g., HEAD~1, branch-name)
        #[arg(long)]
        diff: Option<String>,

        /// Review specific files
        files: Vec<String>,
    },

    /// Fix issues in code
    Fix {
        /// Target file or directory
        target: String,

        /// Fix lint/clippy issues
        #[arg(long)]
        lint: bool,

        /// Apply fixes automatically (without confirmation)
        #[arg(long)]
        apply: bool,
    },

    /// Generate commit message from staged changes
    Commit {
        /// Include body with detailed explanation
        #[arg(long)]
        body: bool,

        /// Commit style (conventional, simple)
        #[arg(long, default_value = "conventional")]
        style: String,
    },

    /// Manage configuration
    Config {
        /// Configuration key to get/set
        key: Option<String>,

        /// Value to set (if omitted, shows current value)
        value: Option<String>,

        /// List all configuration values
        #[arg(long)]
        list: bool,
    },

    /// List available Ollama models
    Models,
}

/// Run the CLI application
///
/// # Errors
/// Returns error if command execution fails or if working directory cannot be determined
pub async fn run() -> Result<()> {
    let cli = Cli::parse();

    let working_dir = std::env::current_dir().context("failed to get current directory")?;

    // Build config from defaults, then apply CLI overrides
    let config = Config {
        model: cli.model.clone().unwrap_or_else(|| "llama3.2".to_string()),
        ollama_url: cli
            .ollama_url
            .clone()
            .unwrap_or_else(|| "http://localhost:11434".to_string()),
        working_dir,
        max_turns: cli.max_turns.unwrap_or(50),
    };

    match cli.command {
        Command::Ask { prompt } => cmd_ask(&config, prompt).await,
        Command::Explain { target, context } => cmd_explain(&config, &target, context).await,
        Command::Review { diff, files } => cmd_review(&config, diff, files).await,
        Command::Fix {
            target,
            lint,
            apply,
        } => cmd_fix(&config, &target, lint, apply).await,
        Command::Commit { body, style } => cmd_commit(&config, body, &style).await,
        Command::Config { key, value, list } => cmd_config(&config, key, value, list),
        Command::Models => cmd_models(&config).await,
    }
}

async fn cmd_ask(config: &Config, prompt: Vec<String>) -> Result<()> {
    let prompt = prompt.join(" ");
    let client = OllamaClient::new(&config.ollama_url, &config.model);
    let agent = Agent::new(config.clone(), client);
    let result = agent.run(&prompt).await?;
    println!("{result}");
    Ok(())
}

async fn cmd_explain(config: &Config, target: &str, _context: Option<usize>) -> Result<()> {
    let prompt = format!("Explain the code in: {target}");
    let client = OllamaClient::new(&config.ollama_url, &config.model);
    let agent = Agent::new(config.clone(), client);
    let result = agent.run(&prompt).await?;
    println!("{result}");
    Ok(())
}

async fn cmd_review(config: &Config, diff: Option<String>, files: Vec<String>) -> Result<()> {
    let prompt = if let Some(ref d) = diff {
        format!("Review the changes in git diff {d}")
    } else if !files.is_empty() {
        format!("Review the code in: {}", files.join(", "))
    } else {
        "Review the staged changes".to_string()
    };
    let client = OllamaClient::new(&config.ollama_url, &config.model);
    let agent = Agent::new(config.clone(), client);
    let result = agent.run(&prompt).await?;
    println!("{result}");
    Ok(())
}

async fn cmd_fix(config: &Config, target: &str, lint: bool, _apply: bool) -> Result<()> {
    let prompt = if lint {
        format!("Fix lint/clippy issues in: {target}")
    } else {
        format!("Fix issues in: {target}")
    };
    let client = OllamaClient::new(&config.ollama_url, &config.model);
    let agent = Agent::new(config.clone(), client);
    let result = agent.run(&prompt).await?;
    println!("{result}");
    Ok(())
}

async fn cmd_commit(config: &Config, body: bool, style: &str) -> Result<()> {
    let prompt = format!(
        "Generate a {} commit message{}",
        style,
        if body { " with detailed body" } else { "" }
    );
    let client = OllamaClient::new(&config.ollama_url, &config.model);
    let agent = Agent::new(config.clone(), client);
    let result = agent.run(&prompt).await?;
    println!("{result}");
    Ok(())
}

#[allow(clippy::unnecessary_wraps)] // Will return errors when config set is implemented
fn cmd_config(
    config: &Config,
    key: Option<String>,
    value: Option<String>,
    list: bool,
) -> Result<()> {
    if list {
        println!("model = {}", config.model);
        println!("ollama_url = {}", config.ollama_url);
        println!("max_turns = {}", config.max_turns);
        println!("working_dir = {}", config.working_dir.display());
    } else if let Some(k) = key {
        if let Some(v) = value {
            println!("Setting {k} = {v} (not yet implemented)");
        } else {
            match k.as_str() {
                "model" => println!("{}", config.model),
                "ollama_url" => println!("{}", config.ollama_url),
                "max_turns" => println!("{}", config.max_turns),
                "working_dir" => println!("{}", config.working_dir.display()),
                _ => println!("Unknown config key: {k}"),
            }
        }
    } else {
        println!("Usage: ur config <key> [value] or ur config --list");
    }
    Ok(())
}

async fn cmd_models(config: &Config) -> Result<()> {
    let client = OllamaClient::new(&config.ollama_url, &config.model);
    match client.list_models().await {
        Ok(models) => {
            for model in models {
                println!("{model}");
            }
        }
        Err(e) => {
            eprintln!("Failed to list models: {e}");
        }
    }
    Ok(())
}
