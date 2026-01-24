//! CLI argument parsing and command dispatch
//!
//! This module defines the CLI structure using clap and dispatches to
//! command implementations in the `commands` module.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use thiserror::Error;

use crate::agent::{Agent, AgentError, AgentResult};
use crate::commands::{
    FixMode, FixOptions, ReviewOptions, cmd_commit, cmd_config, cmd_explain, cmd_fix, cmd_review,
};
use crate::config::{Config, Provider, TokenizerMode};
use crate::context::GatheredContext;
use crate::llm::LlmClient;
use crate::llm::ollama::OllamaClient;
use crate::llm::openai::OpenAiClient;
use crate::output::{ExitCode, OutputMode, ToExitCode};
use crate::pipeline::Pipeline;
use crate::pipeline::PipelineError;

#[derive(Parser, Debug)]
#[command(name = "usx")]
#[command(about = "Ursix - An extensible agentic CLI for LLM-powered development")]
#[command(version)]
pub struct Cli {
    /// Output as plain text instead of JSON (default: JSON)
    #[arg(long, global = true)]
    pub text: bool,

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
    #[arg(long, global = true, env = "URSIX_OPENAI_API_KEY")]
    pub openai_api_key: Option<String>,

    /// Maximum agent turns before stopping
    #[arg(long, global = true)]
    pub max_turns: Option<usize>,

    /// Enable chunked processing for large inputs (parallel file-by-file processing)
    #[arg(long, global = true)]
    pub chunk: bool,

    /// Maximum concurrent chunk executions (default: 4)
    #[arg(long, global = true, default_value = "4")]
    pub max_concurrency: usize,

    /// Tokenizer mode for token counting (heuristic or full)
    ///
    /// 'heuristic' (default): Fast character-based approximation
    /// 'full': Accurate `HuggingFace` tokenizer (has network/CPU overhead)
    #[arg(long, global = true)]
    pub tokenizer: Option<TokenizerMode>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Explain code, files, or concepts
    Explain {
        /// File path or concept to explain
        target: String,

        /// Use agentic mode for deep exploration (default: pipeline mode)
        #[arg(long)]
        agent: bool,
    },

    /// Review code changes
    Review {
        /// Review specific git diff (e.g., HEAD~1, branch-name)
        #[arg(long)]
        diff: Option<String>,

        /// Review specific files
        files: Vec<String>,

        /// Use agentic mode for thorough multi-file analysis (default: pipeline mode)
        #[arg(long)]
        agent: bool,

        /// Read input from a file (use - for stdin)
        #[arg(long, value_name = "FILE")]
        from: Option<PathBuf>,

        /// Checks to perform (e.g., style, security, performance)
        #[arg(long, value_delimiter = ',')]
        checks: Vec<String>,
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

        /// Use agentic mode with full tool access (default: pipeline mode)
        #[arg(long)]
        agent: bool,

        /// Read issues from a file (use - for stdin, e.g., from review JSON output)
        #[arg(long, value_name = "FILE")]
        from: Option<PathBuf>,
    },

    /// Generate commit message from staged changes
    Commit {
        /// Include body with detailed explanation
        #[arg(long)]
        body: bool,

        /// Commit style (conventional, simple)
        #[arg(long, default_value = "conventional")]
        style: String,

        /// Auto-execute git commit with the generated message
        #[arg(long)]
        execute: bool,
    },

    /// View configuration values (edit .ursix.toml to change settings)
    Config {
        /// Configuration key to display
        key: Option<String>,

        /// List all configuration values
        #[arg(long)]
        list: bool,
    },
}

/// CLI-specific errors with appropriate exit codes
#[derive(Debug, Error)]
pub enum CliError {
    #[error("{0}")]
    Config(#[source] anyhow::Error),

    #[error("{0}")]
    Input(#[source] anyhow::Error),

    #[error("{0}")]
    Git(#[source] anyhow::Error),

    #[error("{0}")]
    Parse(#[source] anyhow::Error),

    #[error("{0}")]
    Pipeline(#[from] PipelineError),

    #[error("{0}")]
    Agent(#[from] AgentError),
}

impl ToExitCode for CliError {
    fn to_exit_code(&self) -> ExitCode {
        match self {
            Self::Config(_) => ExitCode::ConfigError,
            Self::Input(_) => ExitCode::InputError,
            Self::Git(_) => ExitCode::GitError,
            Self::Parse(_) => ExitCode::ParseError,
            Self::Pipeline(e) => e.to_exit_code(),
            Self::Agent(e) => e.to_exit_code(),
        }
    }
}

/// Run the CLI application
///
/// # Errors
/// Returns error if command execution fails or if working directory cannot be determined
pub async fn run() -> Result<ExitCode> {
    let cli = Cli::parse();

    let working_dir = std::env::current_dir().context("failed to get current directory")?;

    // Determine output mode (JSON is default, --text for human-readable)
    let output_mode = if cli.text {
        OutputMode::Human
    } else {
        OutputMode::Json
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
    if let Some(mode) = cli.tokenizer {
        config.tokenizer_mode = mode;
    }

    match cli.command {
        Command::Explain { target, agent } => {
            cmd_explain(&config, &target, agent, output_mode, cli.chunk).await
        }
        Command::Review {
            diff,
            files,
            agent,
            from,
            checks,
        } => {
            let options = ReviewOptions {
                agent,
                chunk: cli.chunk,
                max_concurrency: cli.max_concurrency,
            };
            cmd_review(&config, diff, files, options, from, &checks, output_mode).await
        }
        Command::Fix {
            target,
            lint,
            apply,
            agent,
            from,
        } => {
            let options = FixOptions {
                lint,
                apply,
                mode: if agent {
                    FixMode::Agent
                } else {
                    FixMode::Pipeline
                },
                chunk: cli.chunk,
                max_concurrency: cli.max_concurrency,
            };
            cmd_fix(&config, &target, options, from, output_mode).await
        }
        Command::Commit {
            body,
            style,
            execute,
        } => cmd_commit(&config, body, &style, execute, output_mode, cli.chunk).await,
        Command::Config { key, list } => cmd_config(&config, key, list, output_mode),
    }
}

/// Check if a URL points to localhost
fn is_local_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.contains("localhost") || lower.contains("127.0.0.1") || lower.contains("[::1]")
}

/// Create an [`OpenAiClient`] from config, handling API key requirements
pub(crate) fn create_openai_client(config: &Config) -> Result<OpenAiClient> {
    if let Some(ref key) = config.openai_api_key {
        Ok(OpenAiClient::with_api_key(
            &config.openai_url,
            &config.model,
            key,
        ))
    } else if is_local_url(&config.openai_url) {
        Ok(OpenAiClient::new(&config.openai_url, &config.model))
    } else {
        Err(CliError::Config(anyhow::anyhow!(
            "OpenAI API key required for remote endpoints. \
             Set OPENAI_API_KEY environment variable or use --openai-api-key flag."
        ))
        .into())
    }
}

/// Run the agent with the appropriate provider
pub(crate) async fn run_agent(
    config: &Config,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<AgentResult> {
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
) -> Result<AgentResult> {
    let agent = Agent::new(config.clone(), client);
    let result = agent.run(system_prompt, user_prompt).await?;

    if result.interrupted {
        tracing::info!(
            turns = result.turns_completed,
            "agent was interrupted by signal"
        );
    }

    Ok(result)
}

/// Run the pipeline with the appropriate provider
pub(crate) async fn run_pipeline(
    config: &Config,
    system_prompt: &str,
    context: &GatheredContext,
    user_request: &str,
    json_mode: bool,
) -> Result<String> {
    match config.provider {
        Provider::Ollama => {
            let client = OllamaClient::new(&config.ollama_url, &config.model);
            run_pipeline_with_client(client, system_prompt, context, user_request, json_mode).await
        }
        Provider::OpenAi => {
            let client = create_openai_client(config)?;
            run_pipeline_with_client(client, system_prompt, context, user_request, json_mode).await
        }
    }
}

/// Run the pipeline with a specific LLM client
async fn run_pipeline_with_client<C: LlmClient>(
    client: C,
    system_prompt: &str,
    context: &GatheredContext,
    user_request: &str,
    json_mode: bool,
) -> Result<String> {
    let pipeline = Pipeline::new(client);
    pipeline
        .execute(system_prompt, context, user_request, json_mode)
        .await
        .map_err(Into::into)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_error_to_exit_code_config() {
        let err = CliError::Config(anyhow::anyhow!("invalid config"));
        assert_eq!(err.to_exit_code(), ExitCode::ConfigError);
    }

    #[test]
    fn test_cli_error_to_exit_code_input() {
        let err = CliError::Input(anyhow::anyhow!("file not found"));
        assert_eq!(err.to_exit_code(), ExitCode::InputError);
    }

    #[test]
    fn test_cli_error_to_exit_code_git() {
        let err = CliError::Git(anyhow::anyhow!("not a git repo"));
        assert_eq!(err.to_exit_code(), ExitCode::GitError);
    }

    #[test]
    fn test_cli_error_to_exit_code_parse() {
        let err = CliError::Parse(anyhow::anyhow!("invalid json"));
        assert_eq!(err.to_exit_code(), ExitCode::ParseError);
    }

    #[test]
    fn test_cli_error_to_exit_code_agent() {
        let err = CliError::Agent(AgentError::MaxTurnsExceeded(10));
        assert_eq!(err.to_exit_code(), ExitCode::AgentLimitError);
    }
}
