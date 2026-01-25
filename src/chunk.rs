//! Chunking primitives for parallel processing of large inputs
//!
//! With stdin-based input, chunking is token-based rather than file-based.
//! This module provides basic infrastructure for potential future chunking needs.

use anyhow::Result;

use crate::config::TokenizerMode;
use crate::context::InputContext;
use crate::tokens::{TokenCount, TokenLimits, count_tokens};

/// A chunk of context to be processed independently
#[derive(Debug, Clone)]
pub struct Chunk {
    /// Unique identifier for this chunk
    pub id: String,
    /// The context for this chunk
    pub context: InputContext,
    /// Token count for this chunk
    pub token_count: TokenCount,
}

/// Options for chunked execution
#[derive(Debug, Clone)]
pub struct ChunkOptions {
    /// Maximum number of concurrent chunk executions
    pub max_concurrency: usize,
    /// Token limits for individual chunks
    pub token_limits: TokenLimits,
}

impl Default for ChunkOptions {
    fn default() -> Self {
        Self {
            max_concurrency: 4,
            token_limits: TokenLimits::default(),
        }
    }
}

/// Create a single chunk from input context
///
/// # Errors
/// Returns error if token counting fails (only possible with Full tokenizer mode).
pub fn create_chunk(context: &InputContext, mode: TokenizerMode) -> Result<Chunk> {
    let token_count = count_tokens(&context.content, mode)?;

    Ok(Chunk {
        id: "input-0".to_string(),
        context: context.clone(),
        token_count,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_options_default() {
        let options = ChunkOptions::default();
        assert_eq!(options.max_concurrency, 4);
        assert_eq!(options.token_limits.warn_threshold, 8000);
        assert_eq!(options.token_limits.error_threshold, 16000);
    }

    #[test]
    fn test_create_chunk() {
        let context = InputContext::new("fn main() {}");
        let chunk = create_chunk(&context, TokenizerMode::Heuristic).unwrap();

        assert_eq!(chunk.id, "input-0");
        assert_eq!(chunk.context.content, "fn main() {}");
        assert!(chunk.token_count.count > 0);
    }

    #[test]
    fn test_create_chunk_empty() {
        let context = InputContext::default();
        let chunk = create_chunk(&context, TokenizerMode::Heuristic).unwrap();

        assert_eq!(chunk.token_count.count, 0);
    }
}
