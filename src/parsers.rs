//! Response parsing utilities for LLM JSON output
//!
//! This module provides functions for parsing structured JSON responses
//! from the LLM into typed result structures.

use anyhow::{Context, Result};

use crate::output::{CommitResult, ExplainResult, Fix, FixResult, ReviewIssue, ReviewResult};

/// Extract JSON from a response that may contain additional text
pub fn extract_json(response: &str) -> &str {
    if let Some(start) = response.find('{')
        && let Some(end) = response.rfind('}')
        && end > start
    {
        return &response[start..=end];
    }
    response
}

/// Parse the LLM JSON response into a [`CommitResult`]
pub fn parse_commit_response(response: &str) -> Result<CommitResult> {
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
pub fn parse_review_response(response: &str) -> Result<ReviewResult> {
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
pub fn parse_explain_response(response: &str) -> Result<ExplainResult> {
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
pub fn parse_fix_response(response: &str) -> Result<FixResult> {
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
