//! The `fix` command implementation

use std::path::PathBuf;

use anyhow::{Context, Result};
use tokio::process::Command as TokioCommand;

use crate::chunk::{ChunkOptions, ChunkedResult, chunk_by_file, execute_chunked};
use crate::cli::run_pipeline;
use crate::config::Config;
use crate::context::{GatheredContext, gather_fix_context};
use crate::input::{read_from_source, stdin_is_piped, try_read_piped_stdin};
use crate::output::{
    AppliedFix, ApplyResults, ApplyStatus, CommandOutput, ExitCode, ExitStatus, Fix, FixResult,
    OutputMode,
};
use crate::parsers::parse_fix_response;
use crate::pipeline::PipelineError;
use crate::prompts::pipeline_prompt_for_command;
use crate::tokens::{TokenCheck, TokenLimits, check_token_limits, count_context_tokens};

/// Options for the fix command
pub struct FixOptions {
    /// Fix lint/clippy issues
    pub lint: bool,
    /// Apply fixes automatically
    pub apply: bool,
    /// Enable chunked processing
    pub chunk: bool,
    /// Maximum concurrent chunk executions
    pub max_concurrency: usize,
}

pub async fn cmd_fix(
    config: &Config,
    target: &str,
    options: FixOptions,
    from: Option<PathBuf>,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    let FixOptions {
        lint,
        apply,
        chunk: chunk_mode,
        max_concurrency,
    } = options;
    // Check for piped stdin when no explicit --from is provided
    let piped_content = if from.is_some() {
        // Warn if stdin is piped but --from takes precedence
        if stdin_is_piped() {
            tracing::warn!("stdin is piped but --from provided; stdin ignored");
        }
        None
    } else {
        try_read_piped_stdin()
    };

    // Gather issues from: --from flag > piped stdin > --lint clippy
    let has_piped_input = piped_content.is_some();
    let issues = if let Some(ref path) = from {
        Some(read_from_source(path).await?)
    } else if let Some(content) = piped_content {
        tracing::debug!(target = %target, "using piped stdin as issues input");
        Some(content)
    } else if lint {
        // Run clippy to gather lint diagnostics
        Some(run_clippy_diagnostics(&config.working_dir, target).await?)
    } else {
        None
    };

    let files = vec![PathBuf::from(target)];
    let context = gather_fix_context(&config.working_dir, &files, issues.as_deref())
        .await
        .context("failed to gather fix context")?;

    // Check token limits (non-chunked mode)
    if !chunk_mode {
        let token_count = count_context_tokens(&context, config.tokenizer_mode)?;
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
        return run_chunked_fix(
            config,
            target,
            &context,
            apply,
            max_concurrency,
            output_mode,
        )
        .await;
    }

    let prompt = if lint || from.is_some() || has_piped_input {
        format!("Fix the issues shown above in {target}")
    } else {
        format!("Fix issues in {target}")
    };

    let response = run_pipeline(
        config,
        pipeline_prompt_for_command("fix"),
        &context,
        &prompt,
        true, // enforce JSON output at API level
    )
    .await?;

    let result = parse_fix_response(&response).unwrap_or_else(|e| {
        tracing::warn!(error = %e, "failed to parse fix response, returning empty result");
        FixResult {
            diagnosis: String::new(),
            fixes: Vec::new(),
            unfixable_count: 0,
            parse_warning: Some(format!("could not parse structured response: {e}")),
            raw_response: Some(response.clone()),
        }
    });

    // Apply fixes if requested and track results
    let apply_results = if apply && !result.fixes.is_empty() {
        Some(apply_fixes(&config.working_dir, &result.fixes).await)
    } else {
        None
    };

    let exit_code = determine_fix_exit_code(&result, apply_results.as_ref());

    // Print the fix result
    println!("{}", result.render(output_mode));

    // Print apply results summary if fixes were applied
    if let Some(results) = apply_results {
        print_apply_results(&results, output_mode);
    }

    Ok(exit_code)
}

/// Run fix with chunked execution
async fn run_chunked_fix(
    config: &Config,
    target: &str,
    context: &GatheredContext,
    apply: bool,
    max_concurrency: usize,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    let chunks = chunk_by_file(context, config.tokenizer_mode)?;

    if chunks.is_empty() {
        eprintln!("warning: no chunks to process");
        let result = FixResult {
            diagnosis: "No content to fix".to_string(),
            fixes: Vec::new(),
            unfixable_count: 0,
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
        "starting chunked fix"
    );

    let system_prompt = pipeline_prompt_for_command("fix").to_string();
    let target_str = target.to_string();

    let chunked_result = execute_chunked(config, chunks, &options, move |cfg, chunk| {
        let prompt = system_prompt.clone();
        let tgt = target_str.clone();
        async move { execute_fix_chunk(&cfg, chunk.context, &prompt, &tgt).await }
    })
    .await;

    let result = aggregate_fix_results(chunked_result);

    // Apply fixes if requested
    let apply_results = if apply && !result.fixes.is_empty() {
        Some(apply_fixes(&config.working_dir, &result.fixes).await)
    } else {
        None
    };

    // Determine exit code
    let exit_code = determine_fix_exit_code(&result, apply_results.as_ref());

    println!("{}", result.render(output_mode));

    if let Some(results) = apply_results {
        print_apply_results(&results, output_mode);
    }

    Ok(exit_code)
}

/// Determine exit code for fix result with optional apply results
fn determine_fix_exit_code(result: &FixResult, apply_results: Option<&ApplyResults>) -> ExitCode {
    if result.unfixable_count > 0 {
        ExitCode::IssuesFound
    } else if let Some(results) = apply_results {
        if results.all_succeeded() {
            ExitCode::Success
        } else {
            ExitCode::IssuesFound
        }
    } else {
        result.exit_code()
    }
}

/// Print apply results summary to stdout
fn print_apply_results(results: &ApplyResults, output_mode: OutputMode) {
    if output_mode == OutputMode::Human {
        println!();
        println!(
            "Applied {} of {} fixes ({} failed)",
            results.success_count,
            results.applied_fixes.len(),
            results.failure_count
        );
        for applied in &results.applied_fixes {
            if applied.status == ApplyStatus::Failed
                && let Some(ref err) = applied.error
            {
                println!("  - {}: {}", applied.fix.file, err);
            }
        }
    } else {
        // For JSON mode, print the apply results as JSON
        if let Ok(json) = serde_json::to_string_pretty(results) {
            println!("{json}");
        }
    }
}

/// Run clippy and capture diagnostics for context
async fn run_clippy_diagnostics(working_dir: &std::path::Path, target: &str) -> Result<String> {
    let output = TokioCommand::new("cargo")
        .args([
            "clippy",
            "--message-format=short",
            "--",
            "-W",
            "clippy::all",
        ])
        .current_dir(working_dir)
        .output()
        .await
        .context("failed to run cargo clippy")?;

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Filter to only include diagnostics related to the target file
    let target_path = std::path::Path::new(target);
    let relevant_lines: Vec<&str> = stderr
        .lines()
        .filter(|line| {
            // Include lines that reference the target file or are continuation lines
            line.contains(target)
                || target_path
                    .file_name()
                    .is_some_and(|name| line.contains(&name.to_string_lossy().to_string()))
                || line.starts_with("  ")
                || line.starts_with("   ")
        })
        .collect();

    if relevant_lines.is_empty() {
        Ok("No clippy issues found for this file.".to_string())
    } else {
        Ok(format!(
            "Clippy diagnostics:\n{}",
            relevant_lines.join("\n")
        ))
    }
}

/// Apply fixes to files by performing string replacements
///
/// Returns detailed results for each fix attempt, tracking which succeeded and which failed.
pub(crate) async fn apply_fixes(working_dir: &std::path::Path, fixes: &[Fix]) -> ApplyResults {
    use tokio::fs;

    let mut applied_fixes = Vec::with_capacity(fixes.len());
    let mut success_count = 0;
    let mut failure_count = 0;

    for fix in fixes {
        let file_path = if std::path::Path::new(&fix.file).is_absolute() {
            PathBuf::from(&fix.file)
        } else {
            working_dir.join(&fix.file)
        };

        // Read the file
        let content = match fs::read_to_string(&file_path).await {
            Ok(content) => content,
            Err(e) => {
                let error_msg = format!("failed to read file: {e}");
                tracing::warn!(file = %fix.file, error = %e, "failed to read file for fix");
                applied_fixes.push(AppliedFix {
                    fix: fix.clone(),
                    status: ApplyStatus::Failed,
                    error: Some(error_msg),
                });
                failure_count += 1;
                continue;
            }
        };

        // Check if the original code exists in the file
        if !content.contains(&fix.original) {
            let error_msg = "original code not found in file".to_string();
            tracing::warn!(
                file = %fix.file,
                original = %fix.original,
                "could not find original code to replace"
            );
            applied_fixes.push(AppliedFix {
                fix: fix.clone(),
                status: ApplyStatus::Failed,
                error: Some(error_msg),
            });
            failure_count += 1;
            continue;
        }

        // Apply the replacement
        let new_content = content.replacen(&fix.original, &fix.replacement, 1);
        match fs::write(&file_path, new_content).await {
            Ok(()) => {
                tracing::info!(file = %fix.file, "applied fix");
                applied_fixes.push(AppliedFix {
                    fix: fix.clone(),
                    status: ApplyStatus::Applied,
                    error: None,
                });
                success_count += 1;
            }
            Err(e) => {
                let error_msg = format!("failed to write file: {e}");
                tracing::warn!(file = %fix.file, error = %e, "failed to write fix to file");
                applied_fixes.push(AppliedFix {
                    fix: fix.clone(),
                    status: ApplyStatus::Failed,
                    error: Some(error_msg),
                });
                failure_count += 1;
            }
        }
    }

    ApplyResults {
        applied_fixes,
        success_count,
        failure_count,
    }
}

/// Execute a fix for a single chunk
async fn execute_fix_chunk(
    config: &Config,
    context: GatheredContext,
    system_prompt: &str,
    target: &str,
) -> Result<FixResult> {
    let prompt = format!("Fix issues in {target}");

    let response = run_pipeline(config, system_prompt, &context, &prompt, true)
        .await
        .context("failed to execute fix chunk")?;

    let result = parse_fix_response(&response).unwrap_or_else(|e| {
        tracing::warn!(error = %e, "failed to parse fix chunk response");
        FixResult {
            diagnosis: String::new(),
            fixes: Vec::new(),
            unfixable_count: 0,
            parse_warning: Some(format!("could not parse structured response: {e}")),
            raw_response: Some(response.clone()),
        }
    });

    Ok(result)
}

/// Aggregate results from chunked fix execution
fn aggregate_fix_results(chunked: ChunkedResult<FixResult>) -> FixResult {
    let mut all_fixes: Vec<Fix> = Vec::new();
    let mut diagnosis_parts: Vec<String> = Vec::new();
    let mut total_unfixable = 0;
    let mut has_parse_warnings = false;
    let results_count = chunked.results.len();
    let failure_count = chunked.failures.len();

    // Collect results from successful chunks
    for result in chunked.results {
        if !result.diagnosis.is_empty() {
            diagnosis_parts.push(result.diagnosis);
        }
        all_fixes.extend(result.fixes);
        total_unfixable += result.unfixable_count;
        if result.parse_warning.is_some() {
            has_parse_warnings = true;
        }
    }

    // Add failures to unfixable count
    total_unfixable += failure_count;

    // Build aggregated diagnosis
    let diagnosis = if diagnosis_parts.is_empty() {
        if failure_count > 0 {
            format!(
                "Processed {} chunks ({} succeeded, {} failed)",
                chunked.total_chunks, results_count, failure_count
            )
        } else {
            format!("Processed {} chunks", chunked.total_chunks)
        }
    } else {
        diagnosis_parts.join("\n")
    };

    let parse_warning = if has_parse_warnings || failure_count > 0 {
        let issues_desc = if has_parse_warnings {
            "some".to_string()
        } else {
            failure_count.to_string()
        };
        Some(format!("{issues_desc} chunks had issues"))
    } else {
        None
    };

    FixResult {
        diagnosis,
        fixes: all_fixes,
        unfixable_count: total_unfixable,
        parse_warning,
        raw_response: None,
    }
}
