//! Token counting using `HuggingFace` tokenizers
//!
//! Provides token counting functionality for input validation and chunking decisions.
//! Uses GPT-2 tokenizer as a reasonable cross-provider approximation.

use std::sync::OnceLock;

use anyhow::Result;
use tokenizers::Tokenizer;

use crate::context::GatheredContext;

/// Global tokenizer instance (loaded once, cached)
static TOKENIZER: OnceLock<Tokenizer> = OnceLock::new();

/// Load the tokenizer (GPT-2 as reasonable cross-provider approximation)
///
/// # Errors
/// Returns error if the tokenizer cannot be loaded from `HuggingFace` Hub.
fn get_tokenizer() -> Result<&'static Tokenizer> {
    // Check if already initialized
    if let Some(tokenizer) = TOKENIZER.get() {
        return Ok(tokenizer);
    }

    // Try to initialize
    let tokenizer = Tokenizer::from_pretrained("gpt2", None)
        .map_err(|e| anyhow::anyhow!("failed to load tokenizer: {e}"))?;

    // Store it (ignore if another thread beat us)
    let _ = TOKENIZER.set(tokenizer);

    // Return the stored reference
    TOKENIZER
        .get()
        .ok_or_else(|| anyhow::anyhow!("tokenizer initialization failed unexpectedly"))
}

/// Token count result
#[derive(Debug, Clone, Copy)]
pub struct TokenCount {
    /// Number of tokens
    pub count: usize,
}

impl TokenCount {
    /// Create a new token count
    #[must_use]
    pub const fn new(count: usize) -> Self {
        Self { count }
    }
}

/// Token limit thresholds for warnings and errors
#[derive(Debug, Clone)]
pub struct TokenLimits {
    /// Threshold above which to warn about quality/hallucination risk
    pub warn_threshold: usize,
    /// Threshold above which to error (require --chunk or shorter input)
    pub error_threshold: usize,
}

impl Default for TokenLimits {
    fn default() -> Self {
        Self {
            warn_threshold: 8000,
            error_threshold: 16000,
        }
    }
}

/// Result of checking token count against limits
#[derive(Debug)]
pub enum TokenCheck {
    /// Token count is within acceptable limits
    Ok(TokenCount),
    /// Token count exceeds warning threshold but not error threshold
    Warning {
        /// The token count
        count: TokenCount,
        /// Warning message to display
        message: String,
    },
    /// Token count exceeds error threshold
    Error {
        /// The token count
        count: TokenCount,
        /// Error message to display
        message: String,
    },
}

/// Count tokens in a string using `HuggingFace` tokenizer
///
/// # Errors
/// Returns error if the tokenizer cannot be loaded or encoding fails.
pub fn count_tokens(content: &str) -> Result<TokenCount> {
    let tokenizer = get_tokenizer()?;
    let encoding = tokenizer
        .encode(content, false)
        .map_err(|e| anyhow::anyhow!("tokenization failed: {e}"))?;
    Ok(TokenCount::new(encoding.get_tokens().len()))
}

/// Format gathered context into a string for token counting
///
/// This replicates the formatting logic from pipeline.rs to ensure
/// accurate token counts for what will actually be sent to the LLM.
fn format_context_for_counting(context: &GatheredContext) -> String {
    let mut output = String::new();

    // Format file contents
    if !context.files.is_empty() {
        output.push_str("## Files\n\n");
        for file in &context.files {
            output.push_str("### ");
            output.push_str(&file.path.display().to_string());
            output.push_str("\n\n```\n");
            output.push_str(&file.content);
            output.push_str("\n```\n\n");
        }
    }

    // Format git diff
    if let Some(ref diff) = context.git_diff
        && !diff.is_empty()
    {
        output.push_str("## Git Diff\n\n```diff\n");
        output.push_str(diff);
        output.push_str("\n```\n\n");
    }

    // Format git status
    if let Some(ref status) = context.git_status
        && !status.is_empty()
    {
        output.push_str("## Git Status\n\n```\n");
        output.push_str(status);
        output.push_str("\n```\n\n");
    }

    // Format additional context
    if let Some(ref additional) = context.additional_context
        && !additional.is_empty()
    {
        output.push_str("## Additional Context\n\n");
        output.push_str(additional);
        output.push_str("\n\n");
    }

    output
}

/// Count tokens for gathered context
///
/// # Errors
/// Returns error if the tokenizer cannot be loaded or encoding fails.
pub fn count_context_tokens(context: &GatheredContext) -> Result<TokenCount> {
    let formatted = format_context_for_counting(context);
    count_tokens(&formatted)
}

/// Check token count against limits
#[must_use]
pub fn check_token_limits(count: TokenCount, limits: &TokenLimits) -> TokenCheck {
    if count.count > limits.error_threshold {
        TokenCheck::Error {
            count,
            message: format!(
                "input has {} tokens (>{} limit); use --chunk to process in parallel or reduce input size",
                count.count, limits.error_threshold
            ),
        }
    } else if count.count > limits.warn_threshold {
        TokenCheck::Warning {
            count,
            message: format!(
                "input has {} tokens (>{} threshold); quality may be affected. consider using --chunk for better results",
                count.count, limits.warn_threshold
            ),
        }
    } else {
        TokenCheck::Ok(count)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::context::FileContext;
    use std::path::PathBuf;

    #[test]
    fn test_count_tokens_empty() {
        let count = count_tokens("").unwrap();
        assert_eq!(count.count, 0);
    }

    #[test]
    fn test_count_tokens_simple() {
        let count = count_tokens("hello world").unwrap();
        // GPT-2 tokenizes "hello world" as 2-3 tokens typically
        assert!(count.count > 0);
        assert!(count.count < 10);
    }

    #[test]
    fn test_count_tokens_code() {
        let count = count_tokens("fn main() { println!(\"Hello, world!\"); }").unwrap();
        assert!(count.count > 0);
    }

    #[test]
    fn test_token_limits_default() {
        let limits = TokenLimits::default();
        assert_eq!(limits.warn_threshold, 8000);
        assert_eq!(limits.error_threshold, 16000);
    }

    #[test]
    fn test_check_token_limits_ok() {
        let limits = TokenLimits::default();
        let count = TokenCount::new(1000);
        match check_token_limits(count, &limits) {
            TokenCheck::Ok(c) => assert_eq!(c.count, 1000),
            _ => panic!("expected Ok"),
        }
    }

    #[test]
    fn test_check_token_limits_warning() {
        let limits = TokenLimits::default();
        let count = TokenCount::new(10000);
        match check_token_limits(count, &limits) {
            TokenCheck::Warning { count: c, message } => {
                assert_eq!(c.count, 10000);
                assert!(message.contains("10000 tokens"));
                assert!(message.contains("--chunk"));
            }
            _ => panic!("expected Warning"),
        }
    }

    #[test]
    fn test_check_token_limits_error() {
        let limits = TokenLimits::default();
        let count = TokenCount::new(20000);
        match check_token_limits(count, &limits) {
            TokenCheck::Error { count: c, message } => {
                assert_eq!(c.count, 20000);
                assert!(message.contains("20000 tokens"));
                assert!(message.contains("--chunk"));
            }
            _ => panic!("expected Error"),
        }
    }

    #[test]
    fn test_count_context_tokens_empty() {
        let context = GatheredContext::default();
        let count = count_context_tokens(&context).unwrap();
        assert_eq!(count.count, 0);
    }

    #[test]
    fn test_count_context_tokens_with_files() {
        let context = GatheredContext {
            files: vec![FileContext {
                path: PathBuf::from("test.rs"),
                content: "fn main() {}".to_string(),
            }],
            git_diff: None,
            git_status: None,
            additional_context: None,
        };
        let count = count_context_tokens(&context).unwrap();
        assert!(count.count > 0);
    }

    #[test]
    fn test_count_context_tokens_with_diff() {
        let context = GatheredContext {
            files: vec![],
            git_diff: Some("+fn new() {}".to_string()),
            git_status: None,
            additional_context: None,
        };
        let count = count_context_tokens(&context).unwrap();
        assert!(count.count > 0);
    }

    #[test]
    fn test_check_token_limits_at_boundaries() {
        let limits = TokenLimits {
            warn_threshold: 100,
            error_threshold: 200,
        };

        // At warn threshold - should be warning
        let count = TokenCount::new(101);
        assert!(matches!(
            check_token_limits(count, &limits),
            TokenCheck::Warning { .. }
        ));

        // At error threshold - should be error
        let count = TokenCount::new(201);
        assert!(matches!(
            check_token_limits(count, &limits),
            TokenCheck::Error { .. }
        ));

        // Exactly at warn threshold - should be ok
        let count = TokenCount::new(100);
        assert!(matches!(
            check_token_limits(count, &limits),
            TokenCheck::Ok(_)
        ));

        // Exactly at error threshold - should be warning
        let count = TokenCount::new(200);
        assert!(matches!(
            check_token_limits(count, &limits),
            TokenCheck::Warning { .. }
        ));
    }
}
