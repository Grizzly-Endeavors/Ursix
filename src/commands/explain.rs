//! The `explain` command implementation

use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::cli::{run_agent, run_pipeline};
use crate::config::Config;
use crate::context::{GatheredContext, gather_explain_context};
use crate::input::try_read_piped_stdin;
use crate::output::{CommandOutput, ExitCode, ExplainResult, OutputMode};
use crate::parsers::parse_explain_response;
use crate::prompts::{EXPLAIN_PROMPT, pipeline_prompt_for_command};

pub async fn cmd_explain(
    config: &Config,
    target: &str,
    agent_mode: bool,
    output_mode: OutputMode,
    chunk_mode: bool,
) -> Result<ExitCode> {
    // Warn if --chunk is passed (not supported for explain)
    if chunk_mode {
        eprintln!("warning: --chunk is not supported for explain (requires full context)");
    }

    // Check for piped stdin - use as file content if present
    let piped_content = try_read_piped_stdin();

    if agent_mode {
        // Agent mode: use full agentic behavior for deep exploration
        // Instruct agent to respond with structured JSON matching pipeline output
        let prompt = format!(
            "Explain the code in: {target}\n\n\
             When you have completed your analysis, provide your final response as JSON:\n\
             {{\"explanation\": \"your detailed explanation here\"}}"
        );
        let agent_result = run_agent(config, EXPLAIN_PROMPT, &prompt).await?;

        // Parse agent output with same function as pipeline mode
        let result =
            parse_explain_response(&agent_result.content).unwrap_or_else(|e| ExplainResult {
                explanation: agent_result.content.clone(),
                parse_warning: Some(format!("could not parse structured response: {e}")),
            });

        println!("{}", result.render(output_mode));
    } else {
        // Pipeline mode: gather context and make single LLM call
        let context = if let Some(content) = piped_content {
            // Use piped stdin as file content
            tracing::debug!(target = %target, "using piped stdin as file content");
            GatheredContext {
                files: vec![crate::context::FileContext {
                    path: PathBuf::from(target),
                    content,
                }],
                git_diff: None,
                git_status: None,
                additional_context: None,
            }
        } else {
            // Read from file system
            let files = vec![PathBuf::from(target)];
            gather_explain_context(&config.working_dir, &files)
                .await
                .context("failed to gather explain context")?
        };

        let response = run_pipeline(
            config,
            pipeline_prompt_for_command("explain"),
            &context,
            &format!("Explain this code from {target}"),
            true, // enforce JSON output at API level
        )
        .await?;

        let result = parse_explain_response(&response).unwrap_or_else(|e| ExplainResult {
            explanation: response,
            parse_warning: Some(format!("could not parse structured response: {e}")),
        });

        println!("{}", result.render(output_mode));
    }

    Ok(ExitCode::Success)
}
