//! The `review` command implementation

use std::path::PathBuf;

use anyhow::{Context, Result};
use futures::stream::{self, StreamExt};

use crate::chunk::{DiffChunk, chunk_diff_by_file};
use crate::cli::run_pipeline;
use crate::config::Config;
use crate::context::InputContext;
use crate::input::{is_diff_format, read_input};
use crate::output::{
    ChunkFailure, ChunkInfo, ChunkPlan, CommandOutput, DryRunResult, ExitCode, ExitStatus,
    OutputMode, ReviewIssue, ReviewResult,
};
use crate::parsers::parse_review_response;
use crate::pipeline::PipelineError;
use crate::prompts::{build_category_review_prompt, build_review_prompt};
use crate::rules::RulesConfig;
use crate::tokens::{TokenCheck, TokenLimits, check_token_limits, count_context_tokens};

/// Options for the review command
pub struct ReviewOptions {
    /// Enable chunked processing
    pub chunk: bool,
    /// Maximum concurrent chunk executions
    pub max_concurrency: usize,
    /// Return partial results when some chunks fail
    pub partial: bool,
    /// Show dry-run information without LLM calls
    pub dry_run: bool,
}

/// Input type determined by validation
#[derive(Debug)]
enum InputType {
    /// Input is in diff format (auto-detected from stdin or file)
    Diff,
    /// Input is from --from FILE (not diff format)
    SingleFile(PathBuf),
}

/// Validate input and determine its type
///
/// # Errors
/// Returns error if:
/// - stdin content is not diff format and no --from is provided
fn validate_input(content: &str, from: Option<&PathBuf>) -> Result<InputType, &'static str> {
    let is_diff = is_diff_format(content);

    match (is_diff, from) {
        // Diff format is always OK
        (true, _) => Ok(InputType::Diff),
        // Non-diff with --from is a single file review
        (false, Some(path)) => Ok(InputType::SingleFile(path.clone())),
        // Non-diff from stdin without --from is an error
        (false, None) => Err(
            "review expects diff input or --from FILE; use 'usx derive explanation' for arbitrary text",
        ),
    }
}

pub async fn cmd_review(
    config: &Config,
    options: ReviewOptions,
    from: Option<PathBuf>,
    checks: &[String],
    output_mode: OutputMode,
) -> Result<ExitCode> {
    let ReviewOptions {
        chunk: chunk_mode,
        max_concurrency,
        partial,
        dry_run,
    } = options;

    // Load rules config for --checks filtering
    let rules_config =
        RulesConfig::load(&config.working_dir).context("failed to load review rules")?;

    // Read input from stdin or --from
    let input = read_input(from.as_ref())
        .await
        .context("failed to read code to review")?;

    // Validate input type
    let input_type = validate_input(&input, from.as_ref()).map_err(|e| anyhow::anyhow!("{e}"))?;

    // Handle --chunk with single file (not supported)
    if chunk_mode {
        match &input_type {
            InputType::SingleFile(_) => {
                eprintln!(
                    "warning: chunking single files not supported; processing as single-pass"
                );
            }
            InputType::Diff => {
                // Check if diff is not in proper format for chunking
                let chunks = chunk_diff_by_file(&input);
                if chunks.is_empty() {
                    return Err(anyhow::anyhow!(
                        "--chunk requires diff input with file boundaries; input has no recognizable file sections"
                    ));
                }
            }
        }
    }

    let ctx = InputContext::new(&input);

    // Dry-run mode: output token estimation without LLM calls
    if dry_run {
        return handle_review_dry_run(config, &ctx, chunk_mode, &input, output_mode);
    }

    // Build system prompt with rules (same for all chunks)
    let rules_section = rules_config.resolve(checks, &[]).to_prompt_section();
    let system_prompt = if checks.is_empty() {
        build_review_prompt(&rules_section)
    } else {
        let categories = checks.join(", ");
        build_category_review_prompt(&categories, &rules_section)
    };

    // Branch based on chunking mode
    if chunk_mode && matches!(input_type, InputType::Diff) {
        let chunks = chunk_diff_by_file(&input);
        if !chunks.is_empty() {
            return execute_chunked_diff_review(
                config,
                &system_prompt,
                chunks,
                max_concurrency,
                partial,
                output_mode,
            )
            .await;
        }
    }

    // Single-pass review
    // Check token limits
    let token_count = count_context_tokens(&ctx, config.tokenizer_mode)?;
    match check_token_limits(token_count, &TokenLimits::default()) {
        TokenCheck::Warning { message, .. } => eprintln!("warning: {message}"),
        TokenCheck::Error { message, .. } => return Err(PipelineError::TokenLimit(message).into()),
        TokenCheck::Ok(_) => {}
    }

    let response = run_pipeline(
        config,
        &system_prompt,
        &ctx,
        "Review this code.",
        true, // enforce JSON output at API level
    )
    .await?;

    let result = parse_review_response(&response)?;

    let exit_code = result.exit_code();
    println!("{}", result.render(output_mode));
    Ok(exit_code)
}

/// Execute chunked review, processing each file's diff independently
async fn execute_chunked_diff_review(
    config: &Config,
    system_prompt: &str,
    chunks: Vec<DiffChunk>,
    max_concurrency: usize,
    partial: bool,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    let chunk_count = chunks.len();
    tracing::info!(chunks = chunk_count, "starting chunked review");

    // Process chunks with bounded concurrency
    let results: Vec<(String, Result<ReviewResult, String>)> = stream::iter(chunks)
        .map(|chunk| {
            let file_path = chunk.file_path.clone();
            let system_prompt = system_prompt.to_string();
            async move {
                let result = process_single_chunk(config, &system_prompt, &chunk).await;
                (file_path, result)
            }
        })
        .buffer_unordered(max_concurrency)
        .collect()
        .await;

    // Aggregate results
    let mut all_issues: Vec<ReviewIssue> = Vec::new();
    let mut chunk_failures: Vec<ChunkFailure> = Vec::new();
    let mut successful_chunks = 0;

    for (file_path, result) in results {
        match result {
            Ok(review_result) => {
                successful_chunks += 1;
                all_issues.extend(review_result.issues);
            }
            Err(error) => {
                if partial {
                    eprintln!("warning: chunk failed for {file_path}: {error}");
                    chunk_failures.push(ChunkFailure { file_path, error });
                } else {
                    return Err(anyhow::anyhow!("review failed for {file_path}: {error}"));
                }
            }
        }
    }

    // All chunks failed
    if successful_chunks == 0 && !chunk_failures.is_empty() {
        return Err(anyhow::anyhow!(
            "all {} chunk(s) failed; first error: {}",
            chunk_failures.len(),
            chunk_failures
                .first()
                .map_or("unknown", |f| f.error.as_str())
        ));
    }

    // Determine if review passed (no errors in issues)
    let has_errors = all_issues.iter().any(|i| i.severity == "error");
    let passed = !has_errors && chunk_failures.is_empty();

    // Build summary
    let summary = if chunk_failures.is_empty() {
        format!("Reviewed {chunk_count} file(s)")
    } else {
        let failed_count = chunk_failures.len();
        format!(
            "Reviewed {chunk_count} file(s) ({successful_chunks} succeeded, {failed_count} failed)"
        )
    };

    let result = ReviewResult {
        summary,
        issues: all_issues,
        passed,
        chunks_processed: Some(chunk_count),
        chunk_failures,
    };

    let exit_code = result.exit_code();
    println!("{}", result.render(output_mode));
    Ok(exit_code)
}

/// Process a single diff chunk and return the review result
async fn process_single_chunk(
    config: &Config,
    system_prompt: &str,
    chunk: &DiffChunk,
) -> Result<ReviewResult, String> {
    let ctx = InputContext::new(&chunk.content);

    // Check token limits for this chunk
    let token_count = count_context_tokens(&ctx, config.tokenizer_mode)
        .map_err(|e| format!("token counting failed: {e}"))?;

    if let TokenCheck::Error { message, .. } =
        check_token_limits(token_count, &TokenLimits::default())
    {
        return Err(format!("chunk too large: {message}"));
    }

    let response = run_pipeline(config, system_prompt, &ctx, "Review this code.", true)
        .await
        .map_err(|e| format!("LLM call failed: {e}"))?;

    parse_review_response(&response).map_err(|e| format!("parse failed: {e}"))
}

/// Handle dry-run mode for review command
fn handle_review_dry_run(
    config: &Config,
    context: &InputContext,
    chunk_mode: bool,
    raw_input: &str,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    let token_count = count_context_tokens(context, config.tokenizer_mode)?;

    // Determine chunk plan
    let chunk_plan = if chunk_mode && is_diff_format(raw_input) {
        let diff_chunks = chunk_diff_by_file(raw_input);
        if diff_chunks.is_empty() {
            ChunkPlan {
                enabled: false,
                chunk_count: None,
                chunks: vec![],
            }
        } else {
            // Estimate tokens for each chunk
            let chunk_infos: Vec<ChunkInfo> = diff_chunks
                .iter()
                .map(|c| {
                    let chunk_tokens = crate::tokens::count_tokens_heuristic(&c.content);
                    ChunkInfo {
                        id: c.file_path.clone(),
                        tokens_estimated: chunk_tokens.count,
                        files: vec![c.file_path.clone()],
                    }
                })
                .collect();

            ChunkPlan {
                enabled: true,
                chunk_count: Some(diff_chunks.len()),
                chunks: chunk_infos,
            }
        }
    } else {
        ChunkPlan {
            enabled: false,
            chunk_count: None,
            chunks: vec![],
        }
    };

    let result = DryRunResult::new(
        "review",
        token_count.count,
        config.provider.to_string(),
        &config.model,
        config.timeout_secs,
    )
    .with_chunking(chunk_plan);

    println!("{}", result.render(output_mode));
    Ok(result.exit_code())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_input_diff_stdin() {
        let diff = "diff --git a/file.rs b/file.rs\n--- a/file.rs\n+++ b/file.rs\n";
        let result = validate_input(diff, None);
        assert!(matches!(result, Ok(InputType::Diff)));
    }

    #[test]
    fn test_validate_input_diff_with_from() {
        let diff = "diff --git a/file.rs b/file.rs\n--- a/file.rs\n+++ b/file.rs\n";
        let from = PathBuf::from("changes.diff");
        let result = validate_input(diff, Some(&from));
        assert!(matches!(result, Ok(InputType::Diff)));
    }

    #[test]
    fn test_validate_input_single_file() {
        let code = "fn main() { println!(\"hello\"); }";
        let from = PathBuf::from("src/main.rs");
        let result = validate_input(code, Some(&from));
        assert!(matches!(result, Ok(InputType::SingleFile(_))));
    }

    #[test]
    fn test_validate_input_invalid_stdin() {
        let code = "fn main() { println!(\"hello\"); }";
        let result = validate_input(code, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("review expects diff input"));
    }
}
