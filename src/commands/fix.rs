//! The `fix` command implementation

use std::path::PathBuf;

use anyhow::{Context, Result};
use tokio::process::Command as TokioCommand;

use crate::cli::{run_agent, run_pipeline};
use crate::config::Config;
use crate::context::gather_fix_context;
use crate::input::{read_from_source, stdin_is_piped, try_read_piped_stdin};
use crate::output::{
    AppliedFix, ApplyResults, ApplyStatus, CommandOutput, ExitCode, ExitStatus, Fix, FixResult,
    OutputMode,
};
use crate::parsers::parse_fix_response;
use crate::prompts::{FIX_PROMPT, pipeline_prompt_for_command};

pub async fn cmd_fix(
    config: &Config,
    target: &str,
    lint: bool,
    apply: bool,
    agent_mode: bool,
    from: Option<PathBuf>,
    output_mode: OutputMode,
) -> Result<ExitCode> {
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

    if agent_mode {
        // Agent mode: use full agentic behavior with tool access
        let base_prompt = if lint {
            format!("Fix lint/clippy issues in: {target}. Run clippy first to identify issues.")
        } else {
            format!("Fix issues in: {target}")
        };

        // Instruct agent to respond with structured JSON matching pipeline output
        let prompt_with_json = format!(
            "{base_prompt}\n\n\
             When you have completed your fixes, provide your final response as JSON:\n\
             {{\"diagnosis\": \"description of issues found\", \"fixes\": [\
             {{\"file\": \"path\", \"line\": 42, \"original\": \"old code\", \"replacement\": \"new code\", \"explanation\": \"why\"}}], \
             \"unfixable_count\": 0}}"
        );

        let agent_result = run_agent(config, FIX_PROMPT, &prompt_with_json).await?;

        // Parse agent output with same function as pipeline mode
        let result = parse_fix_response(&agent_result.content).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "failed to parse agent fix response, returning raw content");
            FixResult {
                diagnosis: String::new(),
                fixes: Vec::new(),
                unfixable_count: 0,
                parse_warning: Some(format!("could not parse structured response: {e}")),
                raw_response: Some(agent_result.content.clone()),
            }
        });

        if apply && !result.fixes.is_empty() {
            let _ = apply_fixes(&config.working_dir, &result.fixes).await;
        }
        println!("{}", result.render(output_mode));
        Ok(result.exit_code())
    } else {
        // Pipeline mode: gather context and make single LLM call

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

        // Determine exit code: fail if there are unfixable issues OR apply failures
        let exit_code = if result.unfixable_count > 0 {
            ExitCode::IssuesFound
        } else if let Some(ref results) = apply_results {
            if results.all_succeeded() {
                ExitCode::Success
            } else {
                ExitCode::IssuesFound
            }
        } else {
            result.exit_code()
        };

        // Print the fix result
        println!("{}", result.render(output_mode));

        // Print apply results summary if fixes were applied
        if let Some(results) = apply_results {
            print_apply_results(&results, output_mode);
        }

        Ok(exit_code)
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
