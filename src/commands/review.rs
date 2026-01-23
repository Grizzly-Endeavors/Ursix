//! The `review` command implementation

use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::chunk::{ChunkOptions, ChunkedResult, chunk_by_file, execute_chunked};
use crate::cli::{run_agent, run_pipeline};
use crate::config::Config;
use crate::context::{GatheredContext, gather_review_context};
use crate::input::{read_from_source, stdin_is_piped, try_read_piped_stdin};
use crate::output::{CommandOutput, ExitCode, ExitStatus, OutputMode, ReviewIssue, ReviewResult};
use crate::parsers::parse_review_response;
use crate::pipeline::PipelineError;
use crate::prompts::{REVIEW_PROMPT, build_review_prompt};
use crate::rules::RulesConfig;
use crate::tokens::{TokenCheck, TokenLimits, check_token_limits, count_context_tokens};

/// Options for the review command
pub struct ReviewOptions {
    /// Use agentic mode
    pub agent: bool,
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
        agent: agent_mode,
        chunk: chunk_mode,
        max_concurrency,
    } = options;
    // Load and resolve review rules
    let file_paths: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
    let rules_config =
        RulesConfig::load(&config.working_dir).context("failed to load review rules")?;
    let resolved_rules = rules_config.resolve(checks, &file_paths);
    let rules_section = resolved_rules.to_prompt_section();

    // Check if any explicit input is provided
    let has_explicit_input = from.is_some() || diff.is_some() || !files.is_empty();

    // Check for piped stdin
    let piped_content = if has_explicit_input {
        // Warn if stdin is piped but explicit input takes precedence
        if stdin_is_piped() {
            tracing::warn!(
                "stdin is piped but explicit input provided (--from, --diff, or files); stdin ignored"
            );
        }
        None
    } else {
        try_read_piped_stdin()
    };

    if agent_mode {
        return run_review_agent(config, diff.as_ref(), &files, &rules_section, output_mode).await;
    }

    // Pipeline mode: gather context and make single LLM call
    let additional_context = if let Some(ref path) = from {
        Some(read_from_source(path).await?)
    } else {
        None
    };

    let diff_only = diff.is_none() && files.is_empty() && piped_content.is_none();

    let mut context = if let Some(content) = piped_content {
        // Use piped stdin as diff content
        tracing::debug!("using piped stdin as review context");
        GatheredContext {
            files: Vec::new(),
            git_diff: Some(content),
            git_status: None,
            additional_context: None,
        }
    } else {
        gather_review_context(&config.working_dir, diff_only, &file_paths)
            .await
            .context("failed to gather review context")?
    };

    context.additional_context = additional_context;

    // Build prompt with injected rules
    let system_prompt = build_review_prompt(&rules_section);

    // Check token limits (non-chunked mode)
    if !chunk_mode {
        let token_count = count_context_tokens(&context)?;
        match check_token_limits(token_count, &TokenLimits::default()) {
            TokenCheck::Warning { message, .. } => {
                eprintln!("warning: {message}");
            }
            TokenCheck::Error { message, .. } => {
                return Err(PipelineError::TokenLimit(message).into());
            }
            TokenCheck::Ok(_) => {}
        }
    }

    // Chunked mode: process files in parallel
    if chunk_mode && !context.files.is_empty() {
        return run_chunked_review(
            config,
            &context,
            &system_prompt,
            max_concurrency,
            output_mode,
        )
        .await;
    }

    let response = run_pipeline(
        config,
        &system_prompt,
        &context,
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

/// Run review with chunked execution
async fn run_chunked_review(
    config: &Config,
    context: &GatheredContext,
    system_prompt: &str,
    max_concurrency: usize,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    let chunks = chunk_by_file(context)?;

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
        "starting chunked review"
    );

    let prompt = system_prompt.to_string();
    let chunked_result = execute_chunked(config, chunks, &options, move |cfg, chunk| {
        let p = prompt.clone();
        async move { execute_review_chunk(&cfg, chunk.context, &p).await }
    })
    .await;

    let result = aggregate_review_results(chunked_result);
    let exit_code = result.exit_code();
    println!("{}", result.render(output_mode));
    Ok(exit_code)
}

/// Run review in agent mode
async fn run_review_agent(
    config: &Config,
    diff: Option<&String>,
    files: &[String],
    rules_section: &str,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    let base_prompt = if let Some(d) = diff {
        format!("Review the changes in git diff {d}")
    } else if !files.is_empty() {
        format!("Review the code in: {}", files.join(", "))
    } else {
        "Review the staged changes (use git diff --cached)".to_string()
    };

    let system_prompt = if rules_section.is_empty() {
        REVIEW_PROMPT.to_string()
    } else {
        format!("{REVIEW_PROMPT}\n{rules_section}")
    };

    let prompt_with_json = format!(
        "{base_prompt}\n\n\
         When you have completed your review, provide your final response as JSON:\n\
         {{\"summary\": \"brief summary\", \"issues\": [\
         {{\"severity\": \"error|warning|info\", \"file\": \"path\", \"line\": 42, \"message\": \"description\"}}]}}"
    );

    let agent_result = run_agent(config, &system_prompt, &prompt_with_json).await?;

    let result = parse_review_response(&agent_result.content).unwrap_or_else(|e| ReviewResult {
        summary: String::new(),
        issues: Vec::new(),
        passed: false,
        parse_warning: Some(format!("could not parse structured response: {e}")),
        raw_response: Some(agent_result.content.clone()),
    });

    let exit_code = result.exit_code();
    println!("{}", result.render(output_mode));
    Ok(exit_code)
}

/// Execute a review for a single chunk
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

/// Aggregate results from chunked review execution
fn aggregate_review_results(chunked: ChunkedResult<ReviewResult>) -> ReviewResult {
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
