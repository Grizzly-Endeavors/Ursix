//! JSON repair module for handling common LLM output issues
//!
//! LLMs often produce malformed JSON due to various issues like markdown fences,
//! Python-style booleans, single quotes, or truncation. This module attempts
//! recovery before returning errors while maintaining strict validation.

use std::fmt::{self, Write};

/// Errors that can occur during JSON repair
#[derive(Debug, Clone)]
pub enum RepairError {
    /// No JSON object found in the response
    NoJsonFound,
    /// JSON was truncated mid-value and cannot be recovered
    Truncated { context: String },
    /// All repair attempts failed
    Unrecoverable {
        original_error: String,
        attempts: Vec<String>,
    },
}

impl fmt::Display for RepairError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoJsonFound => write!(f, "no JSON object found in response"),
            Self::Truncated { context } => {
                write!(f, "JSON appears truncated: {context}")
            }
            Self::Unrecoverable {
                original_error,
                attempts,
            } => {
                write!(
                    f,
                    "failed to repair JSON: {original_error}; tried: {}",
                    attempts.join(", ")
                )
            }
        }
    }
}

impl std::error::Error for RepairError {}

/// The result of a successful JSON repair operation
#[derive(Debug, Clone)]
pub struct RepairResult {
    /// The repaired JSON string
    pub json: String,
    /// List of repairs that were applied
    pub repairs_applied: Vec<RepairKind>,
    /// Whether any repairs were needed
    pub was_repaired: bool,
}

/// Types of repairs that can be applied to JSON
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairKind {
    /// Removed markdown code fence
    RemovedCodeFence,
    /// Removed preamble text before JSON
    RemovedPreamble,
    /// Removed postamble text after JSON
    RemovedPostamble,
    /// Replaced single quotes with double quotes
    ReplacedSingleQuotes,
    /// Added quotes around unquoted keys
    QuotedKeys,
    /// Replaced Python True/False with true/false
    ReplacedPythonBooleans,
    /// Replaced Python None with null
    ReplacedPythonNone,
    /// Removed trailing commas
    RemovedTrailingCommas,
    /// Added missing commas between elements
    AddedMissingCommas,
    /// Escaped unescaped control characters
    EscapedControlChars,
    /// Added missing closing braces/brackets
    AddedMissingClosers,
}

impl fmt::Display for RepairKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RemovedCodeFence => write!(f, "removed code fence"),
            Self::RemovedPreamble => write!(f, "removed preamble"),
            Self::RemovedPostamble => write!(f, "removed postamble"),
            Self::ReplacedSingleQuotes => write!(f, "replaced single quotes"),
            Self::QuotedKeys => write!(f, "quoted unquoted keys"),
            Self::ReplacedPythonBooleans => write!(f, "replaced Python booleans"),
            Self::ReplacedPythonNone => write!(f, "replaced Python None"),
            Self::RemovedTrailingCommas => write!(f, "removed trailing commas"),
            Self::AddedMissingCommas => write!(f, "added missing commas"),
            Self::EscapedControlChars => write!(f, "escaped control chars"),
            Self::AddedMissingClosers => write!(f, "added missing closers"),
        }
    }
}

/// Attempt to repair malformed JSON from LLM responses
///
/// Applies repairs in order of safety, validating after each phase.
/// Returns early once valid JSON is achieved.
pub fn repair_json(response: &str) -> Result<RepairResult, RepairError> {
    let trimmed = response.trim();
    if trimmed.is_empty() {
        return Err(RepairError::NoJsonFound);
    }

    let mut repairs = Vec::new();
    let mut attempts = Vec::new();

    // Phase 1: Try parsing as-is
    if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
        return Ok(RepairResult {
            json: trimmed.to_string(),
            repairs_applied: repairs,
            was_repaired: false,
        });
    }

    // Phase 2: Extract JSON block (remove code fences, preamble, postamble)
    let (extracted, extraction_repairs) = extract_json_block(trimmed);
    repairs.extend(extraction_repairs);

    if extracted.is_empty() || !extracted.contains('{') {
        return Err(RepairError::NoJsonFound);
    }

    if serde_json::from_str::<serde_json::Value>(&extracted).is_ok() {
        let was_repaired = !repairs.is_empty();
        return Ok(RepairResult {
            json: extracted,
            repairs_applied: repairs,
            was_repaired,
        });
    }
    attempts.push("extraction".to_string());

    // Phase 3: Value normalization (Python booleans, None)
    let (normalized, norm_repairs) = normalize_values(&extracted);
    repairs.extend(norm_repairs.clone());

    if serde_json::from_str::<serde_json::Value>(&normalized).is_ok() {
        return Ok(RepairResult {
            json: normalized,
            repairs_applied: repairs,
            was_repaired: true,
        });
    }
    if !norm_repairs.is_empty() {
        attempts.push("value normalization".to_string());
    }

    // Phase 4: Quote fixes (single -> double, unquoted keys)
    let (quoted, quote_repairs) = fix_quotes(&normalized);
    repairs.extend(quote_repairs.clone());

    if serde_json::from_str::<serde_json::Value>(&quoted).is_ok() {
        return Ok(RepairResult {
            json: quoted,
            repairs_applied: repairs,
            was_repaired: true,
        });
    }
    if !quote_repairs.is_empty() {
        attempts.push("quote fixes".to_string());
    }

    // Phase 5: Structure fixes (trailing commas, escape control chars)
    let (structured, struct_repairs) = fix_structure(&quoted);
    repairs.extend(struct_repairs.clone());

    if serde_json::from_str::<serde_json::Value>(&structured).is_ok() {
        return Ok(RepairResult {
            json: structured,
            repairs_applied: repairs,
            was_repaired: true,
        });
    }
    if !struct_repairs.is_empty() {
        attempts.push("structure fixes".to_string());
    }

    // Phase 6: Try adding missing closers (last resort for truncated JSON)
    let (completed, closer_repairs) = add_missing_closers(&structured);
    repairs.extend(closer_repairs.clone());

    if serde_json::from_str::<serde_json::Value>(&completed).is_ok() {
        return Ok(RepairResult {
            json: completed,
            repairs_applied: repairs,
            was_repaired: true,
        });
    }
    if !closer_repairs.is_empty() {
        attempts.push("adding closers".to_string());
    }

    // Check if it looks truncated
    if looks_truncated(&structured) {
        return Err(RepairError::Truncated {
            context: truncation_context(&structured),
        });
    }

    // Get the actual parse error for the final message
    let original_error = match serde_json::from_str::<serde_json::Value>(&completed) {
        Ok(_) => "unknown error".to_string(),
        Err(e) => e.to_string(),
    };

    Err(RepairError::Unrecoverable {
        original_error,
        attempts,
    })
}

/// Extract JSON block from response, handling code fences and surrounding text
fn extract_json_block(input: &str) -> (String, Vec<RepairKind>) {
    let mut repairs = Vec::new();
    let mut text = input.to_string();

    // Remove markdown code fences
    if let Some(fence_result) = remove_code_fences(&text) {
        text = fence_result.content;
        repairs.push(RepairKind::RemovedCodeFence);
        // Track preamble/postamble that was removed with the fence
        if fence_result.had_preamble {
            repairs.push(RepairKind::RemovedPreamble);
        }
        if fence_result.had_postamble {
            repairs.push(RepairKind::RemovedPostamble);
        }
    }

    // Find JSON boundaries
    let Some(start) = text.find('{') else {
        return (String::new(), repairs);
    };

    // Only report preamble removal if not already reported from fence removal
    if start > 0 && !repairs.contains(&RepairKind::RemovedPreamble) {
        repairs.push(RepairKind::RemovedPreamble);
    }

    // Find matching closing brace, accounting for nested structures
    let end = find_matching_brace(&text, start);

    // Only report postamble removal if not already reported from fence removal
    if end < text.len() - 1 && !repairs.contains(&RepairKind::RemovedPostamble) {
        repairs.push(RepairKind::RemovedPostamble);
    }

    (text[start..=end].to_string(), repairs)
}

/// Result of code fence removal
struct FenceRemovalResult {
    content: String,
    had_preamble: bool,
    had_postamble: bool,
}

/// Remove markdown code fences from text
///
/// Handles fences at start/end of text as well as fences embedded in surrounding text.
/// Returns the extracted content along with flags indicating if preamble/postamble was removed.
fn remove_code_fences(input: &str) -> Option<FenceRemovalResult> {
    // Try to match fences at start/end first
    let start_fence = regex::Regex::new(r"^```(?:json)?\s*\n?").ok()?;
    let end_fence = regex::Regex::new(r"\n?```\s*$").ok()?;

    let has_start_fence = start_fence.is_match(input);
    let has_end_fence = end_fence.is_match(input);

    if has_start_fence || has_end_fence {
        let result = start_fence.replace(input, "");
        let result = end_fence.replace(&result, "");
        return Some(FenceRemovalResult {
            content: result.to_string(),
            had_preamble: false,
            had_postamble: false,
        });
    }

    // Also try to extract content from fences embedded in text (preamble...```json...```...postamble)
    let embedded_fence = regex::Regex::new(r"```(?:json)?\s*\n?([\s\S]*?)\n?```").ok()?;
    if let Some(captures) = embedded_fence.captures(input) {
        let full_match = captures.get(0)?;
        let content = captures.get(1)?;

        // Check if there was text before or after the fence
        let had_preamble = full_match.start() > 0;
        let had_postamble = full_match.end() < input.len();

        return Some(FenceRemovalResult {
            content: content.as_str().to_string(),
            had_preamble,
            had_postamble,
        });
    }

    None
}

/// Find the index of the closing brace that matches the opening brace at `start`
///
/// Returns the index of the matching `}`, or the last index in the string if
/// no match is found (for truncated JSON that needs repair).
fn find_matching_brace(text: &str, start: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut depth = 0;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, &ch) in chars.iter().enumerate().skip(start) {
        if escape_next {
            escape_next = false;
            continue;
        }

        if ch == '\\' && in_string {
            escape_next = true;
            continue;
        }

        if ch == '"' {
            in_string = !in_string;
            continue;
        }

        if in_string {
            continue;
        }

        match ch {
            '{' | '[' => depth += 1,
            '}' | ']' => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
    }

    // No match found (truncated JSON) - return last position to include all content
    chars.len().saturating_sub(1)
}

/// Normalize Python-style values to JSON
fn normalize_values(input: &str) -> (String, Vec<RepairKind>) {
    let mut repairs = Vec::new();
    let mut result = input.to_string();

    // Replace Python booleans (must be careful not to match inside strings)
    let bool_replaced = replace_outside_strings(&result, &[("True", "true"), ("False", "false")]);
    if bool_replaced != result {
        repairs.push(RepairKind::ReplacedPythonBooleans);
        result = bool_replaced;
    }

    // Replace Python None
    let none_replaced = replace_outside_strings(&result, &[("None", "null")]);
    if none_replaced != result {
        repairs.push(RepairKind::ReplacedPythonNone);
        result = none_replaced;
    }

    (result, repairs)
}

/// Replace patterns only when they appear outside of quoted strings
fn replace_outside_strings(input: &str, replacements: &[(&str, &str)]) -> String {
    let mut result = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let mut in_string = false;
    let mut escape_next = false;

    while i < chars.len() {
        if escape_next {
            escape_next = false;
            result.push(chars[i]);
            i += 1;
            continue;
        }

        if chars[i] == '\\' && in_string {
            escape_next = true;
            result.push(chars[i]);
            i += 1;
            continue;
        }

        if chars[i] == '"' {
            in_string = !in_string;
            result.push(chars[i]);
            i += 1;
            continue;
        }

        if in_string {
            result.push(chars[i]);
            i += 1;
            continue;
        }

        // Try to match replacements
        let remaining: String = chars[i..].iter().collect();
        let mut replaced = false;

        for (from, to) in replacements {
            if remaining.starts_with(from) {
                // Ensure it's a complete token (not part of a longer identifier)
                let after_idx = i + from.len();
                let is_complete_token = after_idx >= chars.len()
                    || !chars[after_idx].is_alphanumeric() && chars[after_idx] != '_';

                let before_ok = i == 0
                    || !chars[i - 1].is_alphanumeric()
                        && chars[i - 1] != '_'
                        && chars[i - 1] != '"';

                if is_complete_token && before_ok {
                    result.push_str(to);
                    i += from.len();
                    replaced = true;
                    break;
                }
            }
        }

        if !replaced {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

/// Fix quote issues: single quotes to double, unquoted keys
fn fix_quotes(input: &str) -> (String, Vec<RepairKind>) {
    let mut repairs = Vec::new();
    let mut result = input.to_string();

    // Replace single quotes with double quotes (carefully)
    let single_fixed = fix_single_quotes(&result);
    if single_fixed != result {
        repairs.push(RepairKind::ReplacedSingleQuotes);
        result = single_fixed;
    }

    // Quote unquoted keys
    let keys_fixed = quote_unquoted_keys(&result);
    if keys_fixed != result {
        repairs.push(RepairKind::QuotedKeys);
        result = keys_fixed;
    }

    (result, repairs)
}

/// Replace single quotes with double quotes for JSON compatibility
fn fix_single_quotes(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let mut in_double_string = false;
    let mut in_single_string = false;
    let mut escape_next = false;

    while i < chars.len() {
        if escape_next {
            escape_next = false;
            // If escaping a single quote in what's becoming a double-quoted string,
            // we don't need the escape
            if chars[i] == '\'' && in_single_string {
                result.push('\'');
            } else {
                result.push(chars[i]);
            }
            i += 1;
            continue;
        }

        if chars[i] == '\\' {
            escape_next = true;
            result.push(chars[i]);
            i += 1;
            continue;
        }

        if chars[i] == '"' && !in_single_string {
            in_double_string = !in_double_string;
            result.push(chars[i]);
            i += 1;
            continue;
        }

        if chars[i] == '\'' && !in_double_string {
            // Convert single quote to double quote
            result.push('"');
            in_single_string = !in_single_string;
            i += 1;
            continue;
        }

        // If we're in a single-quoted string and encounter a double quote, escape it
        if chars[i] == '"' && in_single_string {
            result.push('\\');
            result.push('"');
            i += 1;
            continue;
        }

        result.push(chars[i]);
        i += 1;
    }

    result
}

/// Add quotes around unquoted object keys
fn quote_unquoted_keys(input: &str) -> String {
    // Pattern: key followed by colon, where key is unquoted
    let re = regex::Regex::new(r"([{,]\s*)([a-zA-Z_][a-zA-Z0-9_]*)\s*:");
    match re {
        Ok(regex) => regex.replace_all(input, r#"$1"$2":"#).to_string(),
        Err(_) => input.to_string(),
    }
}

/// Fix structural issues like trailing commas and unescaped control chars
fn fix_structure(input: &str) -> (String, Vec<RepairKind>) {
    let mut repairs = Vec::new();
    let mut result = input.to_string();

    // Escape unescaped control characters in strings
    let escaped = escape_control_chars(&result);
    if escaped != result {
        repairs.push(RepairKind::EscapedControlChars);
        result = escaped;
    }

    // Remove trailing commas before ] or }
    let no_trailing = remove_trailing_commas(&result);
    if no_trailing != result {
        repairs.push(RepairKind::RemovedTrailingCommas);
        result = no_trailing;
    }

    // Try to add missing commas between elements
    let with_commas = add_missing_commas(&result);
    if with_commas != result {
        repairs.push(RepairKind::AddedMissingCommas);
        result = with_commas;
    }

    (result, repairs)
}

/// Escape control characters inside strings
fn escape_control_chars(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut in_string = false;
    let mut escape_next = false;

    for ch in input.chars() {
        if escape_next {
            escape_next = false;
            result.push(ch);
            continue;
        }

        if ch == '\\' && in_string {
            escape_next = true;
            result.push(ch);
            continue;
        }

        if ch == '"' {
            in_string = !in_string;
            result.push(ch);
            continue;
        }

        if in_string {
            // Check for unescaped control characters
            match ch {
                '\n' => result.push_str("\\n"),
                '\r' => result.push_str("\\r"),
                '\t' => result.push_str("\\t"),
                c if c.is_control() => {
                    // Escape other control chars as unicode - write! on String never fails
                    let _ = write!(result, "\\u{:04x}", c as u32);
                }
                _ => result.push(ch),
            }
        } else {
            result.push(ch);
        }
    }

    result
}

/// Remove trailing commas before closing brackets
fn remove_trailing_commas(input: &str) -> String {
    let re = regex::Regex::new(r",(\s*[}\]])");
    match re {
        Ok(regex) => regex.replace_all(input, "$1").to_string(),
        Err(_) => input.to_string(),
    }
}

/// Try to add missing commas between array/object elements
fn add_missing_commas(input: &str) -> String {
    // Pattern: "value" "key" or "value" { or ] { etc. without comma
    let patterns = [
        (r#""\s+""#, "\", \""),                    // "value" "next" -> "value", "next"
        (r#""\s+\{"#, "\", {"),                    // "value" { -> "value", {
        (r#"\}\s+""#, "}, \""),                    // } "key" -> }, "key"
        (r"\}\s+\{", "}, {"),                      // } { -> }, {
        (r"\]\s+\[", "], ["),                      // ] [ -> ], [
        (r#"(true|false|null|\d)\s+""#, "$1, \""), // literal "key" -> literal, "key"
    ];

    let mut result = input.to_string();
    for (pattern, replacement) in patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            result = re.replace_all(&result, replacement).to_string();
        }
    }
    result
}

/// Add missing closing braces/brackets for truncated JSON
fn add_missing_closers(input: &str) -> (String, Vec<RepairKind>) {
    let mut result = input.to_string();
    let mut stack = Vec::new();
    let mut in_string = false;
    let mut escape_next = false;

    for ch in input.chars() {
        if escape_next {
            escape_next = false;
            continue;
        }

        if ch == '\\' && in_string {
            escape_next = true;
            continue;
        }

        if ch == '"' {
            in_string = !in_string;
            continue;
        }

        if in_string {
            continue;
        }

        match ch {
            '{' => stack.push('}'),
            '[' => stack.push(']'),
            '}' | ']' => {
                if let Some(expected) = stack.pop()
                    && expected != ch
                {
                    // Mismatched brackets - push back and try to recover
                    stack.push(expected);
                }
            }
            _ => {}
        }
    }

    if stack.is_empty() {
        return (result, Vec::new());
    }

    // If we're in a string, close it first
    if in_string {
        result.push('"');
    }

    // Add missing closers in reverse order
    while let Some(closer) = stack.pop() {
        result.push(closer);
    }

    (result, vec![RepairKind::AddedMissingClosers])
}

/// Check if JSON appears to be truncated mid-value
fn looks_truncated(input: &str) -> bool {
    let trimmed = input.trim_end();

    // If it ends with an incomplete token, it's probably truncated
    if trimmed.ends_with(':')
        || trimmed.ends_with(',')
        || trimmed.ends_with('[')
        || trimmed.ends_with('{')
    {
        return true;
    }

    // Check for unclosed string at end
    let mut in_string = false;
    let mut escape_next = false;
    for ch in trimmed.chars() {
        if escape_next {
            escape_next = false;
            continue;
        }
        if ch == '\\' {
            escape_next = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
        }
    }

    in_string
}

/// Get context about where truncation occurred
fn truncation_context(input: &str) -> String {
    let trimmed = input.trim();
    let len = trimmed.len();
    let preview_len = 50.min(len);
    let start = len.saturating_sub(preview_len);

    format!("...{}", &trimmed[start..])
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // === Extraction Tests ===

    #[test]
    fn test_valid_json_unchanged() {
        let input = r#"{"key": "value"}"#;
        let result = repair_json(input).unwrap();
        assert!(!result.was_repaired);
        assert_eq!(result.json, input);
        assert!(result.repairs_applied.is_empty());
    }

    #[test]
    fn test_remove_code_fence() {
        let input = "```json\n{\"key\": \"value\"}\n```";
        let result = repair_json(input).unwrap();
        assert!(result.was_repaired);
        assert!(
            result
                .repairs_applied
                .contains(&RepairKind::RemovedCodeFence)
        );
        assert_eq!(result.json, r#"{"key": "value"}"#);
    }

    #[test]
    fn test_remove_code_fence_no_lang() {
        let input = "```\n{\"key\": \"value\"}\n```";
        let result = repair_json(input).unwrap();
        assert!(result.was_repaired);
        assert_eq!(result.json, r#"{"key": "value"}"#);
    }

    #[test]
    fn test_remove_preamble() {
        let input = "Sure, here is the JSON:\n{\"key\": \"value\"}";
        let result = repair_json(input).unwrap();
        assert!(result.was_repaired);
        assert!(
            result
                .repairs_applied
                .contains(&RepairKind::RemovedPreamble)
        );
        assert_eq!(result.json, r#"{"key": "value"}"#);
    }

    #[test]
    fn test_remove_postamble() {
        let input = "{\"key\": \"value\"}\nThat's the result.";
        let result = repair_json(input).unwrap();
        assert!(result.was_repaired);
        assert!(
            result
                .repairs_applied
                .contains(&RepairKind::RemovedPostamble)
        );
        assert_eq!(result.json, r#"{"key": "value"}"#);
    }

    #[test]
    fn test_remove_preamble_and_postamble() {
        let input = "Here's your JSON:\n{\"key\": \"value\"}\nHope this helps!";
        let result = repair_json(input).unwrap();
        assert!(result.was_repaired);
        assert!(
            result
                .repairs_applied
                .contains(&RepairKind::RemovedPreamble)
        );
        assert!(
            result
                .repairs_applied
                .contains(&RepairKind::RemovedPostamble)
        );
    }

    // === Value Normalization Tests ===

    #[test]
    fn test_python_booleans() {
        let input = r#"{"enabled": True, "disabled": False}"#;
        let result = repair_json(input).unwrap();
        assert!(result.was_repaired);
        assert!(
            result
                .repairs_applied
                .contains(&RepairKind::ReplacedPythonBooleans)
        );
        assert_eq!(result.json, r#"{"enabled": true, "disabled": false}"#);
    }

    #[test]
    fn test_python_none() {
        let input = r#"{"value": None}"#;
        let result = repair_json(input).unwrap();
        assert!(result.was_repaired);
        assert!(
            result
                .repairs_applied
                .contains(&RepairKind::ReplacedPythonNone)
        );
        assert_eq!(result.json, r#"{"value": null}"#);
    }

    #[test]
    fn test_python_values_not_in_strings() {
        let input = r#"{"text": "True and False and None"}"#;
        let result = repair_json(input).unwrap();
        // Should NOT be repaired - values are inside a string
        assert!(!result.was_repaired);
        assert_eq!(result.json, input);
    }

    // === Quote Fix Tests ===

    #[test]
    fn test_single_quotes() {
        let input = r"{'key': 'value'}";
        let result = repair_json(input).unwrap();
        assert!(result.was_repaired);
        assert!(
            result
                .repairs_applied
                .contains(&RepairKind::ReplacedSingleQuotes)
        );
        assert_eq!(result.json, r#"{"key": "value"}"#);
    }

    #[test]
    fn test_unquoted_keys() {
        let input = r#"{key: "value", other: 123}"#;
        let result = repair_json(input).unwrap();
        assert!(result.was_repaired);
        assert!(result.repairs_applied.contains(&RepairKind::QuotedKeys));
        assert_eq!(result.json, r#"{"key": "value", "other": 123}"#);
    }

    #[test]
    fn test_single_quotes_with_internal_double() {
        let input = r#"{'message': 'He said "hello"'}"#;
        let result = repair_json(input).unwrap();
        assert!(result.was_repaired);
        // The internal double quote should be escaped
        assert_eq!(result.json, r#"{"message": "He said \"hello\""}"#);
    }

    // === Structure Fix Tests ===

    #[test]
    fn test_trailing_comma_object() {
        let input = r#"{"key": "value",}"#;
        let result = repair_json(input).unwrap();
        assert!(result.was_repaired);
        assert!(
            result
                .repairs_applied
                .contains(&RepairKind::RemovedTrailingCommas)
        );
        assert_eq!(result.json, r#"{"key": "value"}"#);
    }

    #[test]
    fn test_trailing_comma_array() {
        let input = r#"{"items": [1, 2, 3,]}"#;
        let result = repair_json(input).unwrap();
        assert!(result.was_repaired);
        assert_eq!(result.json, r#"{"items": [1, 2, 3]}"#);
    }

    #[test]
    fn test_escape_newline_in_string() {
        let input = "{\"text\": \"line1\nline2\"}";
        let result = repair_json(input).unwrap();
        assert!(result.was_repaired);
        assert!(
            result
                .repairs_applied
                .contains(&RepairKind::EscapedControlChars)
        );
        assert_eq!(result.json, r#"{"text": "line1\nline2"}"#);
    }

    // === Missing Closers Tests ===

    #[test]
    fn test_missing_closing_brace() {
        let input = r#"{"key": "value""#;
        let result = repair_json(input).unwrap();
        assert!(result.was_repaired);
        assert!(
            result
                .repairs_applied
                .contains(&RepairKind::AddedMissingClosers)
        );
        assert_eq!(result.json, r#"{"key": "value"}"#);
    }

    #[test]
    fn test_missing_nested_closers() {
        let input = r#"{"items": [1, 2, 3"#;
        let result = repair_json(input).unwrap();
        assert!(result.was_repaired);
        assert_eq!(result.json, r#"{"items": [1, 2, 3]}"#);
    }

    // === Combined Issues Tests ===

    #[test]
    fn test_code_fence_with_single_quotes_and_python_bool() {
        let input = "```json\n{'ok': True}\n```";
        let result = repair_json(input).unwrap();
        assert!(result.was_repaired);
        assert_eq!(result.json, r#"{"ok": true}"#);
    }

    #[test]
    fn test_preamble_with_trailing_comma() {
        let input = "Here's the JSON:\n{\"a\": 1,}";
        let result = repair_json(input).unwrap();
        assert!(result.was_repaired);
        assert_eq!(result.json, r#"{"a": 1}"#);
    }

    #[test]
    fn test_real_world_llm_response() {
        let input = r"Sure! Here's the commit message:

```json
{
    'message': 'feat: add new feature\n\nThis adds a cool new feature.',
    'title': 'feat: add new feature',
    'body': None,
}
```

Let me know if you need anything else!";
        let result = repair_json(input).unwrap();
        assert!(result.was_repaired);
        // Should have all these repairs
        assert!(
            result
                .repairs_applied
                .contains(&RepairKind::RemovedCodeFence)
        );
        assert!(
            result
                .repairs_applied
                .contains(&RepairKind::RemovedPreamble)
        );
        assert!(
            result
                .repairs_applied
                .contains(&RepairKind::RemovedPostamble)
        );
        assert!(
            result
                .repairs_applied
                .contains(&RepairKind::ReplacedSingleQuotes)
        );
        assert!(
            result
                .repairs_applied
                .contains(&RepairKind::ReplacedPythonNone)
        );

        // Verify we can parse the result
        let parsed: serde_json::Value = serde_json::from_str(&result.json).unwrap();
        assert_eq!(parsed["title"], "feat: add new feature");
    }

    // === Error Cases ===

    #[test]
    fn test_no_json_found() {
        let input = "This is just plain text with no JSON at all.";
        let result = repair_json(input);
        assert!(matches!(result, Err(RepairError::NoJsonFound)));
    }

    #[test]
    fn test_empty_input() {
        let input = "";
        let result = repair_json(input);
        assert!(matches!(result, Err(RepairError::NoJsonFound)));
    }

    #[test]
    fn test_truncated_mid_string() {
        let input = r#"{"message": "This is a very long message that got cut"#;
        // This should either succeed with closers added, or fail with truncation
        let result = repair_json(input);
        // Our implementation should be able to close this
        assert!(result.is_ok());
    }

    #[test]
    fn test_nested_objects() {
        let input = r"{'outer': {'inner': True, 'value': None}}";
        let result = repair_json(input).unwrap();
        assert!(result.was_repaired);
        let parsed: serde_json::Value = serde_json::from_str(&result.json).unwrap();
        assert_eq!(parsed["outer"]["inner"], true);
        assert!(parsed["outer"]["value"].is_null());
    }

    #[test]
    fn test_array_of_objects() {
        let input = r"{'items': [{'a': True}, {'b': False}]}";
        let result = repair_json(input).unwrap();
        assert!(result.was_repaired);
        let parsed: serde_json::Value = serde_json::from_str(&result.json).unwrap();
        assert_eq!(parsed["items"][0]["a"], true);
        assert_eq!(parsed["items"][1]["b"], false);
    }
}
