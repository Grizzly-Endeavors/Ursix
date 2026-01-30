//! Response parsing utilities for LLM JSON output
//!
//! This module provides functions for parsing structured JSON responses
//! from the LLM into typed result structures. Uses [`json_repair`] to handle
//! common LLM output issues like markdown fences, Python-style booleans, etc.

use anyhow::{Context, Result};

use crate::commands::DeriveType;
use crate::json_repair::{RepairError, repair_json};
use crate::output::{DeriveResult, ReviewIssue, ReviewResult};

/// Intermediate commit message result from parsing
pub(crate) struct ParsedCommit {
    pub message: String,
    pub title: String,
    pub body: Option<String>,
}

/// Intermediate explanation result from parsing
pub(crate) struct ParsedExplanation {
    pub explanation: String,
}

/// Extract and repair JSON from an LLM response
///
/// Handles common LLM issues like code fences, preamble text, Python booleans, etc.
/// Logs a warning if repairs were needed.
fn extract_and_repair_json(response: &str) -> Result<String> {
    match repair_json(response) {
        Ok(result) => {
            if result.was_repaired {
                tracing::warn!(
                    repairs = ?result.repairs_applied,
                    "applied JSON repairs to LLM response"
                );
            }
            Ok(result.json)
        }
        Err(RepairError::NoJsonFound) => Err(anyhow::anyhow!("no JSON object found in response")),
        Err(RepairError::Truncated { context }) => {
            Err(anyhow::anyhow!("JSON appears truncated: {context}"))
        }
        Err(RepairError::Unrecoverable {
            original_error,
            attempts,
        }) => Err(anyhow::anyhow!(
            "failed to repair JSON: {original_error}; tried: {}",
            attempts.join(", ")
        )),
    }
}

/// Parse the LLM JSON response into commit message components
pub(crate) fn parse_commit_response(response: &str) -> Result<ParsedCommit> {
    #[derive(serde::Deserialize)]
    struct CommitJson {
        message: String,
        title: String,
        body: Option<String>,
    }

    let json_str = extract_and_repair_json(response)?;
    let parsed: CommitJson = serde_json::from_str(&json_str)
        .with_context(|| format!("failed to parse commit message JSON: {json_str}"))?;

    Ok(ParsedCommit {
        message: parsed.message,
        title: parsed.title,
        body: parsed.body,
    })
}

/// Parse the LLM JSON response into a [`ReviewResult`]
pub(crate) fn parse_review_response(response: &str) -> Result<ReviewResult> {
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

    let json_str = extract_and_repair_json(response)?;
    let parsed: ReviewJson = serde_json::from_str(&json_str)
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
        chunks_processed: None,
        chunk_failures: vec![],
    })
}

/// Parse the LLM JSON response into explanation components
pub(crate) fn parse_explain_response(response: &str) -> Result<ParsedExplanation> {
    #[derive(serde::Deserialize)]
    struct ExplainJson {
        explanation: String,
    }

    let json_str = extract_and_repair_json(response)?;
    let parsed: ExplainJson = serde_json::from_str(&json_str)
        .with_context(|| format!("failed to parse explain JSON: {json_str}"))?;

    Ok(ParsedExplanation {
        explanation: parsed.explanation,
    })
}

/// Parse the LLM JSON response into a [`DeriveResult`] based on derive type
pub(crate) fn parse_derive_response(
    derive_type: DeriveType,
    response: &str,
) -> Result<DeriveResult> {
    match derive_type {
        DeriveType::CommitMsg => {
            let commit = parse_commit_response(response)?;
            Ok(DeriveResult::CommitMsg {
                message: commit.message,
                title: commit.title,
                body: commit.body,
            })
        }
        DeriveType::Explanation => {
            let explain = parse_explain_response(response)?;
            Ok(DeriveResult::Explanation {
                explanation: explain.explanation,
            })
        }
        DeriveType::Summary => {
            #[derive(serde::Deserialize)]
            struct SummaryJson {
                summary: String,
            }

            let json_str = extract_and_repair_json(response)?;
            let parsed: SummaryJson = serde_json::from_str(&json_str)
                .with_context(|| format!("failed to parse summary JSON: {json_str}"))?;

            Ok(DeriveResult::Summary {
                summary: parsed.summary,
            })
        }
    }
}

/// Parse a summary JSON response (used by chunked processing)
pub(crate) fn parse_summary_response(response: &str) -> Result<String> {
    #[derive(serde::Deserialize)]
    struct SummaryJson {
        summary: String,
    }

    let json_str = extract_and_repair_json(response)?;
    let parsed: SummaryJson = serde_json::from_str(&json_str)
        .with_context(|| format!("failed to parse summary JSON: {json_str}"))?;

    Ok(parsed.summary)
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code uses unwrap for clarity")]
mod tests {
    use super::*;

    // === Valid JSON Tests ===

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

    // === JSON Repair Integration Tests ===

    #[test]
    fn test_parse_commit_with_code_fence() {
        let input =
            "```json\n{\"message\": \"fix: typo\", \"title\": \"fix: typo\", \"body\": null}\n```";
        let result = parse_commit_response(input).unwrap();
        assert_eq!(result.title, "fix: typo");
    }

    #[test]
    fn test_parse_commit_with_preamble() {
        let input = "Sure! Here's the commit message:\n{\"message\": \"fix: typo\", \"title\": \"fix: typo\", \"body\": null}";
        let result = parse_commit_response(input).unwrap();
        assert_eq!(result.title, "fix: typo");
    }

    #[test]
    fn test_parse_review_with_python_booleans() {
        // LLMs sometimes output Python-style booleans
        let input = r#"{"summary": "Code looks good", "issues": []}"#;
        let result = parse_review_response(input).unwrap();
        assert!(result.passed);
    }

    #[test]
    fn test_parse_explain_with_single_quotes() {
        let input = "{'explanation': 'This function does something'}";
        let result = parse_explain_response(input).unwrap();
        assert_eq!(result.explanation, "This function does something");
    }

    #[test]
    fn test_parse_commit_real_world_llm_response() {
        // Simulates a typical messy LLM response
        let input = "Here's your commit message:

```json
{
    'message': 'feat: add new feature',
    'title': 'feat: add new feature',
    'body': None,
}
```

Let me know if you need changes!";
        let result = parse_commit_response(input).unwrap();
        assert_eq!(result.title, "feat: add new feature");
        assert_eq!(result.body, None);
    }
}
