//! The `commit` command implementation

use anyhow::{Context, Result};
use tokio::process::Command as TokioCommand;

use crate::cli::{CliError, run_pipeline};
use crate::config::Config;
use crate::context::{GatheredContext, gather_commit_context};
use crate::input::try_read_piped_stdin;
use crate::output::{ChunkPlan, CommandOutput, DryRunResult, ExitCode, ExitStatus, OutputMode};
use crate::parsers::parse_commit_response;
use crate::prompts::pipeline_prompt_for_command;
use crate::tokens::count_context_tokens;

/// Commit message format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageFormat {
    /// Subject line only
    SubjectOnly,
    /// Subject with detailed body
    WithBody,
}

impl MessageFormat {
    /// Create from the --body flag
    #[must_use]
    pub fn from_body_flag(body: bool) -> Self {
        if body {
            Self::WithBody
        } else {
            Self::SubjectOnly
        }
    }

    /// Whether to include a detailed body
    #[must_use]
    pub fn includes_body(self) -> bool {
        matches!(self, Self::WithBody)
    }
}

/// What to do after generating the commit message
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostAction {
    /// Just print the message
    PrintOnly,
    /// Execute git commit with the message
    Execute,
}

impl PostAction {
    /// Create from the --execute flag
    #[must_use]
    pub fn from_execute_flag(execute: bool) -> Self {
        if execute {
            Self::Execute
        } else {
            Self::PrintOnly
        }
    }
}

/// Options for the commit command
pub struct CommitOptions {
    /// Commit message format
    pub format: MessageFormat,
    /// Action after generating message
    pub post_action: PostAction,
    /// Enable chunked processing (not supported, issues warning)
    pub chunk_mode: bool,
    /// Show dry-run information without LLM calls
    pub dry_run: bool,
}

pub async fn cmd_commit(
    config: &Config,
    options: CommitOptions,
    style: &str,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    let CommitOptions {
        format,
        post_action,
        chunk_mode,
        dry_run,
    } = options;

    // Warn if --chunk is passed (not supported for commit)
    if chunk_mode {
        eprintln!("warning: --chunk is not supported for commit (requires full context)");
    }

    // Check for piped stdin first - use as diff if present
    let piped_content = try_read_piped_stdin();

    let context = if let Some(content) = piped_content {
        // Use piped stdin as diff content
        tracing::debug!("using piped stdin as commit diff");
        GatheredContext {
            files: Vec::new(),
            git_diff: Some(content),
            git_status: None,
            additional_context: None,
        }
    } else {
        let ctx = gather_commit_context(&config.working_dir)
            .await
            .context("failed to gather commit context")?;

        // Check if there are staged changes (only when not using piped input)
        if ctx
            .git_diff
            .as_ref()
            .is_some_and(|diff| diff.trim().is_empty())
        {
            return Err(CliError::Git(anyhow::anyhow!("no staged changes to commit")).into());
        }

        ctx
    };

    // Dry-run mode: output token estimation without LLM calls
    if dry_run {
        let token_count = count_context_tokens(&context, config.tokenizer_mode)?;

        let result = DryRunResult::new(
            "commit",
            token_count.count,
            config.provider.to_string(),
            &config.model,
            config.timeout_secs,
        )
        .with_chunking(ChunkPlan {
            enabled: false,
            chunk_count: None,
            chunks: vec![],
        });

        println!("{}", result.render(output_mode));
        return Ok(result.exit_code());
    }

    let user_request = format!(
        "Generate a {} commit message{}.",
        style,
        if format.includes_body() {
            " with a detailed body explaining the changes"
        } else {
            ""
        }
    );

    let response = run_pipeline(
        config,
        pipeline_prompt_for_command("commit"),
        &context,
        &user_request,
        true, // enforce JSON output at API level
    )
    .await
    .context("failed to generate commit message")?;

    let result = parse_commit_response(&response)
        .context("failed to parse commit message from LLM response")?;

    println!("{}", result.render(output_mode));

    if post_action == PostAction::Execute {
        let git_output = execute_git_commit(&config.working_dir, &result.message).await?;
        if !git_output.is_empty() {
            println!("{git_output}");
        }
    }

    Ok(ExitCode::Success)
}

/// Execute git commit with the given message
///
/// Returns the git commit output on success for the caller to display.
async fn execute_git_commit(working_dir: &std::path::Path, message: &str) -> Result<String> {
    let output = TokioCommand::new("git")
        .args(["commit", "-m", message])
        .current_dir(working_dir)
        .output()
        .await
        .context("failed to execute git commit")?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(CliError::Git(anyhow::anyhow!("git commit failed: {}", stderr.trim())).into())
    }
}
