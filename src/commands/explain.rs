//! The `explain` command implementation

use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::cli::run_pipeline;
use crate::config::Config;
use crate::context::InputContext;
use crate::input::read_input;
use crate::output::{ChunkPlan, CommandOutput, DryRunResult, ExitCode, ExitStatus, OutputMode};
use crate::parsers::parse_explain_response;
use crate::prompts::pipeline_prompt_for_command;
use crate::tokens::count_context_tokens;

pub async fn cmd_explain(
    config: &Config,
    from: Option<PathBuf>,
    output_mode: OutputMode,
    chunk_mode: bool,
    dry_run: bool,
) -> Result<ExitCode> {
    // Warn if --chunk is passed (not supported for explain)
    if chunk_mode {
        eprintln!("warning: --chunk is not supported for explain (requires full context)");
    }

    // Read input from stdin or --from
    let input = read_input(from.as_ref())
        .await
        .context("failed to read code to explain")?;

    let ctx = InputContext::new(input);

    // Dry-run mode: output token estimation without LLM call
    if dry_run {
        let token_count = count_context_tokens(&ctx, config.tokenizer_mode)?;

        let result = DryRunResult::new(
            "explain",
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

    let response = run_pipeline(
        config,
        pipeline_prompt_for_command("explain"),
        &ctx,
        "Explain this code.",
        true, // enforce JSON output at API level
    )
    .await?;

    let result = parse_explain_response(&response)?;

    println!("{}", result.render(output_mode));

    Ok(ExitCode::Success)
}
