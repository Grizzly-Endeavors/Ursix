//! The `explain` command implementation

use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::cli::run_pipeline;
use crate::config::Config;
use crate::context::{GatheredContext, gather_explain_context};
use crate::input::try_read_piped_stdin;
use crate::output::{ChunkPlan, CommandOutput, DryRunResult, ExitCode, ExitStatus, OutputMode};
use crate::parsers::parse_explain_response;
use crate::prompts::pipeline_prompt_for_command;
use crate::tokens::count_context_tokens;

pub async fn cmd_explain(
    config: &Config,
    target: &str,
    output_mode: OutputMode,
    chunk_mode: bool,
    dry_run: bool,
) -> Result<ExitCode> {
    // Warn if --chunk is passed (not supported for explain)
    if chunk_mode {
        eprintln!("warning: --chunk is not supported for explain (requires full context)");
    }

    // Check for piped stdin - use as file content if present
    let piped_content = try_read_piped_stdin();

    // Gather context and make single LLM call
    let context = if let Some(content) = piped_content {
        // Use piped stdin as file content
        tracing::debug!(target = %target, "using piped stdin as file content");
        GatheredContext {
            files: vec![crate::context::FileContext {
                path: PathBuf::from(target),
                content,
            }],
            git_diff: None,
            git_status: None,
            additional_context: None,
        }
    } else {
        // Read from file system
        let files = vec![PathBuf::from(target)];
        gather_explain_context(&config.working_dir, &files)
            .await
            .context("failed to gather explain context")?
    };

    // Dry-run mode: output token estimation without LLM call
    if dry_run {
        let token_count = count_context_tokens(&context, config.tokenizer_mode)?;
        let files: Vec<String> = context
            .files
            .iter()
            .map(|f| f.path.display().to_string())
            .collect();

        let result = DryRunResult::new(
            "explain",
            token_count.count,
            config.provider.to_string(),
            &config.model,
            config.timeout_secs,
        )
        .with_files(files)
        .with_chunking(ChunkPlan {
            enabled: false,
            chunk_count: None,
            chunks: vec![],
        });

        println!("{}", result.render(output_mode));
        return Ok(result.exit_code());
    }

    let response = run_pipeline(
        config,
        pipeline_prompt_for_command("explain"),
        &context,
        &format!("Explain this code from {target}"),
        true, // enforce JSON output at API level
    )
    .await?;

    let result = parse_explain_response(&response)?;

    println!("{}", result.render(output_mode));

    Ok(ExitCode::Success)
}
