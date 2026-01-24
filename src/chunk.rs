//! Chunking primitives for parallel processing of large inputs
//!
//! Provides functionality to split large inputs into manageable chunks
//! and execute them in parallel with configurable concurrency limits.

use std::future::Future;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::Semaphore;

use crate::config::{Config, TokenizerMode};
use crate::context::GatheredContext;
use crate::rules::{CategoryResolvedRules, ResolvedRules};
use crate::tokens::{TokenCount, TokenLimits, count_tokens};

/// A chunk of context to be processed independently
#[derive(Debug, Clone)]
pub struct Chunk {
    /// Unique identifier for this chunk
    pub id: String,
    /// The context for this chunk
    pub context: GatheredContext,
    /// Token count for this chunk
    pub token_count: TokenCount,
}

/// Result from processing a single chunk
#[derive(Debug)]
pub struct ChunkResult<T> {
    /// The chunk ID this result corresponds to
    pub chunk_id: String,
    /// The result if processing succeeded
    pub result: Option<T>,
    /// The error message if processing failed
    pub error: Option<String>,
}

/// A failure from chunk processing
#[derive(Debug, Clone)]
pub struct ChunkFailure {
    /// The chunk ID that failed
    pub chunk_id: String,
    /// Error message describing the failure
    pub error: String,
}

/// Aggregated results from chunked execution
#[derive(Debug)]
pub struct ChunkedResult<T> {
    /// Successful results from each chunk
    pub results: Vec<T>,
    /// Failures from chunks that could not be processed
    pub failures: Vec<ChunkFailure>,
    /// Total number of chunks processed
    pub total_chunks: usize,
}

impl<T> ChunkedResult<T> {
    /// Returns true if all chunks were processed successfully
    #[must_use]
    pub fn all_succeeded(&self) -> bool {
        self.failures.is_empty()
    }

    /// Returns the number of successful chunks
    #[must_use]
    pub fn success_count(&self) -> usize {
        self.results.len()
    }

    /// Returns the number of failed chunks
    #[must_use]
    pub fn failure_count(&self) -> usize {
        self.failures.len()
    }
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

/// Split context into chunks by file (one file per chunk)
///
/// Each file becomes its own chunk with an independent context.
/// Git diff and status are not included in file chunks.
///
/// # Errors
/// Returns error if token counting fails (only possible with Full tokenizer mode).
pub fn chunk_by_file(context: &GatheredContext, mode: TokenizerMode) -> Result<Vec<Chunk>> {
    let mut chunks = Vec::new();

    for (i, file) in context.files.iter().enumerate() {
        let chunk_context = GatheredContext {
            files: vec![file.clone()],
            git_diff: None,
            git_status: None,
            additional_context: context.additional_context.clone(),
        };

        // Count tokens for this chunk
        let formatted = format_chunk_context(&chunk_context);
        let token_count = count_tokens(&formatted, mode)?;

        chunks.push(Chunk {
            id: format!("file-{}-{}", i, file.path.display()),
            context: chunk_context,
            token_count,
        });
    }

    // If there's a git diff without files, create a chunk for it
    if context.files.is_empty()
        && let Some(ref diff) = context.git_diff
        && !diff.is_empty()
    {
        let chunk_context = GatheredContext {
            files: Vec::new(),
            git_diff: Some(diff.clone()),
            git_status: context.git_status.clone(),
            additional_context: context.additional_context.clone(),
        };

        let formatted = format_chunk_context(&chunk_context);
        let token_count = count_tokens(&formatted, mode)?;

        chunks.push(Chunk {
            id: "diff-0".to_string(),
            context: chunk_context,
            token_count,
        });
    }

    Ok(chunks)
}

/// Format chunk context for token counting
fn format_chunk_context(context: &GatheredContext) -> String {
    let mut output = String::new();

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

    if let Some(ref diff) = context.git_diff
        && !diff.is_empty()
    {
        output.push_str("## Git Diff\n\n```diff\n");
        output.push_str(diff);
        output.push_str("\n```\n\n");
    }

    if let Some(ref additional) = context.additional_context
        && !additional.is_empty()
    {
        output.push_str("## Additional Context\n\n");
        output.push_str(additional);
        output.push_str("\n\n");
    }

    output
}

/// Execute chunks in parallel with concurrency control
///
/// Spawns tasks for each chunk and executes them with a semaphore-controlled
/// concurrency limit. Results are collected and returned with any failures.
///
/// # Type Parameters
/// * `T` - The result type from processing each chunk
/// * `F` - The function type for processing chunks
/// * `Fut` - The future type returned by the processing function
pub async fn execute_chunked<T, F, Fut>(
    config: &Config,
    chunks: Vec<Chunk>,
    options: &ChunkOptions,
    execute_fn: F,
) -> ChunkedResult<T>
where
    T: Send + 'static,
    F: Fn(Config, Chunk) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T>> + Send,
{
    let total_chunks = chunks.len();

    if chunks.is_empty() {
        return ChunkedResult {
            results: Vec::new(),
            failures: Vec::new(),
            total_chunks: 0,
        };
    }

    let semaphore = Arc::new(Semaphore::new(options.max_concurrency));
    let config = config.clone();

    let handles: Vec<_> = chunks
        .into_iter()
        .map(|chunk| {
            let sem = Arc::clone(&semaphore);
            let cfg = config.clone();
            let exec = execute_fn.clone();
            let chunk_id = chunk.id.clone();

            tokio::spawn(async move {
                // Acquire semaphore permit before execution
                let _permit = sem
                    .acquire()
                    .await
                    .map_err(|e| format!("semaphore error: {e}"))?;

                tracing::debug!(chunk_id = %chunk_id, "executing chunk");

                match exec(cfg, chunk).await {
                    Ok(result) => Ok(ChunkResult {
                        chunk_id,
                        result: Some(result),
                        error: None,
                    }),
                    Err(e) => Ok(ChunkResult {
                        chunk_id,
                        result: None,
                        error: Some(e.to_string()),
                    }),
                }
            })
        })
        .collect();

    let mut results = Vec::new();
    let mut failures = Vec::new();

    for handle in handles {
        match handle.await {
            Ok(Ok(chunk_result)) => {
                if let Some(result) = chunk_result.result {
                    results.push(result);
                } else if let Some(error) = chunk_result.error {
                    failures.push(ChunkFailure {
                        chunk_id: chunk_result.chunk_id,
                        error,
                    });
                }
            }
            Ok(Err(e)) => {
                failures.push(ChunkFailure {
                    chunk_id: "unknown".to_string(),
                    error: e,
                });
            }
            Err(e) => {
                failures.push(ChunkFailure {
                    chunk_id: "unknown".to_string(),
                    error: format!("task join error: {e}"),
                });
            }
        }
    }

    ChunkedResult {
        results,
        failures,
        total_chunks,
    }
}

/// A chunk for category-based processing
///
/// Used when `--checks` is specified to enable per-category LLM calls.
#[derive(Debug, Clone)]
pub struct CategoryChunk {
    /// The category name (e.g., "security", "style")
    pub category: String,
    /// Rules for this category
    pub rules: ResolvedRules,
    /// The context to review
    pub context: GatheredContext,
    /// Token count for this chunk
    pub token_count: TokenCount,
}

/// Split context into chunks by category (one category per chunk)
///
/// Each category becomes its own chunk with the full context.
/// Used when `--checks` is specified without `--chunk`.
///
/// # Errors
/// Returns error if token counting fails (only possible with Full tokenizer mode).
pub fn chunk_by_category(
    context: &GatheredContext,
    category_rules: &CategoryResolvedRules,
    mode: TokenizerMode,
) -> Result<Vec<CategoryChunk>> {
    let mut chunks = Vec::new();

    for (category, rules) in category_rules.iter() {
        if rules.is_empty() {
            continue;
        }

        // Count tokens for context + rules prompt section
        let rules_section = rules.to_category_prompt_section(category);
        let formatted = format!("{}\n{}", format_chunk_context(context), rules_section);
        let token_count = count_tokens(&formatted, mode)?;

        chunks.push(CategoryChunk {
            category: category.clone(),
            rules: rules.clone(),
            context: context.clone(),
            token_count,
        });
    }

    // Sort by category name for consistent output
    chunks.sort_by(|a, b| a.category.cmp(&b.category));

    Ok(chunks)
}

/// Split context into nested chunks (category × file)
///
/// Creates a chunk for each (category, file) pair. Used when both
/// `--checks` and `--chunk` are specified for maximum detail.
///
/// # Errors
/// Returns error if token counting fails (only possible with Full tokenizer mode).
pub fn chunk_by_category_and_file(
    context: &GatheredContext,
    category_rules: &CategoryResolvedRules,
    mode: TokenizerMode,
) -> Result<Vec<CategoryChunk>> {
    let mut chunks = Vec::new();

    // If no files, just return category chunks with diff context
    if context.files.is_empty() {
        return chunk_by_category(context, category_rules, mode);
    }

    for (category, rules) in category_rules.iter() {
        if rules.is_empty() {
            continue;
        }

        for file in &context.files {
            // Create a single-file context
            let file_context = GatheredContext {
                files: vec![file.clone()],
                git_diff: None,
                git_status: None,
                additional_context: context.additional_context.clone(),
            };

            // Count tokens for context + rules
            let rules_section = rules.to_category_prompt_section(category);
            let formatted = format!("{}\n{}", format_chunk_context(&file_context), rules_section);
            let token_count = count_tokens(&formatted, mode)?;

            chunks.push(CategoryChunk {
                category: category.clone(),
                rules: rules.clone(),
                context: file_context,
                token_count,
            });
        }
    }

    // Sort by category then file path for consistent output
    chunks.sort_by(|a, b| {
        let cat_cmp = a.category.cmp(&b.category);
        if cat_cmp.is_eq() {
            let a_path = a.context.files.first().map(|f| &f.path);
            let b_path = b.context.files.first().map(|f| &f.path);
            a_path.cmp(&b_path)
        } else {
            cat_cmp
        }
    });

    Ok(chunks)
}

/// Result from processing a category chunk
#[derive(Debug)]
pub struct CategoryChunkResult<T> {
    /// The category this result corresponds to
    pub category: String,
    /// The file path if this was a nested chunk
    pub file: Option<String>,
    /// The result if processing succeeded
    pub result: Option<T>,
    /// The error message if processing failed
    pub error: Option<String>,
}

/// Execute category chunks in parallel with concurrency control
///
/// Similar to `execute_chunked` but specialized for category chunks.
pub async fn execute_category_chunks<T, F, Fut>(
    config: &Config,
    chunks: Vec<CategoryChunk>,
    options: &ChunkOptions,
    execute_fn: F,
) -> ChunkedResult<T>
where
    T: Send + 'static,
    F: Fn(Config, CategoryChunk) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<T>> + Send,
{
    let total_chunks = chunks.len();

    if chunks.is_empty() {
        return ChunkedResult {
            results: Vec::new(),
            failures: Vec::new(),
            total_chunks: 0,
        };
    }

    let semaphore = Arc::new(Semaphore::new(options.max_concurrency));
    let config = config.clone();

    let handles: Vec<_> = chunks
        .into_iter()
        .map(|chunk| {
            let sem = Arc::clone(&semaphore);
            let cfg = config.clone();
            let exec = execute_fn.clone();
            let chunk_id = format!(
                "{}-{}",
                chunk.category,
                chunk
                    .context
                    .files
                    .first()
                    .map_or_else(|| "diff".to_string(), |f| f.path.display().to_string())
            );

            tokio::spawn(async move {
                // Acquire semaphore permit before execution
                let _permit = sem
                    .acquire()
                    .await
                    .map_err(|e| format!("semaphore error: {e}"))?;

                tracing::debug!(chunk_id = %chunk_id, "executing category chunk");

                match exec(cfg, chunk).await {
                    Ok(result) => Ok(ChunkResult {
                        chunk_id,
                        result: Some(result),
                        error: None,
                    }),
                    Err(e) => Ok(ChunkResult {
                        chunk_id,
                        result: None,
                        error: Some(e.to_string()),
                    }),
                }
            })
        })
        .collect();

    let mut results = Vec::new();
    let mut failures = Vec::new();

    for handle in handles {
        match handle.await {
            Ok(Ok(chunk_result)) => {
                if let Some(result) = chunk_result.result {
                    results.push(result);
                } else if let Some(error) = chunk_result.error {
                    failures.push(ChunkFailure {
                        chunk_id: chunk_result.chunk_id,
                        error,
                    });
                }
            }
            Ok(Err(e)) => {
                failures.push(ChunkFailure {
                    chunk_id: "unknown".to_string(),
                    error: e,
                });
            }
            Err(e) => {
                failures.push(ChunkFailure {
                    chunk_id: "unknown".to_string(),
                    error: format!("task join error: {e}"),
                });
            }
        }
    }

    ChunkedResult {
        results,
        failures,
        total_chunks,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::context::FileContext;
    use std::path::PathBuf;

    #[test]
    fn test_chunk_by_file_empty() {
        let context = GatheredContext::default();
        let chunks = chunk_by_file(&context, TokenizerMode::Heuristic).unwrap();
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_chunk_by_file_single_file() {
        let context = GatheredContext {
            files: vec![FileContext {
                path: PathBuf::from("test.rs"),
                content: "fn main() {}".to_string(),
            }],
            git_diff: None,
            git_status: None,
            additional_context: None,
        };

        let chunks = chunk_by_file(&context, TokenizerMode::Heuristic).unwrap();
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].id.contains("test.rs"));
        assert_eq!(chunks[0].context.files.len(), 1);
    }

    #[test]
    fn test_chunk_by_file_multiple_files() {
        let context = GatheredContext {
            files: vec![
                FileContext {
                    path: PathBuf::from("a.rs"),
                    content: "fn a() {}".to_string(),
                },
                FileContext {
                    path: PathBuf::from("b.rs"),
                    content: "fn b() {}".to_string(),
                },
                FileContext {
                    path: PathBuf::from("c.rs"),
                    content: "fn c() {}".to_string(),
                },
            ],
            git_diff: None,
            git_status: None,
            additional_context: Some("Context for all".to_string()),
        };

        let chunks = chunk_by_file(&context, TokenizerMode::Heuristic).unwrap();
        assert_eq!(chunks.len(), 3);

        // Each chunk should have one file
        for chunk in &chunks {
            assert_eq!(chunk.context.files.len(), 1);
            // Additional context is propagated to each chunk
            assert_eq!(
                chunk.context.additional_context,
                Some("Context for all".to_string())
            );
        }
    }

    #[test]
    fn test_chunk_by_file_diff_only() {
        let context = GatheredContext {
            files: Vec::new(),
            git_diff: Some("+fn new() {}".to_string()),
            git_status: Some("M file.rs".to_string()),
            additional_context: None,
        };

        let chunks = chunk_by_file(&context, TokenizerMode::Heuristic).unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].id, "diff-0");
        assert!(chunks[0].context.git_diff.is_some());
    }

    #[test]
    fn test_chunk_options_default() {
        let options = ChunkOptions::default();
        assert_eq!(options.max_concurrency, 4);
        assert_eq!(options.token_limits.warn_threshold, 8000);
        assert_eq!(options.token_limits.error_threshold, 16000);
    }

    #[test]
    fn test_chunked_result_all_succeeded() {
        let result: ChunkedResult<String> = ChunkedResult {
            results: vec!["a".to_string(), "b".to_string()],
            failures: Vec::new(),
            total_chunks: 2,
        };
        assert!(result.all_succeeded());
        assert_eq!(result.success_count(), 2);
        assert_eq!(result.failure_count(), 0);
    }

    #[test]
    fn test_chunked_result_with_failures() {
        let result: ChunkedResult<String> = ChunkedResult {
            results: vec!["a".to_string()],
            failures: vec![ChunkFailure {
                chunk_id: "chunk-1".to_string(),
                error: "failed".to_string(),
            }],
            total_chunks: 2,
        };
        assert!(!result.all_succeeded());
        assert_eq!(result.success_count(), 1);
        assert_eq!(result.failure_count(), 1);
    }

    #[tokio::test]
    async fn test_execute_chunked_empty() {
        let config = Config::default();
        let chunks: Vec<Chunk> = Vec::new();
        let options = ChunkOptions::default();

        let result: ChunkedResult<String> =
            execute_chunked(&config, chunks, &options, |_cfg, _chunk| async {
                Ok("result".to_string())
            })
            .await;

        assert_eq!(result.total_chunks, 0);
        assert!(result.results.is_empty());
        assert!(result.failures.is_empty());
    }

    #[tokio::test]
    async fn test_execute_chunked_success() {
        let config = Config::default();
        let chunks = vec![
            Chunk {
                id: "chunk-0".to_string(),
                context: GatheredContext::default(),
                token_count: TokenCount::new(100),
            },
            Chunk {
                id: "chunk-1".to_string(),
                context: GatheredContext::default(),
                token_count: TokenCount::new(100),
            },
        ];
        let options = ChunkOptions::default();

        let result: ChunkedResult<String> =
            execute_chunked(&config, chunks, &options, |_cfg, chunk| async move {
                Ok(format!("processed-{}", chunk.id))
            })
            .await;

        assert_eq!(result.total_chunks, 2);
        assert_eq!(result.results.len(), 2);
        assert!(result.failures.is_empty());
    }

    #[tokio::test]
    async fn test_execute_chunked_partial_failure() {
        let config = Config::default();
        let chunks = vec![
            Chunk {
                id: "success".to_string(),
                context: GatheredContext::default(),
                token_count: TokenCount::new(100),
            },
            Chunk {
                id: "fail".to_string(),
                context: GatheredContext::default(),
                token_count: TokenCount::new(100),
            },
        ];
        let options = ChunkOptions::default();

        let result: ChunkedResult<String> =
            execute_chunked(&config, chunks, &options, |_cfg, chunk| async move {
                if chunk.id == "fail" {
                    Err(anyhow::anyhow!("intentional failure"))
                } else {
                    Ok("success".to_string())
                }
            })
            .await;

        assert_eq!(result.total_chunks, 2);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.failures.len(), 1);
        assert!(result.failures[0].error.contains("intentional failure"));
    }

    #[tokio::test]
    async fn test_execute_chunked_concurrency_limit() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let config = Config::default();
        let chunks: Vec<Chunk> = (0..10)
            .map(|i| Chunk {
                id: format!("chunk-{i}"),
                context: GatheredContext::default(),
                token_count: TokenCount::new(100),
            })
            .collect();

        let options = ChunkOptions {
            max_concurrency: 2,
            token_limits: TokenLimits::default(),
        };

        let concurrent_count = Arc::new(AtomicUsize::new(0));
        let max_concurrent = Arc::new(AtomicUsize::new(0));

        let cc = Arc::clone(&concurrent_count);
        let mc = Arc::clone(&max_concurrent);

        let result: ChunkedResult<String> =
            execute_chunked(&config, chunks, &options, move |_cfg, chunk| {
                let cc = Arc::clone(&cc);
                let mc = Arc::clone(&mc);
                async move {
                    // Track concurrent executions
                    let current = cc.fetch_add(1, Ordering::SeqCst) + 1;
                    mc.fetch_max(current, Ordering::SeqCst);

                    // Simulate some work
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

                    cc.fetch_sub(1, Ordering::SeqCst);
                    Ok(chunk.id)
                }
            })
            .await;

        assert_eq!(result.total_chunks, 10);
        assert_eq!(result.results.len(), 10);
        // Max concurrent should be at most 2 (the limit)
        assert!(max_concurrent.load(Ordering::SeqCst) <= 2);
    }

    // Category chunking tests
    use crate::rules::{CategoryResolvedRules, ResolvedRule, ResolvedRules, Severity};

    fn create_test_category_rules() -> CategoryResolvedRules {
        let mut category_rules = CategoryResolvedRules::default();

        category_rules.insert(
            "security".to_string(),
            ResolvedRules {
                rules: vec![ResolvedRule {
                    category: "security".to_string(),
                    name: "no-unwrap".to_string(),
                    description: "Avoid unwrap".to_string(),
                    severity: Severity::Error,
                }],
            },
        );

        category_rules.insert(
            "style".to_string(),
            ResolvedRules {
                rules: vec![ResolvedRule {
                    category: "style".to_string(),
                    name: "docs".to_string(),
                    description: "Add docs".to_string(),
                    severity: Severity::Warning,
                }],
            },
        );

        category_rules
    }

    #[test]
    fn test_chunk_by_category_empty() {
        let context = GatheredContext::default();
        let category_rules = CategoryResolvedRules::default();
        let chunks =
            chunk_by_category(&context, &category_rules, TokenizerMode::Heuristic).unwrap();
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_chunk_by_category_multiple() {
        let context = GatheredContext {
            files: vec![FileContext {
                path: PathBuf::from("test.rs"),
                content: "fn main() {}".to_string(),
            }],
            git_diff: None,
            git_status: None,
            additional_context: None,
        };

        let category_rules = create_test_category_rules();
        let chunks =
            chunk_by_category(&context, &category_rules, TokenizerMode::Heuristic).unwrap();

        assert_eq!(chunks.len(), 2);
        // Should be sorted by category name
        assert_eq!(chunks[0].category, "security");
        assert_eq!(chunks[1].category, "style");

        // Each chunk should have the full context
        assert_eq!(chunks[0].context.files.len(), 1);
        assert_eq!(chunks[1].context.files.len(), 1);
    }

    #[test]
    fn test_chunk_by_category_and_file_empty_files() {
        let context = GatheredContext {
            files: Vec::new(),
            git_diff: Some("+fn new() {}".to_string()),
            git_status: None,
            additional_context: None,
        };

        let category_rules = create_test_category_rules();
        let chunks =
            chunk_by_category_and_file(&context, &category_rules, TokenizerMode::Heuristic)
                .unwrap();

        // Should fall back to category-only chunks when no files
        assert_eq!(chunks.len(), 2);
    }

    #[test]
    fn test_chunk_by_category_and_file_nested() {
        let context = GatheredContext {
            files: vec![
                FileContext {
                    path: PathBuf::from("a.rs"),
                    content: "fn a() {}".to_string(),
                },
                FileContext {
                    path: PathBuf::from("b.rs"),
                    content: "fn b() {}".to_string(),
                },
            ],
            git_diff: None,
            git_status: None,
            additional_context: None,
        };

        let category_rules = create_test_category_rules();
        let chunks =
            chunk_by_category_and_file(&context, &category_rules, TokenizerMode::Heuristic)
                .unwrap();

        // 2 categories × 2 files = 4 chunks
        assert_eq!(chunks.len(), 4);

        // Each chunk should have exactly one file
        for chunk in &chunks {
            assert_eq!(chunk.context.files.len(), 1);
        }

        // Check sorting: security-a.rs, security-b.rs, style-a.rs, style-b.rs
        assert_eq!(chunks[0].category, "security");
        assert!(
            chunks[0].context.files[0]
                .path
                .to_string_lossy()
                .contains("a.rs")
        );
        assert_eq!(chunks[1].category, "security");
        assert!(
            chunks[1].context.files[0]
                .path
                .to_string_lossy()
                .contains("b.rs")
        );
        assert_eq!(chunks[2].category, "style");
        assert_eq!(chunks[3].category, "style");
    }

    #[tokio::test]
    async fn test_execute_category_chunks_success() {
        let config = Config::default();
        let category_rules = create_test_category_rules();
        let context = GatheredContext {
            files: vec![FileContext {
                path: PathBuf::from("test.rs"),
                content: "fn main() {}".to_string(),
            }],
            git_diff: None,
            git_status: None,
            additional_context: None,
        };

        let chunks =
            chunk_by_category(&context, &category_rules, TokenizerMode::Heuristic).unwrap();
        let options = ChunkOptions::default();

        let result: ChunkedResult<String> =
            execute_category_chunks(&config, chunks, &options, |_cfg, chunk| async move {
                Ok(format!("reviewed-{}", chunk.category))
            })
            .await;

        assert_eq!(result.total_chunks, 2);
        assert_eq!(result.results.len(), 2);
        assert!(result.failures.is_empty());
    }
}
