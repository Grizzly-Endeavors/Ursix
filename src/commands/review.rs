//! The `review` command implementation

use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::cli::{run_agent, run_pipeline};
use crate::config::Config;
use crate::context::{GatheredContext, gather_review_context};
use crate::input::{read_from_source, stdin_is_piped, try_read_piped_stdin};
use crate::output::{CommandOutput, ExitCode, ExitStatus, OutputMode, ReviewResult};
use crate::parsers::parse_review_response;
use crate::prompts::{REVIEW_PROMPT, build_review_prompt};
use crate::rules::RulesConfig;

pub async fn cmd_review(
    config: &Config,
    diff: Option<String>,
    files: Vec<String>,
    agent_mode: bool,
    from: Option<PathBuf>,
    checks: &[String],
    output_mode: OutputMode,
) -> Result<ExitCode> {
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
        // Agent mode: use full agentic behavior for thorough analysis
        let base_prompt = if let Some(ref d) = diff {
            format!("Review the changes in git diff {d}")
        } else if !files.is_empty() {
            format!("Review the code in: {}", files.join(", "))
        } else {
            "Review the staged changes (use git diff --cached)".to_string()
        };

        // Append rules to the system prompt for agent mode
        let system_prompt = if rules_section.is_empty() {
            REVIEW_PROMPT.to_string()
        } else {
            format!("{REVIEW_PROMPT}\n{rules_section}")
        };

        // Instruct agent to respond with structured JSON matching pipeline output
        let prompt_with_json = format!(
            "{base_prompt}\n\n\
             When you have completed your review, provide your final response as JSON:\n\
             {{\"summary\": \"brief summary\", \"issues\": [\
             {{\"severity\": \"error|warning|info\", \"file\": \"path\", \"line\": 42, \"message\": \"description\"}}]}}"
        );

        let agent_result = run_agent(config, &system_prompt, &prompt_with_json).await?;

        // Parse agent output with same function as pipeline mode
        let result =
            parse_review_response(&agent_result.content).unwrap_or_else(|e| ReviewResult {
                summary: String::new(),
                issues: Vec::new(),
                passed: false,
                parse_warning: Some(format!("could not parse structured response: {e}")),
                raw_response: Some(agent_result.content.clone()),
            });

        let exit_code = result.exit_code();
        println!("{}", result.render(output_mode));
        Ok(exit_code)
    } else {
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
}
