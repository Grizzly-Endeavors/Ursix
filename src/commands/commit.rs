//! The `commit` command implementation

use anyhow::{Context, Result};
use tokio::process::Command as TokioCommand;

use crate::cli::{CliError, run_pipeline};
use crate::config::Config;
use crate::context::{GatheredContext, gather_commit_context};
use crate::input::try_read_piped_stdin;
use crate::output::{CommandOutput, ExitCode, OutputMode};
use crate::parsers::parse_commit_response;
use crate::prompts::pipeline_prompt_for_command;

pub async fn cmd_commit(
    config: &Config,
    body: bool,
    style: &str,
    execute: bool,
    output_mode: OutputMode,
    chunk_mode: bool,
) -> Result<ExitCode> {
    // Warn if --chunk is passed (not supported for commit)
    if chunk_mode {
        eprintln!("warning: --chunk is not supported for commit (requires full context)");
    }

    // Check for piped stdin first - use as diff if present
    let piped_content = try_read_piped_stdin();

    // Commit is always pipeline mode - agent mode doesn't make sense for
    // a simple single-pass operation like generating a commit message
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

    let user_request = format!(
        "Generate a {} commit message{}.",
        style,
        if body {
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

    if execute {
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
