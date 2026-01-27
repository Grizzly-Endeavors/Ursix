//! Token counting for input validation and chunking decisions
//!
//! Provides two token counting modes:
//! - **Heuristic** (default): Fast character-based approximation (~4 chars per token)
//! - **Full**: Accurate `HuggingFace` tokenizer (GPT-2), with network/CPU overhead
//!
//! The heuristic mode is recommended for most use cases. Full mode is only needed
//! when precise token counts are critical.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use tokenizers::Tokenizer;

use crate::config::TokenizerMode;
use crate::context::InputContext;

/// Track whether we've shown the full tokenizer warning
static FULL_TOKENIZER_WARNING_SHOWN: AtomicBool = AtomicBool::new(false);

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

/// Count tokens using heuristic approximation
///
/// Uses ~4 characters per token as a reasonable approximation for English text
/// and code. This is fast and requires no external dependencies.
///
/// The 4 chars/token ratio is based on empirical analysis of GPT tokenizers:
/// - English prose: ~4-5 chars/token
/// - Code: ~3-4 chars/token (more symbols)
/// - Mixed content: ~4 chars/token
#[must_use]
pub fn count_tokens_heuristic(content: &str) -> TokenCount {
    // Use 4 characters per token as approximation
    // This is slightly conservative (may overcount) which is safer for limit checking
    let char_count = content.chars().count();
    TokenCount::new(char_count.div_ceil(4))
}

/// Count tokens using full `HuggingFace` tokenizer
///
/// Uses GPT-2 tokenizer as a reasonable cross-provider approximation.
/// More accurate than heuristic but has overhead:
/// - First call downloads tokenizer data (~2MB)
/// - Each call has CPU overhead for tokenization
///
/// # Errors
/// Returns error if the tokenizer cannot be loaded or encoding fails.
pub fn count_tokens_full(content: &str) -> Result<TokenCount> {
    let tokenizer = get_tokenizer()?;
    let encoding = tokenizer
        .encode(content, false)
        .map_err(|e| anyhow::anyhow!("tokenization failed: {e}"))?;
    Ok(TokenCount::new(encoding.get_tokens().len()))
}

/// Count tokens in a string using the specified mode
///
/// # Errors
/// Returns error if full tokenizer mode is used and the tokenizer cannot be loaded.
pub fn count_tokens(content: &str, mode: TokenizerMode) -> Result<TokenCount> {
    match mode {
        TokenizerMode::Heuristic => Ok(count_tokens_heuristic(content)),
        TokenizerMode::Full => {
            // Show warning once about overhead
            if !FULL_TOKENIZER_WARNING_SHOWN.swap(true, Ordering::Relaxed) {
                tracing::warn!(
                    "using full tokenizer mode; this has additional overhead \
                     (network on first use, CPU per call). Consider 'heuristic' mode \
                     unless precise token counts are critical."
                );
            }
            count_tokens_full(content)
        }
    }
}

/// Count tokens for input context
///
/// # Errors
/// Returns error if full tokenizer mode is used and the tokenizer cannot be loaded.
pub fn count_context_tokens(context: &InputContext, mode: TokenizerMode) -> Result<TokenCount> {
    count_tokens(context.content(), mode)
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

    #[test]
    fn test_count_tokens_heuristic_empty() {
        let count = count_tokens_heuristic("");
        assert_eq!(count.count, 0);
    }

    #[test]
    fn test_count_tokens_heuristic_simple() {
        // "hello world" = 11 chars, ~3 tokens with heuristic
        let count = count_tokens_heuristic("hello world");
        assert!(count.count > 0);
        assert!(count.count < 10);
    }

    #[test]
    fn test_count_tokens_heuristic_code() {
        let count = count_tokens_heuristic("fn main() { println!(\"Hello, world!\"); }");
        assert!(count.count > 0);
    }

    #[test]
    fn test_count_tokens_heuristic_approximation() {
        // 100 chars should be ~25 tokens (100/4)
        let content = "a".repeat(100);
        let count = count_tokens_heuristic(&content);
        assert_eq!(count.count, 25);

        // 99 chars should round up to 25 tokens ((99+3)/4)
        let content = "a".repeat(99);
        let count = count_tokens_heuristic(&content);
        assert_eq!(count.count, 25);

        // 101 chars should be 26 tokens ((101+3)/4)
        let content = "a".repeat(101);
        let count = count_tokens_heuristic(&content);
        assert_eq!(count.count, 26);
    }

    #[test]
    fn test_count_tokens_full_empty() {
        let count = count_tokens_full("").unwrap();
        assert_eq!(count.count, 0);
    }

    #[test]
    fn test_count_tokens_full_simple() {
        let count = count_tokens_full("hello world").unwrap();
        // GPT-2 tokenizes "hello world" as 2-3 tokens typically
        assert!(count.count > 0);
        assert!(count.count < 10);
    }

    #[test]
    fn test_count_tokens_full_code() {
        let count = count_tokens_full("fn main() { println!(\"Hello, world!\"); }").unwrap();
        assert!(count.count > 0);
    }

    #[test]
    fn test_count_tokens_with_mode() {
        // Heuristic mode
        let count = count_tokens("hello world", TokenizerMode::Heuristic).unwrap();
        assert!(count.count > 0);

        // Full mode
        let count = count_tokens("hello world", TokenizerMode::Full).unwrap();
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
        let context = InputContext::default();
        let count = count_context_tokens(&context, TokenizerMode::Heuristic).unwrap();
        assert_eq!(count.count, 0);
    }

    #[test]
    fn test_count_context_tokens_with_content() {
        let context = InputContext::new("fn main() {}");
        let count = count_context_tokens(&context, TokenizerMode::Heuristic).unwrap();
        assert!(count.count > 0);
    }

    #[test]
    fn test_count_context_tokens_full_mode() {
        let context = InputContext::new("fn main() {}");
        let count = count_context_tokens(&context, TokenizerMode::Full).unwrap();
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
