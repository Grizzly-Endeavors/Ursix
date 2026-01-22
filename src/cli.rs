use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use crate::agent::Agent;
use crate::config::{Config, Provider};
use crate::llm::LlmClient;
use crate::llm::ollama::OllamaClient;
use crate::llm::openai::OpenAiClient;
use crate::output::{
    AskResult, CommandOutput, ConfigEntry, ConfigResult, ModelsResult, OutputMode,
};
use crate::prompts;

#[derive(Parser, Debug)]
#[command(name = "ur")]
#[command(about = "Ursus.rs - An extensible agentic CLI for LLM-powered development")]
#[command(version)]
pub struct Cli {
    /// Output as JSON for scripting
    #[arg(long, global = true)]
    pub json: bool,

    /// LLM provider (ollama, openai)
    #[arg(long, global = true)]
    pub provider: Option<Provider>,

    /// Model to use (overrides config)
    #[arg(short, long, global = true)]
    pub model: Option<String>,

    /// Ollama API base URL (overrides config)
    #[arg(long, global = true)]
    pub ollama_url: Option<String>,

    /// OpenAI-compatible API base URL (overrides config)
    #[arg(long, global = true)]
    pub openai_url: Option<String>,

    /// API key for OpenAI-compatible endpoints (overrides environment variable)
    #[arg(long, global = true, env = "URSUS_OPENAI_API_KEY")]
    pub openai_api_key: Option<String>,

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

    // Determine output mode
    let output_mode = if cli.json {
        OutputMode::Json
    } else {
        OutputMode::Human
    };

    // Load config from files and env vars, then apply CLI overrides
    let mut config = Config::load().context("failed to load configuration")?;
    config.working_dir = working_dir;

    // CLI flags override everything
    if let Some(provider) = cli.provider {
        config.provider = provider;
    }
    if let Some(ref model) = cli.model {
        config.model.clone_from(model);
    }
    if let Some(ref url) = cli.ollama_url {
        config.ollama_url.clone_from(url);
    }
    if let Some(ref url) = cli.openai_url {
        config.openai_url.clone_from(url);
    }
    if let Some(ref key) = cli.openai_api_key {
        config.openai_api_key = Some(key.clone());
    }
    if let Some(turns) = cli.max_turns {
        config.max_turns = turns;
    }

    match cli.command {
        Command::Ask { prompt } => cmd_ask(&config, prompt, output_mode).await,
        Command::Explain { target, context } => {
            cmd_explain(&config, &target, context, output_mode).await
        }
        Command::Review { diff, files } => cmd_review(&config, diff, files, output_mode).await,
        Command::Fix {
            target,
            lint,
            apply,
        } => cmd_fix(&config, &target, lint, apply, output_mode).await,
        Command::Commit { body, style } => cmd_commit(&config, body, &style, output_mode).await,
        Command::Config { key, value, list } => cmd_config(&config, key, value, list, output_mode),
        Command::Models => cmd_models(&config, output_mode).await,
    }
}

/// Check if a URL points to localhost
fn is_local_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.contains("localhost") || lower.contains("127.0.0.1") || lower.contains("[::1]")
}

/// Create an [`OpenAiClient`] from config, handling API key requirements
fn create_openai_client(config: &Config) -> Result<OpenAiClient> {
    if let Some(ref key) = config.openai_api_key {
        Ok(OpenAiClient::with_api_key(
            &config.openai_url,
            &config.model,
            key,
        ))
    } else if is_local_url(&config.openai_url) {
        Ok(OpenAiClient::new(&config.openai_url, &config.model))
    } else {
        anyhow::bail!(
            "OpenAI API key required for remote endpoints. \
             Set OPENAI_API_KEY environment variable or use --openai-api-key flag."
        )
    }
}

/// Run the agent with the appropriate provider
async fn run_agent(config: &Config, system_prompt: &str, user_prompt: &str) -> Result<String> {
    match config.provider {
        Provider::Ollama => {
            let client = OllamaClient::new(&config.ollama_url, &config.model);
            run_agent_with_client(config, client, system_prompt, user_prompt).await
        }
        Provider::OpenAi => {
            let client = create_openai_client(config)?;
            run_agent_with_client(config, client, system_prompt, user_prompt).await
        }
    }
}

/// Run the agent with a specific LLM client
async fn run_agent_with_client<C: LlmClient>(
    config: &Config,
    client: C,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String> {
    let agent = Agent::new(config.clone(), client);
    agent
        .run_with_prompt(system_prompt, user_prompt)
        .await
        .map_err(Into::into)
}

async fn cmd_ask(config: &Config, prompt: Vec<String>, output_mode: OutputMode) -> Result<()> {
    let prompt = prompt.join(" ");
    let response = run_agent(config, prompts::ASK_PROMPT, &prompt).await?;

    let result = AskResult { response, turns: 1 };
    println!("{}", result.render(output_mode));
    Ok(())
}

async fn cmd_explain(
    config: &Config,
    target: &str,
    _context: Option<usize>,
    output_mode: OutputMode,
) -> Result<()> {
    let prompt = format!("Explain the code in: {target}");
    let response = run_agent(config, prompts::EXPLAIN_PROMPT, &prompt).await?;

    let result = AskResult { response, turns: 1 };
    println!("{}", result.render(output_mode));
    Ok(())
}

async fn cmd_review(
    config: &Config,
    diff: Option<String>,
    files: Vec<String>,
    output_mode: OutputMode,
) -> Result<()> {
    let prompt = if let Some(ref d) = diff {
        format!("Review the changes in git diff {d}")
    } else if !files.is_empty() {
        format!("Review the code in: {}", files.join(", "))
    } else {
        "Review the staged changes (use git diff --cached)".to_string()
    };
    let response = run_agent(config, prompts::REVIEW_PROMPT, &prompt).await?;

    let result = AskResult { response, turns: 1 };
    println!("{}", result.render(output_mode));
    Ok(())
}

async fn cmd_fix(
    config: &Config,
    target: &str,
    lint: bool,
    _apply: bool,
    output_mode: OutputMode,
) -> Result<()> {
    let prompt = if lint {
        format!("Fix lint/clippy issues in: {target}. Run clippy first to identify issues.")
    } else {
        format!("Fix issues in: {target}")
    };
    let response = run_agent(config, prompts::FIX_PROMPT, &prompt).await?;

    let result = AskResult { response, turns: 1 };
    println!("{}", result.render(output_mode));
    Ok(())
}

async fn cmd_commit(
    config: &Config,
    body: bool,
    style: &str,
    output_mode: OutputMode,
) -> Result<()> {
    let prompt = format!(
        "Generate a {} commit message for the staged changes{}.",
        style,
        if body {
            " with a detailed body explaining the changes"
        } else {
            ""
        }
    );
    let response = run_agent(config, prompts::COMMIT_PROMPT, &prompt).await?;

    let result = AskResult { response, turns: 1 };
    println!("{}", result.render(output_mode));
    Ok(())
}

#[allow(clippy::unnecessary_wraps)] // Will return errors when config set is implemented
fn cmd_config(
    config: &Config,
    key: Option<String>,
    value: Option<String>,
    list: bool,
    output_mode: OutputMode,
) -> Result<()> {
    let api_key_display = if config.openai_api_key.is_some() {
        "[set]".to_string()
    } else {
        "[not set]".to_string()
    };

    if list {
        let result = ConfigResult {
            entries: vec![
                ConfigEntry {
                    key: "provider".to_string(),
                    value: config.provider.to_string(),
                },
                ConfigEntry {
                    key: "model".to_string(),
                    value: config.model.clone(),
                },
                ConfigEntry {
                    key: "ollama_url".to_string(),
                    value: config.ollama_url.clone(),
                },
                ConfigEntry {
                    key: "openai_url".to_string(),
                    value: config.openai_url.clone(),
                },
                ConfigEntry {
                    key: "openai_api_key".to_string(),
                    value: api_key_display.clone(),
                },
                ConfigEntry {
                    key: "max_turns".to_string(),
                    value: config.max_turns.to_string(),
                },
                ConfigEntry {
                    key: "working_dir".to_string(),
                    value: config.working_dir.display().to_string(),
                },
            ],
        };
        println!("{}", result.render(output_mode));
    } else if let Some(k) = key {
        if let Some(v) = value {
            println!("Setting {k} = {v} (not yet implemented)");
        } else {
            let val = match k.as_str() {
                "provider" => config.provider.to_string(),
                "model" => config.model.clone(),
                "ollama_url" => config.ollama_url.clone(),
                "openai_url" => config.openai_url.clone(),
                "openai_api_key" => api_key_display,
                "max_turns" => config.max_turns.to_string(),
                "working_dir" => config.working_dir.display().to_string(),
                _ => format!("Unknown config key: {k}"),
            };
            let result = ConfigResult {
                entries: vec![ConfigEntry { key: k, value: val }],
            };
            println!("{}", result.render(output_mode));
        }
    } else {
        println!("Usage: ur config <key> [value] or ur config --list");
    }
    Ok(())
}

async fn cmd_models(config: &Config, output_mode: OutputMode) -> Result<()> {
    match config.provider {
        Provider::Ollama => {
            let client = OllamaClient::new(&config.ollama_url, &config.model);
            match client.list_models().await {
                Ok(models) => {
                    let result = ModelsResult { models };
                    println!("{}", result.render(output_mode));
                }
                Err(e) => {
                    eprintln!("Failed to list models: {e}");
                }
            }
        }
        Provider::OpenAi => {
            // OpenAI's /v1/models endpoint requires authentication and returns
            // a different format. For now, inform the user to check OpenAI docs.
            println!(
                "Model listing is not available for OpenAI provider.\n\
                 See https://platform.openai.com/docs/models for available models."
            );
        }
    }
    Ok(())
}
