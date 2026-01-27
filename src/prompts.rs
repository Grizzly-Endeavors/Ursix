//! Command-specific system prompts for Ursix
//!
//! Each command gets an optimized system prompt tailored to its specific task.
//! Pipeline prompts are for single LLM calls with JSON output.
//!
//! Custom prompts can be configured in `.ursix.toml` under the `[prompts]` section.
//! If a custom prompt is set, it overrides the default for that command.

use crate::config::PromptsConfig;

// =============================================================================
// Custom Prompt Resolution
// =============================================================================

/// Get the effective prompt for a command, checking config overrides first.
///
/// If a custom prompt is configured, returns a reference to it.
/// Otherwise, returns the default prompt for that command.
#[must_use]
pub fn get_prompt<'a>(command: &str, config: &'a PromptsConfig) -> &'a str {
    match command {
        "review" => config.review.as_deref().unwrap_or(REVIEW_PIPELINE_PROMPT),
        "commit-msg" | "commit" => config
            .commit_msg
            .as_deref()
            .unwrap_or(COMMIT_PIPELINE_PROMPT),
        "explanation" | "explain" => config
            .explanation
            .as_deref()
            .unwrap_or(EXPLAIN_PIPELINE_PROMPT),
        "summary" => config.summary.as_deref().unwrap_or(DERIVE_SUMMARY_PROMPT),
        "fix" => config.fix.as_deref().unwrap_or(FIX_PIPELINE_PROMPT),
        _ => ASK_PIPELINE_PROMPT,
    }
}

/// Get the effective review prompt, with optional custom base prompt.
///
/// If `custom_base` is provided, uses it instead of the default review prompt.
/// Rules are still injected into the prompt regardless of which base is used.
#[must_use]
pub fn get_review_prompt(config: &PromptsConfig, rules_section: &str) -> String {
    if let Some(ref custom) = config.review {
        // Custom prompt replaces base, but we still inject rules
        if rules_section.is_empty() {
            custom.clone()
        } else {
            format!("{custom}\n\n{rules_section}")
        }
    } else {
        build_review_prompt(rules_section)
    }
}

// =============================================================================
// Pipeline Mode Prompts
// =============================================================================
// These prompts are for single LLM calls without tool access.
// The LLM must return structured JSON that can be parsed into result types.

/// Pipeline prompt for the review command (no tools, JSON output)
pub const REVIEW_PIPELINE_PROMPT: &str = r#"You are a thorough code reviewer. Your task is to review the provided code changes and provide constructive feedback.

When reviewing code, analyze:
- Correctness and potential bugs
- Security vulnerabilities
- Performance concerns
- Code style and best practices
- Test coverage gaps

Be constructive and explain why something is an issue, not just that it is.
Categorize issues by severity: "error" for critical bugs, "warning" for potential problems, "info" for suggestions.

Return your review as JSON in this exact format:
{
  "summary": "Brief summary of the review findings",
  "issues": [
    {
      "severity": "error|warning|info",
      "file": "path/to/file.rs",
      "line": 42,
      "message": "Description of the issue"
    }
  ]
}

Notes:
- The "issues" array can be empty if no issues were found
- The "file" and "line" fields are optional if the issue is general
- Use "error" sparingly, only for critical bugs or security issues

Output ONLY the JSON, no other text."#;

/// Pipeline prompt for the commit command (no tools, JSON output)
pub const COMMIT_PIPELINE_PROMPT: &str = r#"You are a commit message generator. Your task is to analyze the provided code changes and generate an appropriate commit message.

Generate a commit message following conventional commits format:
- Type: feat, fix, docs, style, refactor, test, chore
- Scope: optional, in parentheses
- Subject: imperative mood, lowercase, no period, under 50 chars
- Body: optional, explain why not what

Return your commit message as JSON in this exact format:
{
  "message": "The full commit message including title and body",
  "title": "feat(scope): subject line under 50 chars",
  "body": "Optional body explaining why the change was made"
}

Notes:
- The "body" field is optional and can be null if not needed
- The "message" field should contain the complete commit message (title + body with blank line separator)

Output ONLY the JSON, no other text."#;

/// Pipeline prompt for the explain command (no tools, JSON output)
pub const EXPLAIN_PIPELINE_PROMPT: &str = r#"You are a code explanation expert. Your task is to explain the provided code clearly and concisely.

When explaining code, cover:
1. A high-level overview of what the code does
2. Key functions, types, and patterns
3. Important relationships and dependencies
4. Any notable design decisions or potential issues

Return your explanation as JSON in this exact format:
{
  "explanation": "Your detailed explanation in markdown format"
}

Notes:
- Use markdown formatting for code snippets and structure
- Be thorough but concise

Output ONLY the JSON, no other text."#;

/// Pipeline prompt for the fix command (no tools, raw code output)
///
/// The fix command uses a constrained transformation model:
/// - LLM receives the issue description and code snippet
/// - LLM outputs ONLY the replacement code (no JSON, no metadata)
/// - Diff generation and validation are done programmatically
pub const FIX_PIPELINE_PROMPT: &str = r"You are a code transformation specialist. Your task is to fix the described issue in the provided code snippet.

IMPORTANT:
- Output ONLY the fixed code
- Do NOT include any explanation, comments, or metadata
- Do NOT wrap the code in markdown code fences
- Preserve the exact indentation and formatting style of the original
- Make the minimal change necessary to fix the issue
- If the issue cannot be fixed, output the original code unchanged

Your output will be used directly as a replacement for the original snippet, so it must be valid, complete code that can replace the original exactly.";

/// Pipeline prompt for the ask command (no tools, simple response)
///
/// The ask command in pipeline mode returns a simple text response,
/// not structured JSON, since it handles general queries.
pub const ASK_PIPELINE_PROMPT: &str = r"You are a helpful coding assistant.

Answer the user's question directly and concisely. Focus on providing accurate, actionable information.

Since you cannot access the filesystem or run commands in this mode, base your response only on the information provided in the query and your training knowledge.

If the question requires file access or command execution that you cannot perform, explain what information would be needed and suggest how the user could gather it.";

/// Get the appropriate pipeline prompt for a command (single LLM call, no tools)
///
/// Pipeline prompts instruct the LLM to return structured JSON output
/// that can be parsed into the corresponding result types.
pub fn pipeline_prompt_for_command(command: &str) -> &'static str {
    match command {
        "explain" => EXPLAIN_PIPELINE_PROMPT,
        "review" => REVIEW_PIPELINE_PROMPT,
        "fix" => FIX_PIPELINE_PROMPT,
        "commit" => COMMIT_PIPELINE_PROMPT,
        // "ask" and all other commands use the simple pipeline prompt
        _ => ASK_PIPELINE_PROMPT,
    }
}

/// Build the review pipeline prompt with dynamic rules section.
///
/// This injects the rules section between the analysis instructions and
/// the output format instructions.
#[must_use]
pub fn build_review_prompt(rules_section: &str) -> String {
    let base = r"You are a thorough code reviewer. Your task is to review the provided code changes and provide constructive feedback.

When reviewing code, analyze:
- Correctness and potential bugs
- Security vulnerabilities
- Performance concerns
- Code style and best practices
- Test coverage gaps
";

    let rules_part = if rules_section.is_empty() {
        String::new()
    } else {
        format!("{rules_section}\n")
    };

    let suffix = r#"Be constructive and explain why something is an issue, not just that it is.
Categorize issues by severity: "error" for critical bugs, "warning" for potential problems, "info" for suggestions.

Return your review as JSON in this exact format:
{
  "summary": "Brief summary of the review findings",
  "issues": [
    {
      "severity": "error|warning|info",
      "file": "path/to/file.rs",
      "line": 42,
      "message": "Description of the issue",
      "rule": "rule-name"
    }
  ]
}

Notes:
- The "issues" array can be empty if no issues were found
- The "file" and "line" fields are optional if the issue is general
- The "rule" field is optional - include it when the issue relates to a specific rule from above
- Use "error" sparingly, only for critical bugs or security issues

Output ONLY the JSON, no other text."#;

    format!("{base}{rules_part}{suffix}")
}

/// Build a focused review prompt for a single category.
///
/// Unlike `build_review_prompt`, this creates a prompt focused on reviewing
/// only one category of rules, enabling per-category LLM calls.
#[must_use]
pub fn build_category_review_prompt(category: &str, rules_section: &str) -> String {
    let base = format!(
        r"You are a code reviewer focused on {category} issues. Your task is to review the provided code for {category} concerns only.

Focus exclusively on {category} issues. Do not report issues outside this category.
"
    );

    let rules_part = if rules_section.is_empty() {
        String::new()
    } else {
        format!("{rules_section}\n")
    };

    let suffix = format!(
        r#"Be constructive and explain why something is an issue, not just that it is.
Categorize issues by severity: "error" for critical bugs, "warning" for potential problems, "info" for suggestions.

Return your review as JSON in this exact format:
{{
  "summary": "Brief summary of {category} findings",
  "issues": [
    {{
      "severity": "error|warning|info",
      "file": "path/to/file.rs",
      "line": 42,
      "message": "Description of the {category} issue",
      "rule": "rule-name"
    }}
  ]
}}

Notes:
- The "issues" array can be empty if no {category} issues were found
- The "file" and "line" fields are optional if the issue is general
- The "rule" field is optional - include it when the issue relates to a specific rule from above
- Use "error" sparingly, only for critical problems
- ONLY report {category} issues - ignore issues that belong to other categories

Output ONLY the JSON, no other text."#
    );

    format!("{base}{rules_part}{suffix}")
}

/// Capitalize the first letter of a string.
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().chain(chars).collect(),
    }
}

// =============================================================================
// Derive Command Prompts
// =============================================================================
// These prompts support the unified derive command which generates content
// based on the derive type (commit-msg, explanation, summary).

use crate::commands::DeriveType;

/// Get the appropriate prompt for a derive type
pub fn derive_prompt_for_type(derive_type: DeriveType) -> &'static str {
    match derive_type {
        DeriveType::CommitMsg => COMMIT_PIPELINE_PROMPT,
        DeriveType::Explanation => EXPLAIN_PIPELINE_PROMPT,
        DeriveType::Summary => DERIVE_SUMMARY_PROMPT,
    }
}

/// Pipeline prompt for the summary derive type
pub const DERIVE_SUMMARY_PROMPT: &str = r#"You are a content summarization expert. Your task is to provide a clear, concise summary of the provided content.

When summarizing:
1. Identify the main purpose and key points
2. Highlight important details without unnecessary elaboration
3. Maintain the essential meaning while being concise
4. Use clear, straightforward language

Return your summary as JSON in this exact format:
{
  "summary": "Your concise summary of the content"
}

Notes:
- Focus on the most important information
- Be thorough but concise

Output ONLY the JSON, no other text."#;

// =============================================================================
// Chunk Processing Prompts
// =============================================================================
// These prompts are used when processing large inputs in chunks.

/// Chunk processing prompt for commit-msg derive type
pub const DERIVE_COMMIT_CHUNK_PROMPT: &str = r#"You are analyzing a portion of a code diff. Your task is to summarize what changes were made in this section.

Focus on:
- What files were modified
- What functionality was added, changed, or removed
- Key implementation details relevant to understanding the change

Return your summary as JSON in this exact format:
{
  "summary": "Brief summary of changes in this diff section"
}

Output ONLY the JSON, no other text."#;

/// Chunk processing prompt for explanation derive type
pub const DERIVE_EXPLANATION_CHUNK_PROMPT: &str = r#"You are explaining a section of code. Your task is to explain what this section does.

Focus on:
- The purpose of the code in this section
- Key functions, types, and patterns
- How this section relates to the overall codebase (if apparent)

Return your explanation as JSON in this exact format:
{
  "explanation": "Explanation of this code section"
}

Output ONLY the JSON, no other text."#;

/// Chunk processing prompt for summary derive type
pub const DERIVE_SUMMARY_CHUNK_PROMPT: &str = r#"You are summarizing a section of content. Your task is to capture the key points from this section.

Focus on:
- Main ideas and important details
- Key facts or findings
- Relevant context

Return your summary as JSON in this exact format:
{
  "summary": "Summary of this section"
}

Output ONLY the JSON, no other text."#;

// =============================================================================
// Synthesis Prompts
// =============================================================================
// These prompts combine chunk results into a final unified output.

/// Synthesis prompt for commit-msg derive type
pub const DERIVE_COMMIT_SYNTHESIS_PROMPT: &str = r#"You are combining summaries of different parts of a code diff into a single commit message.

The input contains summaries of different sections of the diff. Combine them into a cohesive commit message following conventional commits format:
- Type: feat, fix, docs, style, refactor, test, chore
- Scope: optional, in parentheses
- Subject: imperative mood, lowercase, no period, under 50 chars
- Body: optional, explain why not what

Return your commit message as JSON in this exact format:
{
  "message": "The full commit message including title and body",
  "title": "feat(scope): subject line under 50 chars",
  "body": "Optional body explaining why the change was made"
}

Notes:
- The "body" field is optional and can be null if not needed
- The "message" field should contain the complete commit message (title + body with blank line separator)
- Synthesize the partial summaries into a unified message that captures all the changes

Output ONLY the JSON, no other text."#;

/// Synthesis prompt for explanation derive type
pub const DERIVE_EXPLANATION_SYNTHESIS_PROMPT: &str = r#"You are combining explanations of different code sections into a cohesive overall explanation.

The input contains explanations of different parts of the code. Combine them into a unified explanation that:
1. Provides a high-level overview of what the code does
2. Explains how the different sections work together
3. Highlights key functions, types, and patterns
4. Notes any important relationships and dependencies

Return your explanation as JSON in this exact format:
{
  "explanation": "Your detailed explanation in markdown format"
}

Notes:
- Use markdown formatting for code snippets and structure
- Be thorough but concise
- Ensure the explanation flows logically

Output ONLY the JSON, no other text."#;

/// Synthesis prompt for summary derive type
pub const DERIVE_SUMMARY_SYNTHESIS_PROMPT: &str = r#"You are combining partial summaries into a comprehensive overall summary.

The input contains summaries of different sections of the content. Combine them into a cohesive summary that:
1. Captures the main purpose and key points from all sections
2. Maintains logical flow and organization
3. Eliminates redundancy while preserving important details

Return your summary as JSON in this exact format:
{
  "summary": "Your comprehensive summary of the content"
}

Notes:
- Focus on the most important information across all sections
- Be thorough but concise

Output ONLY the JSON, no other text."#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_prompt_for_command() {
        // Specific commands get their own pipeline prompts
        assert_eq!(
            pipeline_prompt_for_command("explain"),
            EXPLAIN_PIPELINE_PROMPT
        );
        assert_eq!(
            pipeline_prompt_for_command("review"),
            REVIEW_PIPELINE_PROMPT
        );
        assert_eq!(pipeline_prompt_for_command("fix"), FIX_PIPELINE_PROMPT);
        assert_eq!(
            pipeline_prompt_for_command("commit"),
            COMMIT_PIPELINE_PROMPT
        );
        // "ask" and unknown commands default to ASK_PIPELINE_PROMPT
        assert_eq!(pipeline_prompt_for_command("ask"), ASK_PIPELINE_PROMPT);
        assert_eq!(pipeline_prompt_for_command("unknown"), ASK_PIPELINE_PROMPT);
    }

    #[test]
    fn test_pipeline_prompts_are_not_empty() {
        assert!(!ASK_PIPELINE_PROMPT.is_empty());
        assert!(!EXPLAIN_PIPELINE_PROMPT.is_empty());
        assert!(!REVIEW_PIPELINE_PROMPT.is_empty());
        assert!(!FIX_PIPELINE_PROMPT.is_empty());
        assert!(!COMMIT_PIPELINE_PROMPT.is_empty());
    }

    #[test]
    fn test_pipeline_prompts_contain_json_instructions() {
        // Review, commit, explain prompts use JSON output
        assert!(REVIEW_PIPELINE_PROMPT.contains("JSON"));
        assert!(COMMIT_PIPELINE_PROMPT.contains("JSON"));
        assert!(EXPLAIN_PIPELINE_PROMPT.contains("JSON"));
        // Fix prompt outputs raw code, not JSON
        assert!(!FIX_PIPELINE_PROMPT.contains("JSON"));
        assert!(FIX_PIPELINE_PROMPT.contains("Output ONLY the fixed code"));
    }

    #[test]
    fn test_pipeline_prompts_do_not_mention_tools() {
        // Pipeline prompts should not reference tools
        assert!(!REVIEW_PIPELINE_PROMPT.contains("Available tools"));
        assert!(!COMMIT_PIPELINE_PROMPT.contains("Available tools"));
        assert!(!EXPLAIN_PIPELINE_PROMPT.contains("Available tools"));
        assert!(!FIX_PIPELINE_PROMPT.contains("Available tools"));
        assert!(!ASK_PIPELINE_PROMPT.contains("Available tools"));
    }

    #[test]
    fn test_build_review_prompt_empty_rules() {
        let prompt = build_review_prompt("");
        assert!(prompt.contains("code reviewer"));
        assert!(prompt.contains("JSON"));
        assert!(!prompt.contains("## Review Rules"));
    }

    #[test]
    fn test_build_review_prompt_with_rules() {
        let rules = "## Review Rules\n\n- **no-unwrap** [error]: Avoid unwrap";
        let prompt = build_review_prompt(rules);
        assert!(prompt.contains("code reviewer"));
        assert!(prompt.contains("## Review Rules"));
        assert!(prompt.contains("no-unwrap"));
        assert!(prompt.contains("JSON"));
    }

    #[test]
    fn test_build_review_prompt_includes_rule_field() {
        let prompt = build_review_prompt("");
        assert!(prompt.contains("\"rule\":"));
    }

    #[test]
    fn test_build_category_review_prompt_empty_rules() {
        let prompt = build_category_review_prompt("security", "");
        assert!(prompt.contains("focused on security"));
        assert!(prompt.contains("security concerns only"));
        assert!(prompt.contains("JSON"));
        assert!(prompt.contains("ONLY report security issues"));
    }

    #[test]
    fn test_build_category_review_prompt_with_rules() {
        let rules = "## Review Rules - Security\n\n- **no-unwrap** [error]: Avoid unwrap";
        let prompt = build_category_review_prompt("security", rules);
        assert!(prompt.contains("focused on security"));
        assert!(prompt.contains("## Review Rules - Security"));
        assert!(prompt.contains("no-unwrap"));
    }

    #[test]
    fn test_build_category_review_prompt_different_categories() {
        let security_prompt = build_category_review_prompt("security", "");
        let style_prompt = build_category_review_prompt("style", "");

        assert!(security_prompt.contains("security issues"));
        assert!(!security_prompt.contains("style issues"));

        assert!(style_prompt.contains("style issues"));
        assert!(!style_prompt.contains("security issues"));
    }

    #[test]
    fn test_capitalize_first() {
        assert_eq!(capitalize_first("hello"), "Hello");
        assert_eq!(capitalize_first("HELLO"), "HELLO");
        assert_eq!(capitalize_first(""), "");
        assert_eq!(capitalize_first("a"), "A");
        assert_eq!(capitalize_first("security"), "Security");
    }

    #[test]
    fn test_get_prompt_defaults() {
        let config = PromptsConfig::default();
        assert_eq!(get_prompt("review", &config), REVIEW_PIPELINE_PROMPT);
        assert_eq!(get_prompt("commit-msg", &config), COMMIT_PIPELINE_PROMPT);
        assert_eq!(get_prompt("commit", &config), COMMIT_PIPELINE_PROMPT);
        assert_eq!(get_prompt("explanation", &config), EXPLAIN_PIPELINE_PROMPT);
        assert_eq!(get_prompt("explain", &config), EXPLAIN_PIPELINE_PROMPT);
        assert_eq!(get_prompt("summary", &config), DERIVE_SUMMARY_PROMPT);
        assert_eq!(get_prompt("fix", &config), FIX_PIPELINE_PROMPT);
        assert_eq!(get_prompt("unknown", &config), ASK_PIPELINE_PROMPT);
    }

    #[test]
    fn test_get_prompt_custom() {
        let config = PromptsConfig {
            review: Some("Custom review prompt".to_string()),
            commit_msg: Some("Custom commit prompt".to_string()),
            explanation: Some("Custom explain prompt".to_string()),
            summary: Some("Custom summary prompt".to_string()),
            fix: Some("Custom fix prompt".to_string()),
        };
        assert_eq!(get_prompt("review", &config), "Custom review prompt");
        assert_eq!(get_prompt("commit-msg", &config), "Custom commit prompt");
        assert_eq!(get_prompt("explanation", &config), "Custom explain prompt");
        assert_eq!(get_prompt("summary", &config), "Custom summary prompt");
        assert_eq!(get_prompt("fix", &config), "Custom fix prompt");
    }

    #[test]
    fn test_get_review_prompt_default() {
        let config = PromptsConfig::default();
        let prompt = get_review_prompt(&config, "");
        assert!(prompt.contains("code reviewer"));
    }

    #[test]
    fn test_get_review_prompt_custom_with_rules() {
        let config = PromptsConfig {
            review: Some("Custom reviewer".to_string()),
            ..Default::default()
        };
        let prompt = get_review_prompt(&config, "## Rules\n- test rule");
        assert!(prompt.contains("Custom reviewer"));
        assert!(prompt.contains("## Rules"));
    }

    #[test]
    fn test_get_review_prompt_custom_no_rules() {
        let config = PromptsConfig {
            review: Some("Custom reviewer".to_string()),
            ..Default::default()
        };
        let prompt = get_review_prompt(&config, "");
        assert_eq!(prompt, "Custom reviewer");
    }
}
