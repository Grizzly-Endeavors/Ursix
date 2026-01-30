//! The `fix` command implementation
//!
//! Transforms code snippets using structured input:
//! - User provides issue description, file path, and line range
//! - Snippet is inferred from file content at specified lines
//! - LLM performs semantic transformation only
//! - Diff is generated programmatically
//!
//! Supports two modes:
//! - Atomic (default): Fix a single issue
//! - Whole-file: Fix multiple issues in one file, processing bottom-to-top

mod diff;
mod input;
mod validation;

use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::cli::{FixMode, run_pipeline};
use crate::config::Config;
use crate::context::InputContext;
use crate::input::read_input;
use crate::output::{CommandOutput, ExitCode, OutputMode};
use crate::prompts::FIX_PIPELINE_PROMPT;

use diff::{apply_replacement, generate_unified_diff};
use input::{FixInput, ValidatedFixInput, ValidatedIssue, ValidatedWholeFileInput, WholeFileInput};
use validation::{sanitize_replacement, validate_replacement};

pub(crate) use crate::output::{FixError, FixResult, IssueFailure, WholeFileFixResult};

/// Options for the fix command
#[derive(Debug, Clone)]
pub(crate) struct FixOptions {
    /// Fix mode: atomic (single issue) or whole-file (multiple issues)
    pub mode: FixMode,
    /// Number of context lines in unified diff output
    pub context_lines: usize,
    /// Retry on validation failure with error context
    pub retry: bool,
    /// Return raw LLM output on validation failure
    pub partial: bool,
    /// Show dry-run information without LLM calls
    pub dry_run: bool,
}

/// Execute the fix command
///
/// # Errors
/// Returns error if input parsing, validation, or LLM call fails.
pub(crate) async fn cmd_fix(
    config: &Config,
    options: FixOptions,
    file: Option<PathBuf>,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    match options.mode {
        FixMode::Atomic => cmd_fix_atomic(config, &options, file, output_mode).await,
        FixMode::WholeFile => cmd_fix_whole_file(config, &options, file, output_mode).await,
    }
}

/// Execute the fix command in atomic mode (single issue)
async fn cmd_fix_atomic(
    config: &Config,
    options: &FixOptions,
    file: Option<PathBuf>,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    // Read input from stdin or file argument
    let raw_input = read_input(file.as_ref())
        .await
        .context("failed to read fix input")?;

    // Parse the JSON input
    let fix_input = match FixInput::parse(&raw_input) {
        Ok(input) => input,
        Err(e) => {
            let error = FixError::input_validation(e.to_string());
            println!("{}", error.render(output_mode));
            return Ok(error.code.to_exit_code());
        }
    };

    // Validate input against file system
    let validated = match fix_input.validate(&config.working_dir) {
        Ok(v) => v,
        Err(e) => {
            let error = FixError::input_validation(e.to_string());
            println!("{}", error.render(output_mode));
            return Ok(error.code.to_exit_code());
        }
    };

    // Dry-run mode: show what would be done without LLM call
    if options.dry_run {
        return handle_dry_run(&validated, output_mode);
    }

    // Execute the fix
    execute_fix(config, options, &validated, output_mode, None).await
}

/// Execute the fix command in whole-file mode (multiple issues)
async fn cmd_fix_whole_file(
    config: &Config,
    options: &FixOptions,
    file: Option<PathBuf>,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    // Read input from stdin or file argument
    let raw_input = read_input(file.as_ref())
        .await
        .context("failed to read fix input")?;

    // Parse the JSON input
    let whole_file_input = match WholeFileInput::parse(&raw_input) {
        Ok(input) => input,
        Err(e) => {
            let error = FixError::input_validation(e.to_string());
            println!("{}", error.render(output_mode));
            return Ok(error.code.to_exit_code());
        }
    };

    // Validate input against file system
    let validated = match whole_file_input.validate(&config.working_dir) {
        Ok(v) => v,
        Err(e) => {
            let error = FixError::input_validation(e.to_string());
            println!("{}", error.render(output_mode));
            return Ok(error.code.to_exit_code());
        }
    };

    // Dry-run mode: show what would be done without LLM call
    if options.dry_run {
        return handle_whole_file_dry_run(&validated, output_mode);
    }

    // Execute the whole-file fix
    execute_whole_file_fix(config, options, &validated, output_mode).await
}

/// Execute the actual fix operation
async fn execute_fix(
    config: &Config,
    options: &FixOptions,
    validated: &ValidatedFixInput,
    output_mode: OutputMode,
    retry_context: Option<&str>,
) -> Result<ExitCode> {
    // Build the user request with file path, language, and surrounding context
    let file_path = validated.relative_path(&config.working_dir);
    let user_request = build_fix_user_request(
        &file_path,
        &validated.file_content,
        &validated.snippet,
        &validated.issue,
        validated.lines,
        retry_context,
    );

    // Empty context — all context is in the user request to avoid duplication
    let ctx = InputContext::default();

    // Call the LLM (no JSON mode - we want raw code output)
    let llm_response = match run_pipeline(
        config,
        FIX_PIPELINE_PROMPT,
        &ctx,
        &user_request,
        false, // No JSON mode
    )
    .await
    {
        Ok(response) => response,
        Err(e) => {
            let error = FixError::llm_error(e.to_string());
            println!("{}", error.render(output_mode));
            return Ok(error.code.to_exit_code());
        }
    };

    // Sanitize and validate the LLM output
    let replacement = sanitize_replacement(&llm_response);

    // Validate the replacement
    let validation_result = match validate_replacement(&validated.snippet, &replacement) {
        Ok(result) => result,
        Err(e) => {
            // Validation failed - handle retry or partial output
            if options.retry && retry_context.is_none() {
                // First failure with retry enabled - try again with error context
                tracing::info!("validation failed, retrying with error context");
                let retry_ctx = format!("Previous attempt failed: {e}. Please fix the issue.");
                return Box::pin(execute_fix(
                    config,
                    options,
                    validated,
                    output_mode,
                    Some(&retry_ctx),
                ))
                .await;
            }

            let mut error = FixError::output_validation(e.to_string());
            if options.partial {
                error = error.with_raw_output(&llm_response);
            }
            println!("{}", error.render(output_mode));
            return Ok(error.code.to_exit_code());
        }
    };

    // Collect warnings
    let warnings: Vec<String> = validation_result
        .warnings
        .iter()
        .map(|w| w.message.clone())
        .collect();

    // Generate the diff
    let modified = apply_replacement(&validated.file_content, &replacement, validated.lines);
    let diff_result = generate_unified_diff(
        &validated.relative_path(&config.working_dir),
        &validated.file_content,
        &modified,
        options.context_lines,
    );

    // Build the result
    let result = FixResult {
        diff: diff_result.diff,
        file: validated.relative_path(&config.working_dir),
        lines: validated.lines,
        lines_added: diff_result.lines_added,
        lines_removed: diff_result.lines_removed,
        warnings,
    };

    println!("{}", result.render(output_mode));
    Ok(ExitCode::Success)
}

/// Infer programming language from file extension for code fence annotations
fn language_from_path(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("rs") => "rust",
        Some("py") => "python",
        Some("js") => "javascript",
        Some("ts") => "typescript",
        Some("tsx") => "tsx",
        Some("jsx") => "jsx",
        Some("go") => "go",
        Some("java") => "java",
        Some("c" | "h") => "c",
        Some("cpp" | "cc" | "cxx" | "hpp") => "cpp",
        Some("rb") => "ruby",
        Some("sh" | "bash") => "bash",
        Some("yml" | "yaml") => "yaml",
        Some("json") => "json",
        Some("toml") => "toml",
        Some("md") => "markdown",
        _ => "",
    }
}

/// Extract lines surrounding the target range for LLM context
fn extract_surrounding_context(content: &str, lines: (usize, usize), window: usize) -> String {
    let all_lines: Vec<&str> = content.lines().collect();
    let ctx_start = (lines.0 - 1).saturating_sub(window);
    let ctx_end = (lines.1 + window).min(all_lines.len());

    let ctx_lines = all_lines.get(ctx_start..ctx_end).unwrap_or(&[]);
    ctx_lines
        .iter()
        .enumerate()
        .map(|(i, line)| format!("{:>4}| {}", ctx_start + i + 1, line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Build the user request for the fix LLM call with full context
///
/// Includes file path, language-annotated code fences, surrounding context
/// with line numbers, and the specific snippet to fix.
fn build_fix_user_request(
    file_path: &str,
    file_content: &str,
    snippet: &str,
    issue: &str,
    lines: (usize, usize),
    retry_context: Option<&str>,
) -> String {
    let lang = language_from_path(file_path);
    let surrounding = extract_surrounding_context(file_content, lines, 15);

    let mut request = format!(
        "File: {file_path}\n\n\
         Issue: {issue}\n\n\
         Surrounding context (with line numbers):\n```{lang}\n{surrounding}\n```\n\n\
         Code to fix (lines {}-{}):\n```{lang}\n{snippet}\n```",
        lines.0, lines.1
    );

    if let Some(ctx) = retry_context {
        use std::fmt::Write;
        write!(request, "\n\n{ctx}").ok();
    }

    request
}

/// Handle dry-run mode for atomic fix
fn handle_dry_run(validated: &ValidatedFixInput, output_mode: OutputMode) -> Result<ExitCode> {
    use serde::Serialize;

    #[derive(Serialize)]
    struct DryRunOutput {
        file: String,
        lines: (usize, usize),
        snippet_length: usize,
        issue: String,
    }

    let output = DryRunOutput {
        file: validated.file_path.file_name().map_or_else(
            || validated.file_path.display().to_string(),
            |n| n.to_string_lossy().to_string(),
        ),
        lines: validated.lines,
        snippet_length: validated.snippet.len(),
        issue: validated.issue.clone(),
    };

    match output_mode {
        OutputMode::Human => {
            println!(
                "Dry run: would fix {} at lines {:?}",
                output.file, output.lines
            );
            println!("Issue: {}", output.issue);
            println!("Snippet length: {} chars", output.snippet_length);
        }
        OutputMode::Json => {
            let json = serde_json::to_string_pretty(&output)
                .context("failed to serialize dry-run output")?;
            println!("{json}");
        }
    }

    Ok(ExitCode::Success)
}

/// Handle dry-run mode for whole-file fix
fn handle_whole_file_dry_run(
    validated: &ValidatedWholeFileInput,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    use serde::Serialize;

    #[derive(Serialize)]
    struct IssueDryRun {
        issue: String,
        lines: (usize, usize),
        snippet_length: usize,
    }

    #[derive(Serialize)]
    struct DryRunOutput {
        file: String,
        issue_count: usize,
        issues: Vec<IssueDryRun>,
    }

    let issues: Vec<IssueDryRun> = validated
        .issues
        .iter()
        .map(|i| IssueDryRun {
            issue: i.issue.clone(),
            lines: i.lines,
            snippet_length: i.snippet.len(),
        })
        .collect();

    let output = DryRunOutput {
        file: validated.file_path.file_name().map_or_else(
            || validated.file_path.display().to_string(),
            |n| n.to_string_lossy().to_string(),
        ),
        issue_count: validated.issues.len(),
        issues,
    };

    match output_mode {
        OutputMode::Human => {
            println!(
                "Dry run: would fix {} issues in {}",
                output.issue_count, output.file
            );
            for (i, issue) in output.issues.iter().enumerate() {
                println!(
                    "  {}. lines {:?}: {} ({} chars)",
                    i + 1,
                    issue.lines,
                    issue.issue,
                    issue.snippet_length
                );
            }
        }
        OutputMode::Json => {
            let json = serde_json::to_string_pretty(&output)
                .context("failed to serialize dry-run output")?;
            println!("{json}");
        }
    }

    Ok(ExitCode::Success)
}

/// Execute whole-file fix: process multiple issues bottom-to-top
async fn execute_whole_file_fix(
    config: &Config,
    options: &FixOptions,
    validated: &ValidatedWholeFileInput,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    let mut current_content = validated.file_content.clone();
    let file_path = validated.relative_path(&config.working_dir);
    let mut issues_fixed = 0;
    let mut issue_failures: Vec<IssueFailure> = Vec::new();
    let mut total_lines_added: usize = 0;
    let mut total_lines_removed: usize = 0;

    // Process issues in order (already sorted descending by line number)
    for issue in &validated.issues {
        match execute_single_issue_fix(config, options, issue, &current_content, &file_path).await {
            Ok((new_content, lines_added, lines_removed)) => {
                current_content = new_content;
                issues_fixed += 1;
                total_lines_added += lines_added;
                total_lines_removed += lines_removed;
            }
            Err(e) => {
                if options.partial {
                    // Record failure and continue
                    issue_failures.push(IssueFailure {
                        issue: issue.issue.clone(),
                        lines: issue.lines,
                        error: e,
                    });
                } else {
                    // Fail fast
                    let error = FixError::output_validation(format!(
                        "failed to fix issue at lines {}..{}: {e}",
                        issue.lines.0, issue.lines.1
                    ));
                    println!("{}", error.render(output_mode));
                    return Ok(error.code.to_exit_code());
                }
            }
        }
    }

    // Generate combined diff
    let diff_result = generate_unified_diff(
        &validated.relative_path(&config.working_dir),
        &validated.file_content,
        &current_content,
        options.context_lines,
    );

    let result = WholeFileFixResult {
        diff: diff_result.diff,
        file: validated.relative_path(&config.working_dir),
        issues_fixed,
        issues_failed: issue_failures.len(),
        lines_added: total_lines_added,
        lines_removed: total_lines_removed,
        issue_failures,
    };

    println!("{}", result.render(output_mode));

    if result.issues_failed > 0 && issues_fixed == 0 {
        Ok(ExitCode::PermanentError)
    } else {
        Ok(ExitCode::Success)
    }
}

/// Execute a single issue fix and return the new content
///
/// Returns `(new_content, lines_added, lines_removed)` on success.
async fn execute_single_issue_fix(
    config: &Config,
    options: &FixOptions,
    issue: &ValidatedIssue,
    file_content: &str,
    file_path: &str,
) -> Result<(String, usize, usize), String> {
    // Build the user request with file path, language, and surrounding context
    let user_request = build_fix_user_request(
        file_path,
        file_content,
        &issue.snippet,
        &issue.issue,
        issue.lines,
        None,
    );

    // Empty context — all context is in the user request to avoid duplication
    let ctx = InputContext::default();

    // Call the LLM
    let llm_response = run_pipeline(config, FIX_PIPELINE_PROMPT, &ctx, &user_request, false)
        .await
        .map_err(|e| format!("LLM call failed: {e}"))?;

    // Sanitize and validate the LLM output
    let replacement = sanitize_replacement(&llm_response);

    // Validate the replacement
    let validation_result = validate_replacement(&issue.snippet, &replacement)
        .map_err(|e| format!("validation failed: {e}"))?;

    // Log warnings but don't fail
    for warning in &validation_result.warnings {
        tracing::warn!(
            lines = ?issue.lines,
            warning = %warning.message,
            "fix validation warning"
        );
    }

    // Apply replacement and count changes
    let modified = apply_replacement(file_content, &replacement, issue.lines);

    // Count line changes
    let original_lines = file_content.lines().count();
    let modified_lines = modified.lines().count();
    let (lines_added, lines_removed) = if modified_lines >= original_lines {
        (modified_lines - original_lines, 0)
    } else {
        (0, original_lines - modified_lines)
    };

    // Retry logic if enabled
    if options.retry {
        // For whole-file mode, we don't do inline retry - just fail the issue
        // The user can re-run with --partial to get partial results
    }

    Ok((modified, lines_added, lines_removed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_fix_user_request_includes_file_and_language() {
        let request = build_fix_user_request(
            "src/main.rs",
            "fn main() {\n    let x = 1;\n    println!(\"{}\", x);\n}",
            "    let x = 1;",
            "unused variable",
            (2, 2),
            None,
        );
        assert!(request.contains("File: src/main.rs"));
        assert!(request.contains("Issue: unused variable"));
        assert!(request.contains("```rust"));
        assert!(request.contains("let x = 1;"));
        // Surrounding context should include adjacent lines
        assert!(request.contains("fn main()"));
    }

    #[test]
    fn test_build_fix_user_request_with_retry_context() {
        let request = build_fix_user_request(
            "test.rs",
            "code",
            "code",
            "test issue",
            (1, 1),
            Some("Previous attempt failed"),
        );
        assert!(request.contains("Previous attempt failed"));
        assert!(request.contains("File: test.rs"));
    }

    #[test]
    fn test_language_from_path() {
        assert_eq!(language_from_path("src/main.rs"), "rust");
        assert_eq!(language_from_path("app.py"), "python");
        assert_eq!(language_from_path("index.js"), "javascript");
        assert_eq!(language_from_path("config.toml"), "toml");
        assert_eq!(language_from_path("unknown.xyz"), "");
    }

    #[test]
    fn test_extract_surrounding_context_includes_window() {
        let content = "line 1\nline 2\nline 3\nline 4\nline 5";
        let ctx = extract_surrounding_context(content, (3, 3), 1);
        assert!(ctx.contains("line 2"));
        assert!(ctx.contains("line 3"));
        assert!(ctx.contains("line 4"));
        // Line numbers should be present
        assert!(ctx.contains("   2| "));
        assert!(ctx.contains("   3| "));
        assert!(ctx.contains("   4| "));
    }

    #[test]
    fn test_extract_surrounding_context_clamps_to_bounds() {
        let content = "line 1\nline 2\nline 3";
        let ctx = extract_surrounding_context(content, (1, 1), 10);
        // Should include all lines without panicking
        assert!(ctx.contains("line 1"));
        assert!(ctx.contains("line 2"));
        assert!(ctx.contains("line 3"));
    }
}
