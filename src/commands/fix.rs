//! The `fix` command implementation

use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::cli::run_pipeline;
use crate::config::Config;
use crate::context::InputContext;
use crate::input::read_input;
use crate::output::{ChunkPlan, CommandOutput, DryRunResult, ExitCode, ExitStatus, OutputMode};
use crate::parsers::parse_fix_response;
use crate::pipeline::PipelineError;
use crate::prompts::pipeline_prompt_for_command;
use crate::tokens::{TokenCheck, TokenLimits, check_token_limits, count_context_tokens};

/// Options for the fix command
#[allow(clippy::struct_excessive_bools)]
pub struct FixOptions {
    /// Enable chunked processing
    pub chunk: bool,
    /// Maximum concurrent chunk executions
    pub max_concurrency: usize,
    /// Return partial results when some chunks fail
    pub partial: bool,
    /// Show dry-run information without LLM calls
    pub dry_run: bool,
}

pub async fn cmd_fix(
    config: &Config,
    options: FixOptions,
    from: Option<PathBuf>,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    let FixOptions {
        chunk: chunk_mode,
        max_concurrency: _,
        partial: _,
        dry_run,
    } = options;

    // Warn if --chunk is passed (not supported for fix without file-based context)
    if chunk_mode {
        eprintln!("warning: --chunk is not supported for fix in stdin mode");
    }

    // Read input from stdin or --from
    let input = read_input(from.as_ref())
        .await
        .context("failed to read code to fix")?;

    let ctx = InputContext::new(input);

    // Dry-run mode: output token estimation without LLM calls
    if dry_run {
        return handle_fix_dry_run(config, &ctx, output_mode);
    }

    // Check token limits
    let token_count = count_context_tokens(&ctx, config.tokenizer_mode)?;
    match check_token_limits(token_count, &TokenLimits::default()) {
        TokenCheck::Warning { message, .. } => {
            eprintln!("warning: {message}");
        }
        TokenCheck::Error { message, .. } => {
            return Err(PipelineError::TokenLimit(message).into());
        }
        TokenCheck::Ok(_) => {}
    }

    let response = run_pipeline(
        config,
        pipeline_prompt_for_command("fix"),
        &ctx,
        "Fix the issues in this code.",
        true, // enforce JSON output at API level
    )
    .await?;

    let result = parse_fix_response(&response)?;

    println!("{}", result.render(output_mode));

    Ok(result.exit_code())
}

/// Handle dry-run mode for fix command
fn handle_fix_dry_run(
    config: &Config,
    context: &InputContext,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    let token_count = count_context_tokens(context, config.tokenizer_mode)?;

    let result = DryRunResult::new(
        "fix",
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
    Ok(result.exit_code())
}
