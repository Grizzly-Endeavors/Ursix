use std::io::{self, Read};
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tokio::process::Command as TokioCommand;

use crate::agent::Agent;
use crate::config::{Config, Provider};
use crate::context::{
    GatheredContext, gather_commit_context, gather_explain_context, gather_fix_context,
    gather_review_context,
};
use crate::llm::LlmClient;
use crate::llm::ollama::OllamaClient;
use crate::llm::openai::OpenAiClient;
use crate::output::{
    AskResult, CommandOutput, CommitResult, ConfigEntry, ConfigResult, ExitCode, ExitStatus,
    ExplainResult, Fix, FixResult, ModelsResult, OutputMode, ReviewIssue, ReviewResult,
};
use crate::pipeline::Pipeline;
use crate::prompts::{self, build_review_prompt, pipeline_prompt_for_command};
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

    /// Enable verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// General-purpose LLM query with tool access
    Ask {
        /// The prompt to send to the LLM
        #[arg(required = true)]
        prompt: Vec<String>,

        /// Use agentic mode with tool access (default: pipeline mode)
        #[arg(long)]
        agent: bool,

        /// Read context from a file (use - for stdin)
        #[arg(long, value_name = "FILE")]
        from: Option<PathBuf>,

        /// Read context from stdin
        #[arg(long)]
        stdin: bool,
    },

    /// Explain code, files, or concepts
    Explain {
        /// File path or concept to explain
        target: String,

        /// Include surrounding context lines
        #[arg(short, long)]
        context: Option<usize>,

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

        /// Use agentic mode with tool access (default: pipeline mode)
        #[arg(long)]
        agent: bool,

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

    /// List available Ollama models
    Models,
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
        Command::Ask {
            prompt,
            agent,
            from,
            stdin,
        } => cmd_ask(&config, prompt, agent, from, stdin, output_mode).await,
        Command::Explain {
            target,
            context,
            agent,
        } => cmd_explain(&config, &target, context, agent, output_mode).await,
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
            agent,
            execute,
        } => cmd_commit(&config, body, &style, agent, execute, output_mode).await,
        Command::Config { key, list } => cmd_config(&config, key, list, output_mode),
        Command::Models => cmd_models(&config, output_mode).await,
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
        anyhow::bail!(
            "OpenAI API key required for remote endpoints. \
             Set OPENAI_API_KEY environment variable or use --openai-api-key flag."
        )
    }
}

/// Run the agent with the appropriate provider
async fn run_agent(config: &Config, system_prompt: &str, user_prompt: &str) -> Result<String> {
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
) -> Result<String> {
    let agent = Agent::new(config.clone(), client);
    agent
        .run(system_prompt, user_prompt)
        .await
        .map_err(Into::into)
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
    })
}

/// Execute git commit with the given message
async fn execute_git_commit(working_dir: &std::path::Path, message: &str) -> Result<()> {
    let output = TokioCommand::new("git")
        .args(["commit", "-m", message])
        .current_dir(working_dir)
        .output()
        .await
        .context("failed to execute git commit")?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.is_empty() {
            println!("{stdout}");
        }
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git commit failed: {}", stderr.trim())
    }
}

async fn cmd_ask(
    config: &Config,
    prompt: Vec<String>,
    agent_mode: bool,
    from: Option<PathBuf>,
    stdin: bool,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    let prompt_text = prompt.join(" ");

    if agent_mode {
        // Agent mode: use full agentic behavior with tools
        let response = run_agent(config, prompts::ASK_PROMPT, &prompt_text).await?;
        let result = AskResult { response, turns: 1 };
        println!("{}", result.render(output_mode));
    } else {
        // Pipeline mode: single LLM call with optional context
        let additional_context = if stdin {
            Some(read_stdin()?)
        } else if let Some(ref path) = from {
            Some(read_from_source(path).await?)
        } else {
            None
        };

        let context = GatheredContext {
            files: Vec::new(),
            git_diff: None,
            git_status: None,
            additional_context,
        };

        // "ask" command returns plain text, not JSON
        let response = run_pipeline(
            config,
            pipeline_prompt_for_command("ask"),
            &context,
            &prompt_text,
            false,
        )
        .await?;

        let result = AskResult { response, turns: 1 };
        println!("{}", result.render(output_mode));
    }

    Ok(ExitCode::Success)
}

async fn cmd_explain(
    config: &Config,
    target: &str,
    _context_lines: Option<usize>,
    agent_mode: bool,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    if agent_mode {
        // Agent mode: use full agentic behavior for deep exploration
        let prompt = format!("Explain the code in: {target}");
        let response = run_agent(config, prompts::EXPLAIN_PROMPT, &prompt).await?;
        let result = AskResult { response, turns: 1 };
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

        let result = parse_explain_response(&response).unwrap_or(ExplainResult {
            explanation: response,
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
            prompts::REVIEW_PROMPT.to_string()
        } else {
            format!("{}\n{rules_section}", prompts::REVIEW_PROMPT)
        };

        let response = run_agent(config, &system_prompt, &base_prompt).await?;
        let result = AskResult { response, turns: 1 };
        println!("{}", result.render(output_mode));
        Ok(ExitCode::Success)
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

        let result = parse_review_response(&response).unwrap_or_else(|_| ReviewResult {
            summary: response.clone(),
            issues: Vec::new(),
            passed: true,
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
        let prompt = if lint {
            format!("Fix lint/clippy issues in: {target}. Run clippy first to identify issues.")
        } else {
            format!("Fix issues in: {target}")
        };
        let response = run_agent(config, prompts::FIX_PROMPT, &prompt).await?;
        let result = AskResult { response, turns: 1 };
        println!("{}", result.render(output_mode));
        Ok(ExitCode::Success)
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
                diagnosis: "failed to parse LLM response".to_string(),
                fixes: Vec::new(),
                unfixable_count: 0,
            }
        });

        // Apply fixes if requested
        if apply && !result.fixes.is_empty() {
            apply_fixes(&config.working_dir, &result.fixes).await?;
        }

        let exit_code = result.exit_code();
        println!("{}", result.render(output_mode));
        Ok(exit_code)
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
async fn apply_fixes(working_dir: &std::path::Path, fixes: &[Fix]) -> Result<()> {
    use tokio::fs;

    for fix in fixes {
        let file_path = if std::path::Path::new(&fix.file).is_absolute() {
            PathBuf::from(&fix.file)
        } else {
            working_dir.join(&fix.file)
        };

        // Read the file
        let content = fs::read_to_string(&file_path)
            .await
            .with_context(|| format!("failed to read file for fix: {}", fix.file))?;

        // Apply the replacement
        if content.contains(&fix.original) {
            let new_content = content.replacen(&fix.original, &fix.replacement, 1);
            fs::write(&file_path, new_content)
                .await
                .with_context(|| format!("failed to write fix to: {}", fix.file))?;
            tracing::info!(file = %fix.file, "applied fix");
        } else {
            tracing::warn!(
                file = %fix.file,
                original = %fix.original,
                "could not find original code to replace"
            );
        }
    }

    Ok(())
}

async fn cmd_commit(
    config: &Config,
    body: bool,
    style: &str,
    agent_mode: bool,
    execute: bool,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    if agent_mode {
        // Agent mode: use full agentic behavior with tools
        let prompt = format!(
            "Generate a {} commit message for the staged changes{}.",
            style,
            if body {
                " with a detailed body explaining the changes"
            } else {
                ""
            }
        );
        let response = run_agent(config, prompts::COMMIT_PROMPT, &prompt).await?;
        let result = AskResult { response, turns: 1 };
        println!("{}", result.render(output_mode));
        Ok(ExitCode::Success)
    } else {
        // Pipeline mode: gather context and make single LLM call
        let context = gather_commit_context(&config.working_dir)
            .await
            .context("failed to gather commit context")?;

        // Check if there are staged changes
        if context
            .git_diff
            .as_ref()
            .is_some_and(|diff| diff.trim().is_empty())
        {
            anyhow::bail!("no staged changes to commit");
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
            execute_git_commit(&config.working_dir, &result.message).await?;
        }

        Ok(ExitCode::Success)
    }
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
            _ => format!("Unknown config key: {k}"),
        };
        let result = ConfigResult {
            entries: vec![ConfigEntry { key: k, value: val }],
        };
        println!("{}", result.render(output_mode));
    } else {
        println!("Usage: usx config <key> or usx config --list");
        println!("To change settings, edit .ursix.toml directly.");
    }
    Ok(ExitCode::Success)
}

async fn cmd_models(config: &Config, output_mode: OutputMode) -> Result<ExitCode> {
    match config.provider {
        Provider::Ollama => {
            let client = OllamaClient::new(&config.ollama_url, &config.model);
            match client.list_models().await {
                Ok(models) => {
                    let result = ModelsResult { models };
                    println!("{}", result.render(output_mode));
                }
                Err(e) => {
                    eprintln!("Failed to list models: {e}");
                }
            }
        }
        Provider::OpenAi => {
            println!(
                "Model listing is not available for OpenAI provider.\n\
                 See https://platform.openai.com/docs/models for available models."
            );
        }
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
}
