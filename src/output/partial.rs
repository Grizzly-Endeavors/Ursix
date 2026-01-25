//! Partial result types for chunk failures with --partial flag
//!
//! When `--partial` is enabled and some chunks fail, these types allow
//! returning successfully processed results alongside failure information.

use serde::Serialize;

use super::{ExitCode, ExitStatus, Fix, OutputMeta, ReviewIssue};

/// Partial review result when some chunks failed
#[derive(Debug, Clone, Serialize)]
pub struct PartialReviewResult {
    /// Issues found in successfully processed chunks
    pub issues: Vec<ReviewIssue>,
    /// Files/chunks that were successfully processed
    pub chunks_processed: Vec<String>,
    /// Files/chunks that failed to process
    pub chunks_failed: Vec<ChunkFailureInfo>,
    /// Summary of the partial review
    pub summary: String,
}

/// Partial fix result when some chunks failed
#[derive(Debug, Clone, Serialize)]
pub struct PartialFixResult {
    /// Fixes from successfully processed chunks
    pub fixes: Vec<Fix>,
    /// Files/chunks that were successfully processed
    pub chunks_processed: Vec<String>,
    /// Files/chunks that failed to process
    pub chunks_failed: Vec<ChunkFailureInfo>,
    /// Diagnosis summary
    pub diagnosis: String,
}

/// Information about a failed chunk
#[derive(Debug, Clone, Serialize)]
pub struct ChunkFailureInfo {
    /// Identifier for the failed chunk
    pub chunk_id: String,
    /// Error message describing the failure
    pub error: String,
}

impl ExitStatus for PartialReviewResult {
    fn exit_code(&self) -> ExitCode {
        // Partial results always indicate issues (some chunks failed)
        ExitCode::IssuesFound
    }
}

impl ExitStatus for PartialFixResult {
    fn exit_code(&self) -> ExitCode {
        ExitCode::IssuesFound
    }
}

/// Response wrapper for partial failure scenarios
///
/// Used when `--partial` is enabled and some (but not all) chunks fail.
/// Includes both successful results and error information.
#[derive(Debug, Clone, Serialize)]
pub struct PartialFailureResponse<T: Serialize> {
    /// Metadata about the operation
    #[serde(rename = "_meta")]
    pub meta: OutputMeta,
    /// Partial results from successful chunks
    pub partial_results: T,
    /// Whether this is a partial result (always true for this type)
    pub partial: bool,
}

impl<T: Serialize> PartialFailureResponse<T> {
    /// Create a new partial failure response
    #[must_use]
    pub fn new(partial_results: T, meta: OutputMeta) -> Self {
        Self {
            meta,
            partial_results,
            partial: true,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_partial_review_result_exit_code() {
        let result = PartialReviewResult {
            issues: vec![],
            chunks_processed: vec!["file1.rs".to_string()],
            chunks_failed: vec![ChunkFailureInfo {
                chunk_id: "file2.rs".to_string(),
                error: "network error".to_string(),
            }],
            summary: "Partial review".to_string(),
        };
        assert_eq!(result.exit_code(), ExitCode::IssuesFound);
    }

    #[test]
    fn test_partial_fix_result_exit_code() {
        let result = PartialFixResult {
            fixes: vec![],
            chunks_processed: vec!["file1.rs".to_string()],
            chunks_failed: vec![],
            diagnosis: "Partial fix".to_string(),
        };
        assert_eq!(result.exit_code(), ExitCode::IssuesFound);
    }

    #[test]
    fn test_chunk_failure_info_serialization() {
        let info = ChunkFailureInfo {
            chunk_id: "security-main.rs".to_string(),
            error: "rate limit exceeded".to_string(),
        };

        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["chunk_id"], "security-main.rs");
        assert_eq!(json["error"], "rate limit exceeded");
    }

    #[test]
    fn test_partial_failure_response_serialization() {
        let partial = PartialReviewResult {
            issues: vec![ReviewIssue {
                severity: "warning".to_string(),
                file: Some("test.rs".to_string()),
                line: Some(10),
                message: "unused variable".to_string(),
                rule: None,
            }],
            chunks_processed: vec!["test.rs".to_string()],
            chunks_failed: vec![ChunkFailureInfo {
                chunk_id: "other.rs".to_string(),
                error: "timeout".to_string(),
            }],
            summary: "1 file reviewed, 1 failed".to_string(),
        };

        let meta = OutputMeta::new("model", "provider", 1000, 2);
        let response = PartialFailureResponse::new(partial, meta);

        let json = serde_json::to_value(&response).unwrap();

        // Check structure
        assert!(json.get("_meta").is_some());
        assert!(json.get("partial_results").is_some());
        assert_eq!(json["partial"], true);

        // Check partial_results content
        assert_eq!(
            json["partial_results"]["issues"].as_array().unwrap().len(),
            1
        );
        assert_eq!(
            json["partial_results"]["chunks_failed"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn test_partial_review_result_full_serialization() {
        let result = PartialReviewResult {
            issues: vec![
                ReviewIssue {
                    severity: "error".to_string(),
                    file: Some("src/main.rs".to_string()),
                    line: Some(42),
                    message: "potential null pointer".to_string(),
                    rule: Some("security/null-check".to_string()),
                },
                ReviewIssue {
                    severity: "warning".to_string(),
                    file: Some("src/lib.rs".to_string()),
                    line: None,
                    message: "missing docs".to_string(),
                    rule: None,
                },
            ],
            chunks_processed: vec!["src/main.rs".to_string(), "src/lib.rs".to_string()],
            chunks_failed: vec![ChunkFailureInfo {
                chunk_id: "src/config.rs".to_string(),
                error: "connection timeout".to_string(),
            }],
            summary: "Reviewed 2 files, 1 failed".to_string(),
        };

        let json = serde_json::to_value(&result).unwrap();

        assert_eq!(json["issues"].as_array().unwrap().len(), 2);
        assert_eq!(json["chunks_processed"].as_array().unwrap().len(), 2);
        assert_eq!(json["chunks_failed"].as_array().unwrap().len(), 1);
        assert_eq!(json["summary"], "Reviewed 2 files, 1 failed");
    }
}
