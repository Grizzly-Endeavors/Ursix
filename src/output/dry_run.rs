//! Output types for `--dry-run` mode
//!
//! Shows token estimation and chunking plan without making LLM calls.

use super::{CommandOutput, ExitCode, ExitStatus};
use serde::Serialize;

/// Information about how chunks would be organized
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ChunkPlan {
    /// Whether chunking would be used
    pub enabled: bool,
    /// Number of chunks that would be created
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_count: Option<usize>,
    /// Breakdown by category (for review) or by file
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub chunks: Vec<ChunkInfo>,
}

/// Information about a single chunk
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ChunkInfo {
    /// Chunk identifier (e.g., "security", "style", or file name)
    pub id: String,
    /// Estimated tokens for this chunk
    pub tokens_estimated: usize,
    /// Files included in this chunk
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
}

/// Result from dry-run mode
#[derive(Debug, Serialize)]
pub(crate) struct DryRunResult {
    /// Command that would be executed
    pub command: String,
    /// Estimated total input tokens
    pub tokens_estimated: usize,
    /// Token limit warning threshold
    pub warn_threshold: usize,
    /// Token limit error threshold
    pub error_threshold: usize,
    /// Whether the input exceeds the warning threshold
    pub exceeds_warning: bool,
    /// Whether the input exceeds the error threshold
    pub exceeds_error: bool,
    /// Chunking plan details
    pub chunking: ChunkPlan,
    /// Files that would be processed
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    /// Provider that would be used
    pub provider: String,
    /// Model that would be used
    pub model: String,
    /// Timeout in seconds
    pub timeout_secs: u64,
}

impl DryRunResult {
    /// Create a new dry-run result
    #[must_use]
    pub(crate) fn new(
        command: impl Into<String>,
        tokens_estimated: usize,
        provider: impl Into<String>,
        model: impl Into<String>,
        timeout_secs: u64,
    ) -> Self {
        // Use default thresholds from tokens.rs
        let warn_threshold = 8000;
        let error_threshold = 16000;

        Self {
            command: command.into(),
            tokens_estimated,
            warn_threshold,
            error_threshold,
            exceeds_warning: tokens_estimated > warn_threshold,
            exceeds_error: tokens_estimated > error_threshold,
            chunking: ChunkPlan {
                enabled: false,
                chunk_count: None,
                chunks: vec![],
            },
            files: vec![],
            provider: provider.into(),
            model: model.into(),
            timeout_secs,
        }
    }

    /// Set the files that would be processed
    #[must_use]
    pub(crate) fn with_files(mut self, files: Vec<String>) -> Self {
        self.files = files;
        self
    }

    /// Set the chunking plan
    #[must_use]
    pub(crate) fn with_chunking(mut self, plan: ChunkPlan) -> Self {
        self.chunking = plan;
        self
    }
}

impl CommandOutput for DryRunResult {
    fn render_human(&self) -> String {
        use std::fmt::Write;
        let mut output = String::new();

        writeln!(output, "Dry run: {}", self.command).ok();
        writeln!(output).ok();
        writeln!(output, "Configuration:").ok();
        writeln!(output, "  Provider: {}", self.provider).ok();
        writeln!(output, "  Model: {}", self.model).ok();
        writeln!(output, "  Timeout: {}s", self.timeout_secs).ok();
        writeln!(output).ok();

        writeln!(output, "Token estimation:").ok();
        writeln!(output, "  Estimated tokens: {}", self.tokens_estimated).ok();
        writeln!(
            output,
            "  Warning threshold: {} {}",
            self.warn_threshold,
            if self.exceeds_warning {
                "(EXCEEDED)"
            } else {
                ""
            }
        )
        .ok();
        writeln!(
            output,
            "  Error threshold: {} {}",
            self.error_threshold,
            if self.exceeds_error { "(EXCEEDED)" } else { "" }
        )
        .ok();

        if !self.files.is_empty() {
            writeln!(output).ok();
            writeln!(output, "Files ({}):", self.files.len()).ok();
            for file in &self.files {
                writeln!(output, "  - {file}").ok();
            }
        }

        if self.chunking.enabled {
            writeln!(output).ok();
            writeln!(
                output,
                "Chunking: {} chunks",
                self.chunking.chunk_count.unwrap_or(0)
            )
            .ok();
            for chunk in &self.chunking.chunks {
                writeln!(
                    output,
                    "  - {}: ~{} tokens",
                    chunk.id, chunk.tokens_estimated
                )
                .ok();
                for file in &chunk.files {
                    writeln!(output, "      {file}").ok();
                }
            }
        } else {
            writeln!(output).ok();
            writeln!(output, "Chunking: disabled (single LLM call)").ok();
        }

        if self.exceeds_error {
            writeln!(output).ok();
            writeln!(
                output,
                "WARNING: Input exceeds error threshold. Use --chunk or reduce input size."
            )
            .ok();
        } else if self.exceeds_warning {
            writeln!(output).ok();
            writeln!(
                output,
                "Input exceeds warning threshold. Consider using --chunk for better results."
            )
            .ok();
        }

        output
    }
}

impl ExitStatus for DryRunResult {
    fn exit_code(&self) -> ExitCode {
        // Dry run is always successful (it's informational)
        ExitCode::Success
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code uses unwrap for clarity")]
mod tests {
    use super::*;

    #[test]
    fn test_dry_run_result_exit_code() {
        let result = DryRunResult::new("explain", 1000, "ollama", "llama3.2", 60);
        assert_eq!(result.exit_code(), ExitCode::Success);
    }

    #[test]
    fn test_dry_run_result_exceeds_thresholds() {
        let result_warn = DryRunResult::new("explain", 10000, "ollama", "llama3.2", 60);
        assert!(result_warn.exceeds_warning);
        assert!(!result_warn.exceeds_error);

        let result_error = DryRunResult::new("explain", 20000, "ollama", "llama3.2", 60);
        assert!(result_error.exceeds_warning);
        assert!(result_error.exceeds_error);
    }

    #[test]
    fn test_dry_run_result_human_output() {
        let result = DryRunResult::new("explain", 5000, "ollama", "llama3.2", 60)
            .with_files(vec!["src/main.rs".to_string()]);

        let output = result.render_human();
        assert!(output.contains("Dry run: explain"));
        assert!(output.contains("Provider: ollama"));
        assert!(output.contains("Model: llama3.2"));
        assert!(output.contains("Estimated tokens: 5000"));
        assert!(output.contains("src/main.rs"));
    }

    #[test]
    fn test_dry_run_result_with_chunking() {
        let plan = ChunkPlan {
            enabled: true,
            chunk_count: Some(3),
            chunks: vec![
                ChunkInfo {
                    id: "security".to_string(),
                    tokens_estimated: 2000,
                    files: vec!["src/auth.rs".to_string()],
                },
                ChunkInfo {
                    id: "style".to_string(),
                    tokens_estimated: 1500,
                    files: vec!["src/main.rs".to_string()],
                },
            ],
        };

        let result = DryRunResult::new("review", 3500, "openai", "gpt-4", 120).with_chunking(plan);

        let output = result.render_human();
        assert!(output.contains("Chunking: 3 chunks"));
        assert!(output.contains("security: ~2000 tokens"));
    }

    #[test]
    fn test_dry_run_result_json() {
        let result = DryRunResult::new("explain", 5000, "ollama", "llama3.2", 60);
        let json = result.render_json().unwrap();
        // JSON may have spaces after colons in pretty print
        assert!(
            json.contains("\"tokens_estimated\": 5000")
                || json.contains("\"tokens_estimated\":5000"),
            "Expected tokens_estimated:5000 in JSON: {json}"
        );
        assert!(
            json.contains("\"command\": \"explain\"") || json.contains("\"command\":\"explain\""),
            "Expected command:explain in JSON: {json}"
        );
    }
}
