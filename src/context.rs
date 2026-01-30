//! Input context for LLM calls
//!
//! Simple wrapper around input content. All context gathering happens
//! externally (via shell pipes) before calling the CLI.

use std::sync::Arc;

/// Input content for an LLM call
///
/// Uses `Arc<str>` internally to allow cheap cloning when sharing
/// the same content across multiple parallel operations (e.g., chunked processing).
#[derive(Debug, Clone, Default)]
pub(crate) struct InputContext {
    /// The raw input content from stdin or --from
    content: Arc<str>,
}

impl InputContext {
    /// Create a new input context from content
    #[must_use]
    pub(crate) fn new(content: impl AsRef<str>) -> Self {
        Self {
            content: Arc::from(content.as_ref()),
        }
    }

    /// Get the content as a string slice
    #[must_use]
    pub(crate) fn content(&self) -> &str {
        &self.content
    }

    /// Check if the context is empty
    #[must_use]
    pub(crate) fn is_empty(&self) -> bool {
        self.content.trim().is_empty()
    }

    /// Get the underlying Arc for sharing
    ///
    /// Useful for verifying Arc sharing in tests.
    #[must_use]
    pub(crate) fn content_arc(&self) -> &Arc<str> {
        &self.content
    }

    /// Format content with line numbers for LLM review
    ///
    /// Returns content with each line prefixed by its line number,
    /// making it easier for the LLM to reference specific lines accurately.
    #[must_use]
    pub(crate) fn content_with_line_numbers(&self) -> String {
        self.content
            .lines()
            .enumerate()
            .map(|(i, line)| format!("{:>4}| {}", i + 1, line))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_context_new() {
        let ctx = InputContext::new("test content");
        assert_eq!(ctx.content(), "test content");
    }

    #[test]
    fn test_input_context_default() {
        let ctx = InputContext::default();
        assert!(ctx.content().is_empty());
        assert!(ctx.is_empty());
    }

    #[test]
    fn test_input_context_is_empty() {
        let empty = InputContext::new("");
        assert!(empty.is_empty());

        let whitespace = InputContext::new("  \n\t  ");
        assert!(whitespace.is_empty());

        let content = InputContext::new("hello");
        assert!(!content.is_empty());
    }

    #[test]
    fn test_input_context_content_method() {
        let ctx = InputContext::new("fn main() {}");
        assert_eq!(ctx.content(), "fn main() {}");

        // Verify accessor returns correct slice
        let ctx2 = InputContext::new("");
        assert_eq!(ctx2.content(), "");
    }

    #[test]
    fn test_input_context_clone_shares_content() {
        let ctx1 = InputContext::new("shared content");
        let ctx2 = ctx1.clone();

        // Verify Arc sharing via pointer equality
        assert!(Arc::ptr_eq(ctx1.content_arc(), ctx2.content_arc()));

        // Verify content is the same
        assert_eq!(ctx1.content(), ctx2.content());
    }
}
