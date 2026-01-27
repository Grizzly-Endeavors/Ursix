//! CLI argument parsing using clap
//!
//! This module defines the CLI structure and all command-specific options.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Mode for the fix command
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum FixMode {
    /// Fix a single issue (default)
    #[default]
    Atomic,
    /// Fix multiple issues in one file, processing bottom-to-top
    WholeFile,
}

#[derive(Parser, Debug)]
#[command(name = "usx")]
#[command(about = "Ursix - Unix utilities powered by LLMs")]
#[command(version)]
pub struct Cli {
    /// Output as plain text instead of JSON (default: JSON)
    #[arg(long, global = true)]
    pub text: bool,

    /// LLM provider: NAME [URL] [API-KEY]
    #[arg(long, global = true, num_args = 1..=3, value_names = ["NAME", "URL", "API-KEY"])]
    pub provider: Vec<String>,

    /// Model to use (overrides config)
    #[arg(short, long, global = true)]
    pub model: Option<String>,

    /// Max retry attempts for transient failures (0 to disable)
    #[arg(long, global = true, default_value = "3")]
    pub retries: u32,

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

        /// Input file (omit for stdin, use - for explicit stdin)
        #[arg(value_name = "FILE")]
        file: Option<PathBuf>,

        /// Split large inputs into chunks and synthesize results
        #[arg(long)]
        chunk: bool,

        /// Max concurrent chunk executions
        #[arg(long, default_value = "4")]
        concurrency: usize,
    },

    /// Review code changes
    Review {
        /// Input file (omit for stdin, use - for explicit stdin)
        #[arg(value_name = "FILE")]
        file: Option<PathBuf>,

        /// Checks to perform (e.g., style, security, performance)
        #[arg(long, value_delimiter = ',')]
        checks: Vec<String>,

        /// Split by file and process in parallel
        #[arg(long)]
        chunk: bool,

        /// Max concurrent chunk executions
        #[arg(long, default_value = "4")]
        concurrency: usize,

        /// Return partial results when some chunks fail instead of failing entirely
        #[arg(long)]
        partial: bool,
    },

    /// Fix issues in code using structured JSON input
    Fix {
        /// JSON input file (omit for stdin, use - for explicit stdin)
        #[arg(value_name = "FILE")]
        file: Option<PathBuf>,

        /// Fix mode: atomic (single issue) or whole-file (multiple issues)
        #[arg(long, value_enum, default_value = "atomic")]
        mode: FixMode,

        /// Diff context lines
        #[arg(long, default_value = "3")]
        context: usize,

        /// Retry on validation failure with error context
        #[arg(long)]
        retry: bool,

        /// Return partial results on validation failure instead of failing entirely
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

    /// Initialize Ursix configuration in current directory
    Init {
        /// Skip connectivity test after setup
        #[arg(long)]
        skip_test: bool,

        /// Force overwrite existing config files
        #[arg(long)]
        force: bool,
    },
}

/// Options for retry behavior
#[derive(Debug, Clone)]
pub struct RetryOptions {
    /// Maximum retry attempts (0 = disabled)
    pub max_retries: u32,
}

impl RetryOptions {
    /// Create retry options from CLI retries count
    #[must_use]
    pub fn from_cli(retries: u32) -> Self {
        Self {
            max_retries: retries,
        }
    }

    /// Convert to [`RetryConfig`] for the retry infrastructure
    #[must_use]
    pub fn to_config(&self) -> crate::llm::RetryConfig {
        if self.max_retries > 0 {
            crate::llm::RetryConfig {
                max_retries: self.max_retries,
                ..crate::llm::RetryConfig::default()
            }
        } else {
            crate::llm::RetryConfig::no_retry()
        }
    }
}
