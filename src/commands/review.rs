//! The `review` command implementation

use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::chunk::{
    CategoryChunk, ChunkOptions, ChunkedResult, chunk_by_category, chunk_by_category_and_file,
    chunk_by_file, execute_category_chunks, execute_chunked,
};
use crate::cli::run_pipeline;
use crate::config::Config;
use crate::context::{GatheredContext, gather_review_context};
use crate::input::{read_from_source, stdin_is_piped, try_read_piped_stdin};
use crate::output::{CommandOutput, ExitCode, ExitStatus, OutputMode, ReviewIssue, ReviewResult};
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
}

pub async fn cmd_review(
    config: &Config,
    diff: Option<String>,
    files: Vec<String>,
    options: ReviewOptions,
    from: Option<PathBuf>,
    checks: &[String],
    output_mode: OutputMode,
) -> Result<ExitCode> {
    let ReviewOptions {
        chunk: chunk_mode,
        max_concurrency,
    } = options;

    let file_paths: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
    let rules_config =
        RulesConfig::load(&config.working_dir).context("failed to load review rules")?;

    let piped_content = check_piped_stdin(from.as_ref(), diff.as_ref(), &files);

    // Gather context
    let context = gather_context(
        config,
        from.as_ref(),
        diff.as_ref(),
        &file_paths,
        piped_content,
    )
    .await?;
    let has_checks = !checks.is_empty();

    match (has_checks, chunk_mode) {
        (false, false) => {
            run_single_review_with_limits(
                config,
                &context,
                &rules_config,
                checks,
                &file_paths,
                output_mode,
            )
            .await
        }
        (false, true) => {
            let prompt = build_review_prompt(
                &rules_config
                    .resolve(checks, &file_paths)
                    .to_prompt_section(),
            );
            run_file_chunked_review(config, &context, &prompt, max_concurrency, output_mode).await
        }
        (true, false) => {
            run_category_review_checked(
                config,
                &context,
                &rules_config,
                checks,
                &file_paths,
                max_concurrency,
                output_mode,
            )
            .await
        }
        (true, true) => {
            run_nested_review_checked(
                config,
                &context,
                &rules_config,
                checks,
                &file_paths,
                max_concurrency,
                output_mode,
            )
            .await
        }
    }
}

/// Check for piped stdin, warning if explicit input takes precedence
fn check_piped_stdin(
    from: Option<&PathBuf>,
    diff: Option<&String>,
    files: &[String],
) -> Option<String> {
    let has_explicit = from.is_some() || diff.is_some() || !files.is_empty();
    if has_explicit {
        if stdin_is_piped() {
            tracing::warn!("stdin is piped but explicit input provided; stdin ignored");
        }
        None
    } else {
        try_read_piped_stdin()
    }
}

/// Gather context for pipeline review
async fn gather_context(
    config: &Config,
    from: Option<&PathBuf>,
    diff: Option<&String>,
    file_paths: &[PathBuf],
    piped_content: Option<String>,
) -> Result<GatheredContext> {
    let additional = match from {
        Some(path) => Some(read_from_source(path).await?),
        None => None,
    };

    let diff_only = diff.is_none() && file_paths.is_empty() && piped_content.is_none();

    let mut context = match piped_content {
        Some(content) => {
            tracing::debug!("using piped stdin as review context");
            GatheredContext {
                files: Vec::new(),
                git_diff: Some(content),
                git_status: None,
                additional_context: None,
            }
        }
        None => gather_review_context(&config.working_dir, diff_only, file_paths)
            .await
            .context("failed to gather review context")?,
    };

    context.additional_context = additional;
    Ok(context)
}

/// Run single review with token limit checks
async fn run_single_review_with_limits(
    config: &Config,
    context: &GatheredContext,
    rules_config: &RulesConfig,
    checks: &[String],
    file_paths: &[PathBuf],
    output_mode: OutputMode,
) -> Result<ExitCode> {
    let prompt = build_review_prompt(&rules_config.resolve(checks, file_paths).to_prompt_section());

    let token_count = count_context_tokens(context, config.tokenizer_mode)?;
    match check_token_limits(token_count, &TokenLimits::default()) {
        TokenCheck::Warning { message, .. } => eprintln!("warning: {message}"),
        TokenCheck::Error { message, .. } => return Err(PipelineError::TokenLimit(message).into()),
        TokenCheck::Ok(_) => {}
    }

    run_single_review(config, context, &prompt, output_mode).await
}

/// Run category review, checking for empty rules first
async fn run_category_review_checked(
    config: &Config,
    context: &GatheredContext,
    rules_config: &RulesConfig,
    checks: &[String],
    file_paths: &[PathBuf],
    max_concurrency: usize,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    let category_rules = rules_config.resolve_by_category(checks, file_paths);
    if category_rules.is_empty() {
        return Ok(output_no_rules_matched(output_mode));
    }
    run_category_review(
        config,
        context,
        &category_rules,
        max_concurrency,
        output_mode,
    )
    .await
}

/// Run nested review, checking for empty rules first
async fn run_nested_review_checked(
    config: &Config,
    context: &GatheredContext,
    rules_config: &RulesConfig,
    checks: &[String],
    file_paths: &[PathBuf],
    max_concurrency: usize,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    let category_rules = rules_config.resolve_by_category(checks, file_paths);
    if category_rules.is_empty() {
        return Ok(output_no_rules_matched(output_mode));
    }
    run_nested_review(
        config,
        context,
        &category_rules,
        max_concurrency,
        output_mode,
    )
    .await
}

/// Output result when no rules matched the specified checks
fn output_no_rules_matched(output_mode: OutputMode) -> ExitCode {
    tracing::warn!("no rules matched the specified checks");
    let result = ReviewResult {
        summary: "No rules matched the specified checks".to_string(),
        issues: Vec::new(),
        passed: true,
        parse_warning: None,
        raw_response: None,
    };
    println!("{}", result.render(output_mode));
    ExitCode::Success
}

/// Run a single review call with all rules
async fn run_single_review(
    config: &Config,
    context: &GatheredContext,
    system_prompt: &str,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    let response = run_pipeline(
        config,
        system_prompt,
        context,
        "Review these changes.",
        true, // enforce JSON output at API level
    )
    .await?;

    let result = parse_review_response(&response).unwrap_or_else(|e| ReviewResult {
        summary: String::new(),
        issues: Vec::new(),
        passed: false,
        parse_warning: Some(format!("could not parse structured response: {e}")),
        raw_response: Some(response.clone()),
    });

    let exit_code = result.exit_code();
    println!("{}", result.render(output_mode));
    Ok(exit_code)
}

/// Run review with file-based chunked execution
async fn run_file_chunked_review(
    config: &Config,
    context: &GatheredContext,
    system_prompt: &str,
    max_concurrency: usize,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    let chunks = chunk_by_file(context, config.tokenizer_mode)?;

    if chunks.is_empty() {
        eprintln!("warning: no chunks to process");
        let result = ReviewResult {
            summary: "No content to review".to_string(),
            issues: Vec::new(),
            passed: true,
            parse_warning: None,
            raw_response: None,
        };
        println!("{}", result.render(output_mode));
        return Ok(ExitCode::Success);
    }

    let options = ChunkOptions {
        max_concurrency,
        token_limits: TokenLimits::default(),
    };

    tracing::info!(
        chunks = chunks.len(),
        max_concurrency,
        "starting file-chunked review"
    );

    let prompt = system_prompt.to_string();
    let chunked_result = execute_chunked(config, chunks, &options, move |cfg, chunk| {
        let p = prompt.clone();
        async move { execute_review_chunk(&cfg, chunk.context, &p).await }
    })
    .await;

    let result = aggregate_review_results(chunked_result, false);
    let exit_code = result.exit_code();
    println!("{}", result.render(output_mode));
    Ok(exit_code)
}

/// Run review with category-based chunking (one LLM call per category)
async fn run_category_review(
    config: &Config,
    context: &GatheredContext,
    category_rules: &crate::rules::CategoryResolvedRules,
    max_concurrency: usize,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    let chunks = chunk_by_category(context, category_rules, config.tokenizer_mode)?;

    if chunks.is_empty() {
        let result = ReviewResult {
            summary: "No categories to review".to_string(),
            issues: Vec::new(),
            passed: true,
            parse_warning: None,
            raw_response: None,
        };
        println!("{}", result.render(output_mode));
        return Ok(ExitCode::Success);
    }

    let options = ChunkOptions {
        max_concurrency,
        token_limits: TokenLimits::default(),
    };

    tracing::info!(
        categories = chunks.len(),
        max_concurrency,
        "starting category-chunked review"
    );

    let chunked_result =
        execute_category_chunks(config, chunks, &options, |cfg, chunk| async move {
            execute_category_review_chunk(&cfg, chunk).await
        })
        .await;

    let result = aggregate_review_results(chunked_result, true);
    let exit_code = result.exit_code();
    println!("{}", result.render(output_mode));
    Ok(exit_code)
}

/// Run review with nested chunking (category × file)
async fn run_nested_review(
    config: &Config,
    context: &GatheredContext,
    category_rules: &crate::rules::CategoryResolvedRules,
    max_concurrency: usize,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    let chunks = chunk_by_category_and_file(context, category_rules, config.tokenizer_mode)?;

    if chunks.is_empty() {
        let result = ReviewResult {
            summary: "No content to review".to_string(),
            issues: Vec::new(),
            passed: true,
            parse_warning: None,
            raw_response: None,
        };
        println!("{}", result.render(output_mode));
        return Ok(ExitCode::Success);
    }

    let options = ChunkOptions {
        max_concurrency,
        token_limits: TokenLimits::default(),
    };

    tracing::info!(
        chunks = chunks.len(),
        max_concurrency,
        "starting nested (category × file) review"
    );

    let chunked_result =
        execute_category_chunks(config, chunks, &options, |cfg, chunk| async move {
            execute_category_review_chunk(&cfg, chunk).await
        })
        .await;

    let result = aggregate_review_results(chunked_result, true);
    let exit_code = result.exit_code();
    println!("{}", result.render(output_mode));
    Ok(exit_code)
}

/// Execute a review for a single file chunk
async fn execute_review_chunk(
    config: &Config,
    context: GatheredContext,
    system_prompt: &str,
) -> Result<ReviewResult> {
    let response = run_pipeline(
        config,
        system_prompt,
        &context,
        "Review these changes.",
        true,
    )
    .await
    .context("failed to execute review chunk")?;

    let result = parse_review_response(&response).unwrap_or_else(|e| ReviewResult {
        summary: String::new(),
        issues: Vec::new(),
        passed: false,
        parse_warning: Some(format!("could not parse structured response: {e}")),
        raw_response: Some(response.clone()),
    });

    Ok(result)
}

/// Execute a review for a single category chunk
async fn execute_category_review_chunk(
    config: &Config,
    chunk: CategoryChunk,
) -> Result<ReviewResult> {
    let rules_section = chunk.rules.to_category_prompt_section(&chunk.category);
    let system_prompt = build_category_review_prompt(&chunk.category, &rules_section);

    let response = run_pipeline(
        config,
        &system_prompt,
        &chunk.context,
        &format!("Review for {} issues.", chunk.category),
        true,
    )
    .await
    .context("failed to execute category review chunk")?;

    let result = parse_review_response(&response).unwrap_or_else(|e| ReviewResult {
        summary: String::new(),
        issues: Vec::new(),
        passed: false,
        parse_warning: Some(format!("could not parse structured response: {e}")),
        raw_response: Some(response.clone()),
    });

    Ok(result)
}

/// Aggregate results from chunked review execution
///
/// When `deduplicate` is true, removes duplicate issues (same file, line, message).
/// This is useful when category chunking may produce overlapping results.
fn aggregate_review_results(
    chunked: ChunkedResult<ReviewResult>,
    deduplicate: bool,
) -> ReviewResult {
    let mut all_issues: Vec<ReviewIssue> = Vec::new();
    let mut summaries: Vec<String> = Vec::new();
    let mut has_parse_warnings = false;
    let results_count = chunked.results.len();
    let failures_count = chunked.failures.len();

    // Collect results from successful chunks
    for result in chunked.results {
        if !result.summary.is_empty() {
            summaries.push(result.summary);
        }
        all_issues.extend(result.issues);
        if result.parse_warning.is_some() {
            has_parse_warnings = true;
        }
    }

    // Add failures as error issues
    for failure in &chunked.failures {
        all_issues.push(ReviewIssue {
            severity: "error".to_string(),
            file: None,
            line: None,
            message: format!("chunk {} failed: {}", failure.chunk_id, failure.error),
            rule: None,
        });
    }

    // Deduplicate issues if requested (for category chunking)
    if deduplicate {
        all_issues = deduplicate_issues(all_issues);
    }

    // Determine overall pass status
    let has_errors = all_issues
        .iter()
        .any(|i| i.severity == "error" || i.severity == "warning");
    let passed = !has_errors && failures_count == 0;

    // Build aggregated summary
    let summary = if summaries.is_empty() {
        format!(
            "Reviewed {} chunks ({} succeeded, {} failed)",
            chunked.total_chunks, results_count, failures_count
        )
    } else {
        format!(
            "Reviewed {} chunks:\n{}",
            chunked.total_chunks,
            summaries.join("\n")
        )
    };

    ReviewResult {
        summary,
        issues: all_issues,
        passed,
        parse_warning: if has_parse_warnings {
            Some("some chunks had parse warnings".to_string())
        } else {
            None
        },
        raw_response: None,
    }
}

/// Remove duplicate issues based on file, line, and message
fn deduplicate_issues(issues: Vec<ReviewIssue>) -> Vec<ReviewIssue> {
    let mut seen: HashSet<(Option<String>, Option<usize>, String)> = HashSet::new();
    let mut unique_issues = Vec::new();

    for issue in issues {
        let key = (issue.file.clone(), issue.line, issue.message.clone());
        if seen.insert(key) {
            unique_issues.push(issue);
        }
    }

    unique_issues
}
