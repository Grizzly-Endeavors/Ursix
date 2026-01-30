//! Chunking primitives for parallel processing of large inputs
//!
//! Supports both token-based chunking and file-based chunking for diffs.
//! - Token-based: Split at token boundaries for arbitrary text
//! - File-based: Split diffs at file boundaries for review operations

use anyhow::Result;

use crate::config::TokenizerMode;
use crate::context::InputContext;
use crate::tokens::{TokenCount, TokenLimits, count_tokens};

/// A chunk of context to be processed independently
#[derive(Debug, Clone)]
pub(crate) struct Chunk {
    /// Unique identifier for this chunk
    pub id: String,
    /// The context for this chunk
    pub context: InputContext,
    /// Token count for this chunk
    pub token_count: TokenCount,
}

/// Options for chunked execution
#[derive(Debug, Clone)]
pub(crate) struct ChunkOptions {
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
pub(crate) fn create_chunk(context: &InputContext, mode: TokenizerMode) -> Result<Chunk> {
    let token_count = count_tokens(context.content(), mode)?;

    Ok(Chunk {
        id: "input-0".to_string(),
        context: context.clone(),
        token_count,
    })
}

/// A chunk representing a single file's diff section
#[derive(Debug, Clone)]
pub(crate) struct DiffChunk {
    /// The file path being modified
    pub file_path: String,
    /// The full diff section for this file
    pub content: String,
}

/// Split a git diff into per-file chunks
///
/// Identifies file boundaries in unified diff format and splits the diff
/// so each chunk contains the complete diff for a single file.
///
/// Returns an empty vector if the input doesn't appear to be a valid diff.
#[must_use]
pub(crate) fn chunk_diff_by_file(diff_content: &str) -> Vec<DiffChunk> {
    let mut chunks = Vec::new();
    let mut current_file: Option<String> = None;
    let mut current_content = String::new();

    for line in diff_content.lines() {
        // Detect file boundary: "diff --git a/path b/path"
        if line.starts_with("diff --git ") {
            // Save previous chunk if any
            if let Some(file_path) = current_file.take()
                && !current_content.trim().is_empty()
            {
                chunks.push(DiffChunk {
                    file_path,
                    content: std::mem::take(&mut current_content),
                });
            }

            // Extract file path from "diff --git a/path b/path"
            if let Some(path) = extract_file_path_from_diff_header(line) {
                current_file = Some(path);
            }
        }

        // Accumulate content for current file
        if current_file.is_some() {
            current_content.push_str(line);
            current_content.push('\n');
        }
    }

    // Don't forget the last chunk
    if let Some(file_path) = current_file
        && !current_content.trim().is_empty()
    {
        chunks.push(DiffChunk {
            file_path,
            content: current_content,
        });
    }

    chunks
}

/// Extract file path from a git diff header line
///
/// Input: "diff --git a/src/main.rs b/src/main.rs"
/// Output: Some("src/main.rs")
fn extract_file_path_from_diff_header(line: &str) -> Option<String> {
    // Must start with "diff --git"
    if !line.starts_with("diff --git ") {
        return None;
    }

    // Format: "diff --git a/<path> b/<path>"
    // We extract the b/ path (the destination) as it's the more relevant one for renames
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 4 {
        let b_path = parts.last()?;
        // Strip the "b/" prefix (required for valid git diff format)
        if let Some(path) = b_path.strip_prefix("b/") {
            return Some(path.to_string());
        }
    }
    None
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code uses unwrap for clarity")]
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
        assert_eq!(chunk.context.content(), "fn main() {}");
        assert!(chunk.token_count.count > 0);
    }

    #[test]
    fn test_create_chunk_empty() {
        let context = InputContext::default();
        let chunk = create_chunk(&context, TokenizerMode::Heuristic).unwrap();

        assert_eq!(chunk.token_count.count, 0);
    }

    #[test]
    fn test_chunk_diff_by_file_single_file() {
        let diff = r#"diff --git a/src/main.rs b/src/main.rs
index 1234567..abcdefg 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 fn main() {
+    println!("hello");
 }
"#;
        let chunks = chunk_diff_by_file(diff);
        assert_eq!(chunks.len(), 1);
        assert_eq!(
            chunks.get(0).map(|c| &c.file_path),
            Some(&"src/main.rs".to_string())
        );
        assert!(
            chunks
                .get(0)
                .map_or(false, |c| c.content.contains("diff --git"))
        );
        assert!(
            chunks
                .get(0)
                .map_or(false, |c| c.content.contains("println"))
        );
    }

    #[test]
    fn test_chunk_diff_by_file_multiple_files() {
        let diff = r#"diff --git a/src/main.rs b/src/main.rs
index 1234567..abcdefg 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 fn main() {
+    println!("hello");
 }
diff --git a/src/lib.rs b/src/lib.rs
index 2222222..3333333 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1 +1,2 @@
+pub mod utils;
 pub fn lib_fn() {}
"#;
        let chunks = chunk_diff_by_file(diff);
        assert_eq!(chunks.len(), 2);
        assert_eq!(
            chunks.get(0).map(|c| &c.file_path),
            Some(&"src/main.rs".to_string())
        );
        assert_eq!(
            chunks.get(1).map(|c| &c.file_path),
            Some(&"src/lib.rs".to_string())
        );
        assert!(
            chunks
                .get(0)
                .map_or(false, |c| c.content.contains("println"))
        );
        assert!(chunks.get(1).map_or(false, |c| c.content.contains("utils")));
    }

    #[test]
    fn test_chunk_diff_by_file_empty_input() {
        let chunks = chunk_diff_by_file("");
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_chunk_diff_by_file_not_a_diff() {
        let content = "fn main() { println!(\"hello\"); }";
        let chunks = chunk_diff_by_file(content);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_chunk_diff_by_file_new_file() {
        let diff = "diff --git a/new_file.rs b/new_file.rs
new file mode 100644
index 0000000..1234567
--- /dev/null
+++ b/new_file.rs
@@ -0,0 +1,3 @@
+fn new() {
+    todo!()
+}
";
        let chunks = chunk_diff_by_file(diff);
        assert_eq!(chunks.len(), 1);
        assert_eq!(
            chunks.get(0).map(|c| &c.file_path),
            Some(&"new_file.rs".to_string())
        );
    }

    #[test]
    fn test_chunk_diff_by_file_deleted_file() {
        let diff = "diff --git a/old_file.rs b/old_file.rs
deleted file mode 100644
index 1234567..0000000
--- a/old_file.rs
+++ /dev/null
@@ -1,3 +0,0 @@
-fn old() {
-    todo!()
-}
";
        let chunks = chunk_diff_by_file(diff);
        assert_eq!(chunks.len(), 1);
        assert_eq!(
            chunks.get(0).map(|c| &c.file_path),
            Some(&"old_file.rs".to_string())
        );
    }

    #[test]
    fn test_extract_file_path_from_diff_header() {
        assert_eq!(
            extract_file_path_from_diff_header("diff --git a/src/main.rs b/src/main.rs"),
            Some("src/main.rs".to_string())
        );
        assert_eq!(
            extract_file_path_from_diff_header("diff --git a/old.rs b/new.rs"),
            Some("new.rs".to_string())
        );
        // Non-diff lines return None
        assert_eq!(
            extract_file_path_from_diff_header("not a diff header"),
            None
        );
        assert_eq!(extract_file_path_from_diff_header("--- a/file.rs"), None);
        // Missing b/ prefix returns None
        assert_eq!(
            extract_file_path_from_diff_header("diff --git a/file.rs file.rs"),
            None
        );
    }
}
