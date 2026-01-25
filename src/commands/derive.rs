//! The `derive` command implementation
//!
//! A unified generative command that supports multiple output types (commit-msg,
//! explanation, summary) with optional recursive chunking for large inputs.

use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::cli::run_pipeline;
use crate::config::Config;
use crate::context::InputContext;
use crate::input::read_input;
use crate::output::{
    ChunkPlan, CommandOutput, DeriveResult, DryRunResult, ExitCode, ExitStatus, OutputMode,
};
use crate::parsers::{parse_commit_response, parse_derive_response};
use crate::prompts::derive_prompt_for_type;
use crate::tokens::{TokenLimits, check_token_limits, count_context_tokens};

/// The type of content to derive from the input
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeriveType {
    /// Generate a commit message from a diff
    CommitMsg,
    /// Generate an explanation of code
    Explanation,
    /// Generate a summary of the input
    Summary,
}

impl DeriveType {
    /// Parse derive type from string argument
    ///
    /// # Errors
    /// Returns error if the type string is not recognized.
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "commit-msg" | "commit" => Ok(Self::CommitMsg),
            "explanation" | "explain" => Ok(Self::Explanation),
            "summary" => Ok(Self::Summary),
            _ => Err(anyhow::anyhow!(
                "unknown derive type '{s}'; valid types: commit-msg, explanation, summary"
            )),
        }
    }

    /// Get the command name for prompts and dry-run output
    #[must_use]
    pub fn command_name(self) -> &'static str {
        match self {
            Self::CommitMsg => "derive:commit-msg",
            Self::Explanation => "derive:explanation",
            Self::Summary => "derive:summary",
        }
    }
}

/// Options for the derive command
pub struct DeriveOptions {
    /// Type of content to derive
    pub derive_type: DeriveType,
    /// Commit style (only used for commit-msg type)
    pub style: Option<String>,
    /// Enable recursive chunking for large inputs
    pub chunk_recursive: bool,
    /// Maximum concurrent chunk executions
    pub max_concurrency: usize,
    /// Show dry-run information without LLM calls
    pub dry_run: bool,
}

/// Execute the derive command
///
/// # Errors
/// Returns error if input reading, LLM call, or response parsing fails.
pub async fn cmd_derive(
    config: &Config,
    options: DeriveOptions,
    from: Option<PathBuf>,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    let DeriveOptions {
        derive_type,
        style,
        chunk_recursive,
        max_concurrency: _max_concurrency,
        dry_run,
    } = options;

    // Read input from stdin or --from
    let input = read_input(from.as_ref())
        .await
        .context("failed to read input")?;

    let ctx = InputContext::new(input);

    // Get token count for validation and dry-run
    let token_count = count_context_tokens(&ctx, config.tokenizer_mode)?;

    // Dry-run mode: output token estimation without LLM calls
    if dry_run {
        let chunk_plan = if chunk_recursive {
            // Estimate chunking plan
            let chunk_count = estimate_chunk_count(token_count.count);
            ChunkPlan {
                enabled: true,
                chunk_count: Some(chunk_count),
                chunks: vec![], // Detailed chunks omitted in dry-run for now
            }
        } else {
            ChunkPlan {
                enabled: false,
                chunk_count: None,
                chunks: vec![],
            }
        };

        let result = DryRunResult::new(
            derive_type.command_name(),
            token_count.count,
            config.provider.to_string(),
            &config.model,
            config.timeout_secs,
        )
        .with_chunking(chunk_plan);

        println!("{}", result.render(output_mode));
        return Ok(result.exit_code());
    }

    // Check token limits and warn/error appropriately
    let limits = TokenLimits::default();
    let check = check_token_limits(token_count, &limits);

    match check {
        crate::tokens::TokenCheck::Ok(_) => {}
        crate::tokens::TokenCheck::Warning { message, .. } => {
            if !chunk_recursive {
                eprintln!("warning: {message}; consider using --chunk-recursive");
            }
        }
        crate::tokens::TokenCheck::Error { message, .. } => {
            if !chunk_recursive {
                return Err(anyhow::anyhow!(
                    "{message}; use --chunk-recursive to process large inputs"
                ));
            }
        }
    }

    // Execute the appropriate pipeline
    let result = if chunk_recursive {
        execute_chunked(config, derive_type, style.as_deref(), &ctx).await?
    } else {
        execute_single_pass(config, derive_type, style.as_deref(), &ctx).await?
    };

    println!("{}", result.render(output_mode));
    Ok(result.exit_code())
}

/// Execute single-pass derive (no chunking)
async fn execute_single_pass(
    config: &Config,
    derive_type: DeriveType,
    style: Option<&str>,
    ctx: &InputContext,
) -> Result<DeriveResult> {
    let system_prompt = derive_prompt_for_type(derive_type);
    let user_request = build_user_request(derive_type, style);

    let response = run_pipeline(config, system_prompt, ctx, &user_request, true).await?;

    parse_derive_result(derive_type, &response)
}

/// Execute chunked derive with synthesis
async fn execute_chunked(
    config: &Config,
    derive_type: DeriveType,
    style: Option<&str>,
    ctx: &InputContext,
) -> Result<DeriveResult> {
    // Split input into chunks
    let chunks = split_by_tokens(&ctx.content, TARGET_CHUNK_TOKENS);

    if chunks.len() == 1 {
        // Input fits in single chunk, use single-pass
        return execute_single_pass(config, derive_type, style, ctx).await;
    }

    tracing::info!(
        chunk_count = chunks.len(),
        "processing with chunked execution"
    );

    // Process chunks in parallel
    let chunk_system_prompt = derive_chunk_prompt_for_type(derive_type);
    let chunk_user_request = build_chunk_user_request(derive_type, style);

    let mut chunk_results = Vec::with_capacity(chunks.len());
    for (i, chunk_content) in chunks.iter().enumerate() {
        let chunk_ctx = InputContext::new(chunk_content.clone());
        tracing::debug!(
            chunk = i,
            tokens = chunk_content.len() / 4,
            "processing chunk"
        );

        let response = run_pipeline(
            config,
            chunk_system_prompt,
            &chunk_ctx,
            &chunk_user_request,
            true,
        )
        .await
        .with_context(|| format!("failed to process chunk {i}"))?;

        chunk_results.push(response);
    }

    // Synthesize results
    let synthesis_ctx = InputContext::new(chunk_results.join("\n\n---\n\n"));
    let synthesis_prompt = derive_synthesis_prompt_for_type(derive_type);
    let synthesis_request = build_synthesis_request(derive_type, style);

    let final_response = run_pipeline(
        config,
        synthesis_prompt,
        &synthesis_ctx,
        &synthesis_request,
        true,
    )
    .await
    .context("failed to synthesize chunk results")?;

    parse_derive_result(derive_type, &final_response)
}

/// Target tokens per chunk (roughly 4k to leave room for prompts)
const TARGET_CHUNK_TOKENS: usize = 4000;

/// Estimate number of chunks for dry-run output
fn estimate_chunk_count(total_tokens: usize) -> usize {
    if total_tokens <= TARGET_CHUNK_TOKENS {
        1
    } else {
        total_tokens.div_ceil(TARGET_CHUNK_TOKENS)
    }
}

/// Split content into chunks of approximately `target_tokens` each
///
/// Attempts to split at logical boundaries (double newlines, single newlines)
/// while staying close to the target token count.
fn split_by_tokens(content: &str, target_tokens: usize) -> Vec<String> {
    if content.is_empty() {
        return vec![String::new()];
    }

    // Approximate chars per token (conservative estimate)
    let chars_per_token = 4;
    let target_chars = target_tokens * chars_per_token;

    // If content fits in one chunk, return as-is
    if content.len() <= target_chars {
        return vec![content.to_string()];
    }

    let mut chunks = Vec::new();
    let mut remaining = content;

    while !remaining.is_empty() {
        if remaining.len() <= target_chars {
            chunks.push(remaining.to_string());
            break;
        }

        // Find a good split point near target_chars
        let split_point = find_split_point(remaining, target_chars);
        let (chunk, rest) = remaining.split_at(split_point);
        chunks.push(chunk.trim().to_string());
        remaining = rest.trim_start();
    }

    chunks
}

/// Find a good split point near the target position
///
/// Prefers splitting at paragraph boundaries (\n\n), then line boundaries (\n),
/// then falls back to the exact position.
fn find_split_point(content: &str, target: usize) -> usize {
    let search_start = target.saturating_sub(500);
    let search_end = (target + 500).min(content.len());
    let search_range = &content[search_start..search_end];

    // Look for paragraph boundary first
    if let Some(pos) = search_range.rfind("\n\n") {
        return search_start + pos + 2;
    }

    // Fall back to line boundary
    if let Some(pos) = search_range.rfind('\n') {
        return search_start + pos + 1;
    }

    // Last resort: split at exact position
    target.min(content.len())
}

/// Build user request for single-pass execution
fn build_user_request(derive_type: DeriveType, style: Option<&str>) -> String {
    match derive_type {
        DeriveType::CommitMsg => {
            let style_name = style.unwrap_or("conventional");
            format!("Generate a {style_name} commit message for these changes.")
        }
        DeriveType::Explanation => "Explain this code.".to_string(),
        DeriveType::Summary => "Summarize this content.".to_string(),
    }
}

/// Build user request for chunk processing
fn build_chunk_user_request(derive_type: DeriveType, style: Option<&str>) -> String {
    match derive_type {
        DeriveType::CommitMsg => {
            let style_name = style.unwrap_or("conventional");
            format!(
                "Summarize the changes in this portion of the diff for a {style_name} commit message."
            )
        }
        DeriveType::Explanation => "Explain this section of code.".to_string(),
        DeriveType::Summary => "Summarize this section.".to_string(),
    }
}

/// Build user request for synthesis step
fn build_synthesis_request(derive_type: DeriveType, style: Option<&str>) -> String {
    match derive_type {
        DeriveType::CommitMsg => {
            let style_name = style.unwrap_or("conventional");
            format!("Combine these partial summaries into a single {style_name} commit message.")
        }
        DeriveType::Explanation => {
            "Combine these section explanations into a cohesive overall explanation.".to_string()
        }
        DeriveType::Summary => {
            "Combine these partial summaries into a comprehensive overall summary.".to_string()
        }
    }
}

/// Get the chunk processing prompt for a derive type
fn derive_chunk_prompt_for_type(derive_type: DeriveType) -> &'static str {
    match derive_type {
        DeriveType::CommitMsg => crate::prompts::DERIVE_COMMIT_CHUNK_PROMPT,
        DeriveType::Explanation => crate::prompts::DERIVE_EXPLANATION_CHUNK_PROMPT,
        DeriveType::Summary => crate::prompts::DERIVE_SUMMARY_CHUNK_PROMPT,
    }
}

/// Get the synthesis prompt for a derive type
fn derive_synthesis_prompt_for_type(derive_type: DeriveType) -> &'static str {
    match derive_type {
        DeriveType::CommitMsg => crate::prompts::DERIVE_COMMIT_SYNTHESIS_PROMPT,
        DeriveType::Explanation => crate::prompts::DERIVE_EXPLANATION_SYNTHESIS_PROMPT,
        DeriveType::Summary => crate::prompts::DERIVE_SUMMARY_SYNTHESIS_PROMPT,
    }
}

/// Parse derive result based on type
fn parse_derive_result(derive_type: DeriveType, response: &str) -> Result<DeriveResult> {
    match derive_type {
        DeriveType::CommitMsg => {
            let commit = parse_commit_response(response)?;
            Ok(DeriveResult::CommitMsg {
                message: commit.message,
                title: commit.title,
                body: commit.body,
            })
        }
        DeriveType::Explanation | DeriveType::Summary => {
            parse_derive_response(derive_type, response)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_type_from_str() {
        assert_eq!(
            DeriveType::from_str("commit-msg").unwrap(),
            DeriveType::CommitMsg
        );
        assert_eq!(
            DeriveType::from_str("commit").unwrap(),
            DeriveType::CommitMsg
        );
        assert_eq!(
            DeriveType::from_str("explanation").unwrap(),
            DeriveType::Explanation
        );
        assert_eq!(
            DeriveType::from_str("explain").unwrap(),
            DeriveType::Explanation
        );
        assert_eq!(
            DeriveType::from_str("summary").unwrap(),
            DeriveType::Summary
        );
        assert!(DeriveType::from_str("invalid").is_err());
    }

    #[test]
    fn test_derive_type_command_name() {
        assert_eq!(DeriveType::CommitMsg.command_name(), "derive:commit-msg");
        assert_eq!(DeriveType::Explanation.command_name(), "derive:explanation");
        assert_eq!(DeriveType::Summary.command_name(), "derive:summary");
    }

    #[test]
    fn test_split_by_tokens_small_input() {
        let content = "small input";
        let chunks = split_by_tokens(content, 1000);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "small input");
    }

    #[test]
    fn test_split_by_tokens_empty() {
        let chunks = split_by_tokens("", 1000);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].is_empty());
    }

    #[test]
    fn test_split_by_tokens_large_input() {
        // Create content larger than target
        let content = "line\n".repeat(1000);
        let chunks = split_by_tokens(&content, 100); // 100 tokens * 4 chars = 400 chars
        assert!(chunks.len() > 1);
    }

    #[test]
    fn test_estimate_chunk_count() {
        assert_eq!(estimate_chunk_count(1000), 1);
        assert_eq!(estimate_chunk_count(4000), 1);
        assert_eq!(estimate_chunk_count(4001), 2);
        assert_eq!(estimate_chunk_count(8000), 2);
        assert_eq!(estimate_chunk_count(12000), 3);
    }

    #[test]
    fn test_build_user_request() {
        let req = build_user_request(DeriveType::CommitMsg, None);
        assert!(req.contains("conventional"));

        let req = build_user_request(DeriveType::CommitMsg, Some("simple"));
        assert!(req.contains("simple"));

        let req = build_user_request(DeriveType::Explanation, None);
        assert!(req.contains("Explain"));

        let req = build_user_request(DeriveType::Summary, None);
        assert!(req.contains("Summarize"));
    }
}
