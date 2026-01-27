//! CLI argument parsing using clap
//!
//! This module defines the CLI structure and all command-specific options.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::config::{Provider, TokenizerMode};

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
    /// Derive content from input (commit-msg, explanation, summary)
    Derive {
        /// Type of content to derive: commit-msg, explanation, summary
        #[arg(value_name = "TYPE")]
        derive_type: String,

        /// Read input from a file (use - for stdin)
        #[arg(long, value_name = "FILE")]
        from: Option<PathBuf>,

        /// Commit style (only for commit-msg type: conventional, simple)
        #[arg(long, default_value = "conventional")]
        style: String,

        /// Process large inputs by splitting into chunks and synthesizing results
        #[arg(long)]
        chunk_recursive: bool,

        /// Maximum concurrent chunk executions (default: 4)
        #[arg(long, default_value = "4")]
        max_concurrency: usize,
    },

    /// Review code changes from stdin or --from
    Review {
        /// Read input from a file (use - for stdin)
        #[arg(long, value_name = "FILE")]
        from: Option<PathBuf>,

        /// Checks to perform (e.g., style, security, performance)
        #[arg(long, value_delimiter = ',')]
        checks: Vec<String>,

        /// Enable file-based chunking for large diffs
        ///
        /// Splits diff input by file and processes each file independently.
        /// Only works with diff input via stdin; single files via --from are not chunked.
        #[arg(long)]
        chunk: bool,

        /// Maximum concurrent chunk executions (default: 4)
        #[arg(long, default_value = "4")]
        max_concurrency: usize,

        /// Continue processing remaining chunks when some fail
        ///
        /// Without this flag, the command exits on first chunk failure.
        /// With this flag, partial results are returned with failure details.
        #[arg(long)]
        partial: bool,
    },

    /// Fix issues in code using structured input
    ///
    /// Expects JSON input with: issue, snippet, file, lines
    Fix {
        /// Read input from a file (use - for stdin)
        #[arg(long, value_name = "FILE")]
        from: Option<PathBuf>,

        /// Number of context lines in unified diff output (default: 3)
        #[arg(long, default_value = "3")]
        context: usize,

        /// Retry on validation failure with error context
        #[arg(long)]
        retry: bool,

        /// Return raw LLM output on validation failure for inspection
        #[arg(long)]
        partial: bool,
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
    pub fn to_config(&self) -> crate::llm::RetryConfig {
        if self.enabled {
            crate::llm::RetryConfig {
                max_retries: self.max_retries,
                ..crate::llm::RetryConfig::default()
            }
        } else {
            crate::llm::RetryConfig::no_retry()
        }
    }
}
