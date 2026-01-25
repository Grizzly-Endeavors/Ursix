//! Input context for LLM calls
//!
//! Simple wrapper around input content. All context gathering happens
//! externally (via shell pipes) before calling the CLI.

/// Input content for an LLM call
#[derive(Debug, Clone, Default)]
pub struct InputContext {
    /// The raw input content from stdin or --from
    pub content: String,
}

impl InputContext {
    /// Create a new input context from content
    #[must_use]
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
        }
    }

    /// Check if the context is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.content.trim().is_empty()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_input_context_new() {
        let ctx = InputContext::new("test content");
        assert_eq!(ctx.content, "test content");
    }

    #[test]
    fn test_input_context_default() {
        let ctx = InputContext::default();
        assert!(ctx.content.is_empty());
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
}
