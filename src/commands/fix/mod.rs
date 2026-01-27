//! The `fix` command implementation
//!
//! Transforms code snippets using structured input:
//! - User provides issue description, snippet, file path, and line range
//! - LLM performs semantic transformation only
//! - Diff is generated programmatically

mod diff;
mod input;
mod validation;

use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::cli::run_pipeline;
use crate::config::Config;
use crate::context::InputContext;
use crate::input::read_input;
use crate::output::{CommandOutput, ExitCode, OutputMode};
use crate::prompts::FIX_PIPELINE_PROMPT;

use diff::{apply_replacement, generate_unified_diff};
use input::{FixInput, ValidatedFixInput};
use validation::{sanitize_replacement, validate_replacement};

pub use crate::output::{FixError, FixResult};

/// Options for the fix command
#[derive(Debug, Clone)]
pub struct FixOptions {
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
pub async fn cmd_fix(
    config: &Config,
    options: FixOptions,
    from: Option<PathBuf>,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    // Read input from stdin or --from
    let raw_input = read_input(from.as_ref())
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
    execute_fix(config, &options, &validated, output_mode, None).await
}

/// Execute the actual fix operation
async fn execute_fix(
    config: &Config,
    options: &FixOptions,
    validated: &ValidatedFixInput,
    output_mode: OutputMode,
    retry_context: Option<&str>,
) -> Result<ExitCode> {
    // Build the user request
    let user_request = build_user_request(validated, retry_context);

    // Build context with the snippet
    let ctx = InputContext::new(&validated.snippet);

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

/// Build the user request for the LLM
fn build_user_request(validated: &ValidatedFixInput, retry_context: Option<&str>) -> String {
    let mut request = format!(
        "Issue: {}\n\nCode to fix:\n```\n{}\n```",
        validated.issue, validated.snippet
    );

    if let Some(ctx) = retry_context {
        use std::fmt::Write;
        let _ = write!(request, "\n\n{ctx}");
    }

    request
}

/// Handle dry-run mode
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_build_user_request_basic() {
        let validated = ValidatedFixInput {
            issue: "unused variable".to_string(),
            snippet: "let x = 1;".to_string(),
            file_path: PathBuf::from("/tmp/test.rs"),
            file_content: "let x = 1;".to_string(),
            lines: (1, 1),
        };

        let request = build_user_request(&validated, None);
        assert!(request.contains("Issue: unused variable"));
        assert!(request.contains("let x = 1;"));
    }

    #[test]
    fn test_build_user_request_with_retry_context() {
        let validated = ValidatedFixInput {
            issue: "test".to_string(),
            snippet: "code".to_string(),
            file_path: PathBuf::from("/tmp/test.rs"),
            file_content: "code".to_string(),
            lines: (1, 1),
        };

        let request = build_user_request(&validated, Some("Previous attempt failed"));
        assert!(request.contains("Previous attempt failed"));
    }
}
