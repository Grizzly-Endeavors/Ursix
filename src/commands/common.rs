//! Shared helpers for command implementations
//!
//! This module provides common functionality used across multiple commands,
//! including dry-run handling and token limit validation.

use anyhow::Result;

use crate::config::Config;
use crate::context::InputContext;
use crate::output::{ChunkPlan, CommandOutput, DryRunResult, ExitCode, ExitStatus, OutputMode};
use crate::pipeline::PipelineError;
use crate::tokens::{TokenCheck, TokenLimits, check_token_limits, count_context_tokens};

/// Handle dry-run mode for a command
///
/// Counts tokens in the context and returns a [`DryRunResult`] with the
/// token estimation and configuration information.
pub(crate) fn handle_dry_run(
    command: &str,
    config: &Config,
    context: &InputContext,
    chunk_plan: ChunkPlan,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    let token_count = count_context_tokens(context, config.tokenizer_mode)?;

    let result = DryRunResult::new(
        command,
        token_count.count,
        config.provider.to_string(),
        &config.model,
        config.timeout_secs,
    )
    .with_chunking(chunk_plan);

    println!("{}", result.render(output_mode));
    Ok(result.exit_code())
}

/// Validate token limits and handle warnings/errors
///
/// Returns `Ok(())` if within limits (with optional warning printed to stderr),
/// or `Err` with a [`PipelineError::TokenLimit`] if the limit is exceeded.
pub(crate) fn validate_token_limits(config: &Config, context: &InputContext) -> Result<()> {
    let token_count = count_context_tokens(context, config.tokenizer_mode)?;
    match check_token_limits(token_count, &TokenLimits::default()) {
        TokenCheck::Warning { message, .. } => {
            eprintln!("warning: {message}");
            Ok(())
        }
        TokenCheck::Error { message, .. } => Err(PipelineError::TokenLimit(message).into()),
        TokenCheck::Ok(_) => Ok(()),
    }
}
