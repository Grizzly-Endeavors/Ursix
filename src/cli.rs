use std::io::{self, Read};
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use thiserror::Error;
use tokio::process::Command as TokioCommand;

use crate::agent::{Agent, AgentError, AgentResult};
use crate::config::{Config, Provider};
use crate::context::{
    GatheredContext, gather_commit_context, gather_explain_context, gather_fix_context,
    gather_review_context,
};
use crate::llm::LlmClient;
use crate::llm::ollama::OllamaClient;
use crate::llm::openai::OpenAiClient;
use crate::output::{
    AppliedFix, ApplyResults, ApplyStatus, AskResult, CommandOutput, CommitResult, ConfigEntry,
    ConfigResult, ExitCode, ExitStatus, ExplainResult, Fix, FixResult, OutputMode, ReviewIssue,
    ReviewResult, ToExitCode,
};
use crate::pipeline::Pipeline;
use crate::pipeline::PipelineError;
use crate::prompts::{
    EXPLAIN_PROMPT, FIX_PROMPT, REVIEW_PROMPT, build_review_prompt, pipeline_prompt_for_command,
};
use crate::rules::RulesConfig;

#[derive(Parser, Debug)]
#[command(name = "usx")]
#[command(about = "Ursix - An extensible agentic CLI for LLM-powered development")]
#[command(version)]
pub struct Cli {
    /// Output as JSON for scripting
    #[arg(long, global = true)]
    pub json: bool,

    /// LLM provider (ollama, openai)
    #[arg(long, global = true)]
    pub provider: Option<Provider>,

    /// Model to use (overrides config)
    #[arg(short, long, global = true)]
    pub model: Option<String>,

    /// Ollama API base URL (overrides config)
    #[arg(long, global = true)]
    pub ollama_url: Option<String>,

    /// OpenAI-compatible API base URL (overrides config)
    #[arg(long, global = true)]
    pub openai_url: Option<String>,

    /// API key for OpenAI-compatible endpoints (overrides environment variable)
    #[arg(long, global = true, env = "URSIX_OPENAI_API_KEY")]
    pub openai_api_key: Option<String>,

    /// Maximum agent turns before stopping
    #[arg(long, global = true)]
    pub max_turns: Option<usize>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Explain code, files, or concepts
    Explain {
        /// File path or concept to explain
        target: String,

        /// Use agentic mode for deep exploration (default: pipeline mode)
        #[arg(long)]
        agent: bool,
    },

    /// Review code changes
    Review {
        /// Review specific git diff (e.g., HEAD~1, branch-name)
        #[arg(long)]
        diff: Option<String>,

        /// Review specific files
        files: Vec<String>,

        /// Use agentic mode for thorough multi-file analysis (default: pipeline mode)
        #[arg(long)]
        agent: bool,

        /// Read input from a file (use - for stdin)
        #[arg(long, value_name = "FILE")]
        from: Option<PathBuf>,

        /// Checks to perform (e.g., style, security, performance)
        #[arg(long, value_delimiter = ',')]
        checks: Vec<String>,
    },

    /// Fix issues in code
    Fix {
        /// Target file or directory
        target: String,

        /// Fix lint/clippy issues
        #[arg(long)]
        lint: bool,

        /// Apply fixes automatically (without confirmation)
        #[arg(long)]
        apply: bool,

        /// Use agentic mode with full tool access (default: pipeline mode)
        #[arg(long)]
        agent: bool,

        /// Read issues from a file (use - for stdin, e.g., from review --json output)
        #[arg(long, value_name = "FILE")]
        from: Option<PathBuf>,
    },

    /// Generate commit message from staged changes
    Commit {
        /// Include body with detailed explanation
        #[arg(long)]
        body: bool,

        /// Commit style (conventional, simple)
        #[arg(long, default_value = "conventional")]
        style: String,

        /// Auto-execute git commit with the generated message
        #[arg(long)]
        execute: bool,
    },

    /// View configuration values (edit .ursix.toml to change settings)
    Config {
        /// Configuration key to display
        key: Option<String>,

        /// List all configuration values
        #[arg(long)]
        list: bool,
    },
}

/// CLI-specific errors with appropriate exit codes
#[derive(Debug, Error)]
pub enum CliError {
    #[error("{0}")]
    Config(#[source] anyhow::Error),

    #[error("{0}")]
    Input(#[source] anyhow::Error),

    #[error("{0}")]
    Git(#[source] anyhow::Error),

    #[error("{0}")]
    Parse(#[source] anyhow::Error),

    #[error("{0}")]
    Pipeline(#[from] PipelineError),

    #[error("{0}")]
    Agent(#[from] AgentError),
}

impl ToExitCode for CliError {
    fn to_exit_code(&self) -> ExitCode {
        match self {
            Self::Config(_) => ExitCode::ConfigError,
            Self::Input(_) => ExitCode::InputError,
            Self::Git(_) => ExitCode::GitError,
            Self::Parse(_) => ExitCode::ParseError,
            Self::Pipeline(e) => e.to_exit_code(),
            Self::Agent(e) => e.to_exit_code(),
        }
    }
}

/// Run the CLI application
///
/// # Errors
/// Returns error if command execution fails or if working directory cannot be determined
pub async fn run() -> Result<ExitCode> {
    let cli = Cli::parse();

    let working_dir = std::env::current_dir().context("failed to get current directory")?;

    // Determine output mode
    let output_mode = if cli.json {
        OutputMode::Json
    } else {
        OutputMode::Human
    };

    // Load config from files and env vars, then apply CLI overrides
    let mut config = Config::load().context("failed to load configuration")?;
    config.working_dir = working_dir;

    // CLI flags override everything
    if let Some(provider) = cli.provider {
        config.provider = provider;
    }
    if let Some(ref model) = cli.model {
        config.model.clone_from(model);
    }
    if let Some(ref url) = cli.ollama_url {
        config.ollama_url.clone_from(url);
    }
    if let Some(ref url) = cli.openai_url {
        config.openai_url.clone_from(url);
    }
    if let Some(ref key) = cli.openai_api_key {
        config.openai_api_key = Some(key.clone());
    }
    if let Some(turns) = cli.max_turns {
        config.max_turns = turns;
    }

    match cli.command {
        Command::Explain { target, agent } => {
            cmd_explain(&config, &target, agent, output_mode).await
        }
        Command::Review {
            diff,
            files,
            agent,
            from,
            checks,
        } => cmd_review(&config, diff, files, agent, from, &checks, output_mode).await,
        Command::Fix {
            target,
            lint,
            apply,
            agent,
            from,
        } => cmd_fix(&config, &target, lint, apply, agent, from, output_mode).await,
        Command::Commit {
            body,
            style,
            execute,
        } => cmd_commit(&config, body, &style, execute, output_mode).await,
        Command::Config { key, list } => cmd_config(&config, key, list, output_mode),
    }
}

/// Check if a URL points to localhost
fn is_local_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.contains("localhost") || lower.contains("127.0.0.1") || lower.contains("[::1]")
}

/// Create an [`OpenAiClient`] from config, handling API key requirements
fn create_openai_client(config: &Config) -> Result<OpenAiClient> {
    if let Some(ref key) = config.openai_api_key {
        Ok(OpenAiClient::with_api_key(
            &config.openai_url,
            &config.model,
            key,
        ))
    } else if is_local_url(&config.openai_url) {
        Ok(OpenAiClient::new(&config.openai_url, &config.model))
    } else {
        Err(CliError::Config(anyhow::anyhow!(
            "OpenAI API key required for remote endpoints. \
             Set OPENAI_API_KEY environment variable or use --openai-api-key flag."
        ))
        .into())
    }
}

/// Run the agent with the appropriate provider
async fn run_agent(config: &Config, system_prompt: &str, user_prompt: &str) -> Result<AgentResult> {
    match config.provider {
        Provider::Ollama => {
            let client = OllamaClient::new(&config.ollama_url, &config.model);
            run_agent_with_client(config, client, system_prompt, user_prompt).await
        }
        Provider::OpenAi => {
            let client = create_openai_client(config)?;
            run_agent_with_client(config, client, system_prompt, user_prompt).await
        }
    }
}

/// Run the agent with a specific LLM client
async fn run_agent_with_client<C: LlmClient>(
    config: &Config,
    client: C,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<AgentResult> {
    let agent = Agent::new(config.clone(), client);
    let result = agent.run(system_prompt, user_prompt).await?;

    if result.interrupted {
        tracing::info!(
            turns = result.turns_completed,
            "agent was interrupted by signal"
        );
    }

    Ok(result)
}

/// Run the pipeline with the appropriate provider
async fn run_pipeline(
    config: &Config,
    system_prompt: &str,
    context: &GatheredContext,
    user_request: &str,
    json_mode: bool,
) -> Result<String> {
    match config.provider {
        Provider::Ollama => {
            let client = OllamaClient::new(&config.ollama_url, &config.model);
            run_pipeline_with_client(client, system_prompt, context, user_request, json_mode).await
        }
        Provider::OpenAi => {
            let client = create_openai_client(config)?;
            run_pipeline_with_client(client, system_prompt, context, user_request, json_mode).await
        }
    }
}

/// Run the pipeline with a specific LLM client
async fn run_pipeline_with_client<C: LlmClient>(
    client: C,
    system_prompt: &str,
    context: &GatheredContext,
    user_request: &str,
    json_mode: bool,
) -> Result<String> {
    let pipeline = Pipeline::new(client);
    pipeline
        .execute(system_prompt, context, user_request, json_mode)
        .await
        .map_err(Into::into)
}

/// Read content from stdin
fn read_stdin() -> Result<String> {
    let mut buffer = String::new();
    io::stdin()
        .read_to_string(&mut buffer)
        .context("failed to read from stdin")?;
    Ok(buffer)
}

/// Read content from a file or stdin (if path is "-")
async fn read_from_source(path: &PathBuf) -> Result<String> {
    if path.as_os_str() == "-" {
        read_stdin()
    } else {
        tokio::fs::read_to_string(path)
            .await
            .with_context(|| format!("failed to read from {}", path.display()))
    }
}

/// Extract JSON from a response that may contain additional text
fn extract_json(response: &str) -> &str {
    if let Some(start) = response.find('{')
        && let Some(end) = response.rfind('}')
        && end > start
    {
        return &response[start..=end];
    }
    response
}

/// Parse the LLM JSON response into a [`CommitResult`]
fn parse_commit_response(response: &str) -> Result<CommitResult> {
    #[derive(serde::Deserialize)]
    struct CommitJson {
        message: String,
        title: String,
        body: Option<String>,
    }

    let json_str = extract_json(response);
    let parsed: CommitJson = serde_json::from_str(json_str)
        .with_context(|| format!("failed to parse commit message JSON: {json_str}"))?;

    Ok(CommitResult {
        message: parsed.message,
        title: parsed.title,
        body: parsed.body,
    })
}

/// Parse the LLM JSON response into a [`ReviewResult`]
fn parse_review_response(response: &str) -> Result<ReviewResult> {
    #[derive(serde::Deserialize)]
    struct ReviewJson {
        summary: String,
        issues: Vec<ReviewIssueJson>,
    }

    #[derive(serde::Deserialize)]
    struct ReviewIssueJson {
        severity: String,
        file: Option<String>,
        line: Option<usize>,
        message: String,
        rule: Option<String>,
    }

    let json_str = extract_json(response);
    let parsed: ReviewJson = serde_json::from_str(json_str)
        .with_context(|| format!("failed to parse review JSON: {json_str}"))?;

    let issues: Vec<ReviewIssue> = parsed
        .issues
        .into_iter()
        .map(|i| ReviewIssue {
            severity: i.severity,
            file: i.file,
            line: i.line,
            message: i.message,
            rule: i.rule,
        })
        .collect();

    let review_passed = issues.is_empty()
        || !issues
            .iter()
            .any(|i| i.severity == "error" || i.severity == "warning");

    Ok(ReviewResult {
        summary: parsed.summary,
        issues,
        passed: review_passed,
        parse_warning: None,
        raw_response: None,
    })
}

/// Parse the LLM JSON response into an [`ExplainResult`]
fn parse_explain_response(response: &str) -> Result<ExplainResult> {
    #[derive(serde::Deserialize)]
    struct ExplainJson {
        explanation: String,
    }

    let json_str = extract_json(response);
    let parsed: ExplainJson = serde_json::from_str(json_str)
        .with_context(|| format!("failed to parse explain JSON: {json_str}"))?;

    Ok(ExplainResult {
        explanation: parsed.explanation,
        parse_warning: None,
    })
}

/// Parse the LLM JSON response into a [`FixResult`]
fn parse_fix_response(response: &str) -> Result<FixResult> {
    #[derive(serde::Deserialize)]
    struct FixJson {
        diagnosis: String,
        fixes: Vec<FixItemJson>,
        #[serde(default)]
        unfixable_count: usize,
    }

    #[derive(serde::Deserialize)]
    struct FixItemJson {
        file: String,
        line: Option<usize>,
        original: String,
        replacement: String,
        explanation: String,
    }

    let json_str = extract_json(response);
    let parsed: FixJson = serde_json::from_str(json_str)
        .with_context(|| format!("failed to parse fix JSON: {json_str}"))?;

    let fixes: Vec<Fix> = parsed
        .fixes
        .into_iter()
        .map(|f| Fix {
            file: f.file,
            line: f.line,
            original: f.original,
            replacement: f.replacement,
            explanation: f.explanation,
        })
        .collect();

    Ok(FixResult {
        diagnosis: parsed.diagnosis,
        fixes,
        unfixable_count: parsed.unfixable_count,
        parse_warning: None,
        raw_response: None,
    })
}

/// Execute git commit with the given message
///
/// Returns the git commit output on success for the caller to display.
async fn execute_git_commit(working_dir: &std::path::Path, message: &str) -> Result<String> {
    let output = TokioCommand::new("git")
        .args(["commit", "-m", message])
        .current_dir(working_dir)
        .output()
        .await
        .context("failed to execute git commit")?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(CliError::Git(anyhow::anyhow!("git commit failed: {}", stderr.trim())).into())
    }
}

async fn cmd_explain(
    config: &Config,
    target: &str,
    agent_mode: bool,
    output_mode: OutputMode,
) -> Result<ExitCode> {
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
        let files = vec![PathBuf::from(target)];
        let context = gather_explain_context(&config.working_dir, &files)
            .await
            .context("failed to gather explain context")?;

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

async fn cmd_review(
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

        let diff_only = diff.is_none() && files.is_empty();

        let mut context = gather_review_context(&config.working_dir, diff_only, &file_paths)
            .await
            .context("failed to gather review context")?;

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

async fn cmd_fix(
    config: &Config,
    target: &str,
    lint: bool,
    apply: bool,
    agent_mode: bool,
    from: Option<PathBuf>,
    output_mode: OutputMode,
) -> Result<ExitCode> {
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

        // Apply fixes if requested (agent mode supports --apply too)
        if apply && !result.fixes.is_empty() {
            let _apply_results = apply_fixes(&config.working_dir, &result.fixes).await;
        }

        let exit_code = result.exit_code();
        println!("{}", result.render(output_mode));
        Ok(exit_code)
    } else {
        // Pipeline mode: gather context and make single LLM call

        // Gather issues from --from flag or run clippy for --lint
        let issues = if let Some(ref path) = from {
            Some(read_from_source(path).await?)
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

        let prompt = if lint {
            format!("Fix the clippy/lint issues shown above in {target}")
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
async fn apply_fixes(working_dir: &std::path::Path, fixes: &[Fix]) -> ApplyResults {
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

async fn cmd_commit(
    config: &Config,
    body: bool,
    style: &str,
    execute: bool,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    // Commit is always pipeline mode - agent mode doesn't make sense for
    // a simple single-pass operation like generating a commit message
    let context = gather_commit_context(&config.working_dir)
        .await
        .context("failed to gather commit context")?;

    // Check if there are staged changes
    if context
        .git_diff
        .as_ref()
        .is_some_and(|diff| diff.trim().is_empty())
    {
        return Err(CliError::Git(anyhow::anyhow!("no staged changes to commit")).into());
    }

    let user_request = format!(
        "Generate a {} commit message{}.",
        style,
        if body {
            " with a detailed body explaining the changes"
        } else {
            ""
        }
    );

    let response = run_pipeline(
        config,
        pipeline_prompt_for_command("commit"),
        &context,
        &user_request,
        true, // enforce JSON output at API level
    )
    .await
    .context("failed to generate commit message")?;

    let result = parse_commit_response(&response)
        .context("failed to parse commit message from LLM response")?;

    println!("{}", result.render(output_mode));

    if execute {
        let git_output = execute_git_commit(&config.working_dir, &result.message).await?;
        if !git_output.is_empty() {
            println!("{git_output}");
        }
    }

    Ok(ExitCode::Success)
}

#[allow(clippy::unnecessary_wraps)]
fn cmd_config(
    config: &Config,
    key: Option<String>,
    list: bool,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    let api_key_display = if config.openai_api_key.is_some() {
        "[set]".to_string()
    } else {
        "[not set]".to_string()
    };

    if list {
        let result = ConfigResult {
            entries: vec![
                ConfigEntry {
                    key: "provider".to_string(),
                    value: config.provider.to_string(),
                },
                ConfigEntry {
                    key: "model".to_string(),
                    value: config.model.clone(),
                },
                ConfigEntry {
                    key: "ollama_url".to_string(),
                    value: config.ollama_url.clone(),
                },
                ConfigEntry {
                    key: "openai_url".to_string(),
                    value: config.openai_url.clone(),
                },
                ConfigEntry {
                    key: "openai_api_key".to_string(),
                    value: api_key_display.clone(),
                },
                ConfigEntry {
                    key: "max_turns".to_string(),
                    value: config.max_turns.to_string(),
                },
                ConfigEntry {
                    key: "working_dir".to_string(),
                    value: config.working_dir.display().to_string(),
                },
            ],
        };
        println!("{}", result.render(output_mode));
    } else if let Some(k) = key {
        let val = match k.as_str() {
            "provider" => config.provider.to_string(),
            "model" => config.model.clone(),
            "ollama_url" => config.ollama_url.clone(),
            "openai_url" => config.openai_url.clone(),
            "openai_api_key" => api_key_display,
            "max_turns" => config.max_turns.to_string(),
            "working_dir" => config.working_dir.display().to_string(),
            _ => format!("unknown config key: {k}"),
        };
        let result = ConfigResult {
            entries: vec![ConfigEntry { key: k, value: val }],
        };
        println!("{}", result.render(output_mode));
    } else {
        // Show usage help through the render system
        let help_text = "Usage: usx config <key> or usx config --list\n\
                         To change settings, edit .ursix.toml directly.";
        let result = AskResult {
            response: help_text.to_string(),
            turns: 0,
        };
        println!("{}", result.render(output_mode));
    }
    Ok(ExitCode::Success)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_simple() {
        let input = r#"{"message": "test", "title": "test", "body": null}"#;
        assert_eq!(extract_json(input), input);
    }

    #[test]
    fn test_extract_json_with_surrounding_text() {
        let input = r#"Here is the commit message:
{"message": "test", "title": "test", "body": null}
That's the JSON."#;
        let expected = r#"{"message": "test", "title": "test", "body": null}"#;
        assert_eq!(extract_json(input), expected);
    }

    #[test]
    fn test_extract_json_no_json() {
        let input = "No JSON here";
        assert_eq!(extract_json(input), input);
    }

    #[test]
    fn test_parse_commit_response_valid() {
        let input = r#"{"message": "feat: add new feature\n\nThis adds a cool feature.", "title": "feat: add new feature", "body": "This adds a cool feature."}"#;
        let result = parse_commit_response(input).unwrap();
        assert_eq!(result.title, "feat: add new feature");
        assert_eq!(result.body, Some("This adds a cool feature.".to_string()));
    }

    #[test]
    fn test_parse_commit_response_no_body() {
        let input = r#"{"message": "fix: typo", "title": "fix: typo", "body": null}"#;
        let result = parse_commit_response(input).unwrap();
        assert_eq!(result.title, "fix: typo");
        assert_eq!(result.body, None);
    }

    #[test]
    fn test_parse_commit_response_invalid() {
        let input = "not json at all";
        assert!(parse_commit_response(input).is_err());
    }

    #[test]
    fn test_parse_review_response_valid() {
        let input = r#"{"summary": "Code looks good", "issues": [{"severity": "warning", "file": "src/main.rs", "line": 10, "message": "Unused variable"}]}"#;
        let result = parse_review_response(input).unwrap();
        assert_eq!(result.summary, "Code looks good");
        assert_eq!(result.issues.len(), 1);
        assert!(!result.passed); // Has a warning
    }

    #[test]
    fn test_parse_review_response_no_issues() {
        let input = r#"{"summary": "Code looks perfect", "issues": []}"#;
        let result = parse_review_response(input).unwrap();
        assert!(result.passed);
        assert!(result.issues.is_empty());
    }

    #[test]
    fn test_parse_explain_response_valid() {
        let input = r#"{"explanation": "This function does..."}"#;
        let result = parse_explain_response(input).unwrap();
        assert_eq!(result.explanation, "This function does...");
    }

    #[test]
    fn test_parse_fix_response_valid() {
        let input = r#"{"diagnosis": "unused variable", "fixes": [{"file": "src/main.rs", "line": 10, "original": "let x = 1;", "replacement": "let _x = 1;", "explanation": "prefix unused variable with underscore"}], "unfixable_count": 0}"#;
        let result = parse_fix_response(input).unwrap();
        assert_eq!(result.diagnosis, "unused variable");
        assert_eq!(result.fixes.len(), 1);
        assert_eq!(result.fixes[0].file, "src/main.rs");
        assert_eq!(result.fixes[0].line, Some(10));
        assert_eq!(result.fixes[0].original, "let x = 1;");
        assert_eq!(result.fixes[0].replacement, "let _x = 1;");
        assert_eq!(
            result.fixes[0].explanation,
            "prefix unused variable with underscore"
        );
        assert_eq!(result.unfixable_count, 0);
    }

    #[test]
    fn test_parse_fix_response_no_line() {
        let input = r#"{"diagnosis": "issue found", "fixes": [{"file": "src/lib.rs", "line": null, "original": "foo()", "replacement": "bar()", "explanation": "renamed function"}], "unfixable_count": 1}"#;
        let result = parse_fix_response(input).unwrap();
        assert_eq!(result.fixes[0].line, None);
        assert_eq!(result.unfixable_count, 1);
    }

    #[test]
    fn test_parse_fix_response_empty_fixes() {
        let input = r#"{"diagnosis": "no issues found", "fixes": [], "unfixable_count": 0}"#;
        let result = parse_fix_response(input).unwrap();
        assert!(result.fixes.is_empty());
        assert_eq!(result.diagnosis, "no issues found");
    }

    #[test]
    fn test_cli_error_to_exit_code_config() {
        let err = CliError::Config(anyhow::anyhow!("invalid config"));
        assert_eq!(err.to_exit_code(), ExitCode::ConfigError);
    }

    #[test]
    fn test_cli_error_to_exit_code_input() {
        let err = CliError::Input(anyhow::anyhow!("file not found"));
        assert_eq!(err.to_exit_code(), ExitCode::InputError);
    }

    #[test]
    fn test_cli_error_to_exit_code_git() {
        let err = CliError::Git(anyhow::anyhow!("not a git repo"));
        assert_eq!(err.to_exit_code(), ExitCode::GitError);
    }

    #[test]
    fn test_cli_error_to_exit_code_parse() {
        let err = CliError::Parse(anyhow::anyhow!("invalid json"));
        assert_eq!(err.to_exit_code(), ExitCode::ParseError);
    }

    #[test]
    fn test_cli_error_to_exit_code_agent() {
        let err = CliError::Agent(AgentError::MaxTurnsExceeded(10));
        assert_eq!(err.to_exit_code(), ExitCode::AgentLimitError);
    }
}
