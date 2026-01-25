//! CLI argument parsing and command dispatch
//!
//! This module defines the CLI structure using clap and dispatches to
//! command implementations in the `commands` module.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use thiserror::Error;

use crate::commands::{
    CommitOptions, FixOptions, MessageFormat, PostAction, ReviewOptions, cmd_commit, cmd_config,
    cmd_explain, cmd_fix, cmd_review,
};
use crate::config::{Config, Provider, TokenizerMode};
use crate::context::InputContext;
use crate::llm::LlmClient;
use crate::llm::RetryConfig;
use crate::llm::ollama::OllamaClient;
use crate::llm::openai::OpenAiClient;
use crate::output::{ExitCode, OutputMode, ToExitCode};
use crate::pipeline::Pipeline;
use crate::pipeline::PipelineError;

/// Options for retry behavior
#[derive(Debug, Clone)]
pub struct RetryOptions {
    /// Whether retries are enabled
    pub enabled: bool,
    /// Maximum retry attempts
    pub max_retries: u32,
}

impl RetryOptions {
    /// Create retry options from CLI flags
    #[must_use]
    pub fn from_cli(no_retry: bool, max_retries: u32) -> Self {
        Self {
            enabled: !no_retry,
            max_retries,
        }
    }

    /// Convert to [`RetryConfig`] for the retry infrastructure
    #[must_use]
    pub fn to_config(&self) -> RetryConfig {
        if self.enabled {
            RetryConfig {
                max_retries: self.max_retries,
                ..RetryConfig::default()
            }
        } else {
            RetryConfig::no_retry()
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "usx")]
#[command(about = "Ursix - Unix utilities powered by LLMs")]
#[command(version)]
#[allow(clippy::struct_excessive_bools)]
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

    /// Enable chunked processing for large inputs (parallel token-based processing)
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

    /// Disable automatic retry on transient failures
    #[arg(long, global = true)]
    pub no_retry: bool,

    /// Maximum retry attempts for transient failures (default: 3)
    #[arg(long, global = true, default_value = "3")]
    pub max_retries: u32,

    /// Return partial results when some chunks fail (instead of failing entirely)
    #[arg(long, global = true)]
    pub partial: bool,

    /// Timeout for LLM requests in seconds (default: 60)
    ///
    /// Each LLM API call will fail with a timeout error if it exceeds this duration.
    /// For chunked operations, this timeout applies to each individual chunk.
    #[arg(long, global = true)]
    pub timeout: Option<u64>,

    /// Show token estimation and chunking plan without making LLM calls
    ///
    /// Enables cost prediction, early "too large" detection, and CI validation without API cost.
    /// Output includes estimated tokens, chunking plan (if --chunk is enabled), and configuration.
    #[arg(long, global = true)]
    pub dry_run: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Explain code from stdin or --from
    Explain {
        /// Read input from a file (use - for stdin)
        #[arg(long, value_name = "FILE")]
        from: Option<PathBuf>,
    },

    /// Review code changes from stdin or --from
    Review {
        /// Read input from a file (use - for stdin)
        #[arg(long, value_name = "FILE")]
        from: Option<PathBuf>,

        /// Checks to perform (e.g., style, security, performance)
        #[arg(long, value_delimiter = ',')]
        checks: Vec<String>,
    },

    /// Fix issues in code from stdin or --from
    Fix {
        /// Read input from a file (use - for stdin)
        #[arg(long, value_name = "FILE")]
        from: Option<PathBuf>,
    },

    /// Generate commit message from diff provided via stdin or --from
    Commit {
        /// Read diff from a file (use - for stdin)
        #[arg(long, value_name = "FILE")]
        from: Option<PathBuf>,

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
}

impl ToExitCode for CliError {
    fn to_exit_code(&self) -> ExitCode {
        match self {
            Self::Config(_) | Self::Input(_) | Self::Git(_) => ExitCode::UserError,
            Self::Parse(_) => ExitCode::PermanentError,
            Self::Pipeline(e) => e.to_exit_code(),
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
    if let Some(mode) = cli.tokenizer {
        config.tokenizer_mode = mode;
    }

    // Apply retry config from CLI
    config.retry_config = RetryOptions::from_cli(cli.no_retry, cli.max_retries).to_config();

    // Apply timeout from CLI if provided
    if let Some(timeout) = cli.timeout {
        config.timeout_secs = timeout;
    }

    match cli.command {
        Command::Explain { from } => {
            cmd_explain(&config, from, output_mode, cli.chunk, cli.dry_run).await
        }
        Command::Review { from, checks } => {
            let options = ReviewOptions {
                chunk: cli.chunk,
                max_concurrency: cli.max_concurrency,
                partial: cli.partial,
                dry_run: cli.dry_run,
            };
            cmd_review(&config, options, from, &checks, output_mode).await
        }
        Command::Fix { from } => {
            let options = FixOptions {
                chunk: cli.chunk,
                max_concurrency: cli.max_concurrency,
                partial: cli.partial,
                dry_run: cli.dry_run,
            };
            cmd_fix(&config, options, from, output_mode).await
        }
        Command::Commit {
            from,
            body,
            style,
            execute,
        } => {
            let options = CommitOptions {
                format: MessageFormat::from_body_flag(body),
                post_action: PostAction::from_execute_flag(execute),
                chunk_mode: cli.chunk,
                dry_run: cli.dry_run,
            };
            cmd_commit(&config, options, from, &style, output_mode).await
        }
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
            config.timeout_secs,
        ))
    } else if is_local_url(&config.openai_url) {
        Ok(OpenAiClient::new(
            &config.openai_url,
            &config.model,
            config.timeout_secs,
        ))
    } else {
        Err(CliError::Config(anyhow::anyhow!(
            "OpenAI API key required for remote endpoints. \
             Set OPENAI_API_KEY environment variable or use --openai-api-key flag."
        ))
        .into())
    }
}

/// Run the pipeline with the appropriate provider
pub(crate) async fn run_pipeline(
    config: &Config,
    system_prompt: &str,
    context: &InputContext,
    user_request: &str,
    json_mode: bool,
) -> Result<String> {
    match config.provider {
        Provider::Ollama => {
            let client = OllamaClient::new(&config.ollama_url, &config.model, config.timeout_secs);
            run_pipeline_with_client(
                client,
                system_prompt,
                context,
                user_request,
                json_mode,
                &config.retry_config,
            )
            .await
        }
        Provider::OpenAi => {
            let client = create_openai_client(config)?;
            run_pipeline_with_client(
                client,
                system_prompt,
                context,
                user_request,
                json_mode,
                &config.retry_config,
            )
            .await
        }
    }
}

/// Run the pipeline with a specific LLM client
async fn run_pipeline_with_client<C: LlmClient + Clone + 'static>(
    client: C,
    system_prompt: &str,
    context: &InputContext,
    user_request: &str,
    json_mode: bool,
    retry_config: &RetryConfig,
) -> Result<String> {
    let pipeline = Pipeline::new(client);
    pipeline
        .execute(
            system_prompt,
            context,
            user_request,
            json_mode,
            retry_config,
        )
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
        assert_eq!(err.to_exit_code(), ExitCode::UserError);
    }

    #[test]
    fn test_cli_error_to_exit_code_input() {
        let err = CliError::Input(anyhow::anyhow!("file not found"));
        assert_eq!(err.to_exit_code(), ExitCode::UserError);
    }

    #[test]
    fn test_cli_error_to_exit_code_git() {
        let err = CliError::Git(anyhow::anyhow!("not a git repo"));
        assert_eq!(err.to_exit_code(), ExitCode::UserError);
    }

    #[test]
    fn test_cli_error_to_exit_code_parse() {
        let err = CliError::Parse(anyhow::anyhow!("invalid json"));
        assert_eq!(err.to_exit_code(), ExitCode::PermanentError);
    }
}
