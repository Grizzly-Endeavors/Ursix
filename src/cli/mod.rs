//! CLI argument parsing and command dispatch
//!
//! This module defines the CLI structure using clap and dispatches to
//! command implementations in the `commands` module.

mod args;
mod dispatch;

use anyhow::{Context, Result};
use clap::Parser;
use thiserror::Error;

pub use args::{Cli, Command, RetryOptions};
pub(crate) use dispatch::run_pipeline;

use crate::commands::{
    DeriveOptions, DeriveType, FixOptions, ReviewOptions, cmd_config, cmd_derive, cmd_fix,
    cmd_review,
};
use crate::config::Config;
use crate::output::{ExitCode, OutputMode, ToExitCode};
use crate::pipeline::PipelineError;

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
        Command::Derive {
            derive_type,
            from,
            style,
            chunk_recursive,
            max_concurrency,
        } => {
            let derive_type = DeriveType::from_str(&derive_type).map_err(CliError::Config)?;
            let options = DeriveOptions {
                derive_type,
                style: Some(style),
                chunk_recursive,
                max_concurrency,
                dry_run: cli.dry_run,
            };
            cmd_derive(&config, options, from, output_mode).await
        }
        Command::Review {
            from,
            checks,
            chunk,
            max_concurrency,
            partial,
        } => {
            let options = ReviewOptions {
                chunk,
                max_concurrency,
                partial,
                dry_run: cli.dry_run,
            };
            cmd_review(&config, options, from, &checks, output_mode).await
        }
        Command::Fix { from } => {
            let options = FixOptions {
                chunk: false,
                max_concurrency: 4,
                partial: false,
                dry_run: cli.dry_run,
            };
            cmd_fix(&config, options, from, output_mode).await
        }
        Command::Config { key, list } => cmd_config(&config, key, list, output_mode),
    }
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
