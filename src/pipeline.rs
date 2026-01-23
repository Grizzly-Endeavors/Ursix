//! Stateless single-pass execution pipeline
//!
//! The pipeline provides a simple, stateless interface for making single LLM calls
//! without tool execution or message history management. This is useful for tasks
//! that don't require iterative tool use, such as code explanation or summarization.

use thiserror::Error;

use crate::context::GatheredContext;
use crate::llm::{ChatOptions, LlmClient, LlmError, Message, Role};

/// Error type for pipeline execution
#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("LLM error: {0}")]
    Llm(#[from] LlmError),
}

/// Format gathered context into a string suitable for inclusion in a prompt
fn format_context(context: &GatheredContext) -> String {
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

/// Stateless single-pass execution pipeline
///
/// The pipeline makes a single LLM call without tools or message history mutation.
/// Use this for simple tasks that don't require iterative tool execution.
pub struct Pipeline<L: LlmClient> {
    client: L,
}

impl<L: LlmClient> Pipeline<L> {
    /// Create a new pipeline with the given LLM client
    #[must_use]
    pub fn new(client: L) -> Self {
        Self { client }
    }

    /// Execute a single-pass LLM call
    ///
    /// Builds messages from the system prompt, context, and user request,
    /// then makes a single LLM call with no tools.
    ///
    /// # Arguments
    /// * `system_prompt` - The system prompt defining the LLM's behavior
    /// * `context` - Gathered context to include in the prompt
    /// * `user_request` - The user's request
    /// * `json_mode` - If true, enforce JSON output at the API level
    ///
    /// # Errors
    /// Returns error if the LLM call fails
    pub async fn execute(
        &self,
        system_prompt: &str,
        context: &GatheredContext,
        user_request: &str,
        json_mode: bool,
    ) -> Result<String, PipelineError> {
        let formatted_context = format_context(context);

        let user_content = if formatted_context.is_empty() {
            user_request.to_string()
        } else {
            format!("{formatted_context}{user_request}")
        };

        let messages = vec![
            Message {
                role: Role::System,
                content: system_prompt.to_string(),
                tool_calls: None,
                tool_call_id: None,
            },
            Message {
                role: Role::User,
                content: user_content,
                tool_calls: None,
                tool_call_id: None,
            },
        ];

        let options = if json_mode {
            ChatOptions::json()
        } else {
            ChatOptions::default()
        };

        tracing::info!(
            model = %self.client.model_name(),
            files = context.files.len(),
            has_diff = context.git_diff.is_some(),
            json_mode,
            "executing pipeline"
        );

        let response = self.client.chat(&messages, &[], &options).await?;

        tracing::debug!(
            content_len = response.content.len(),
            "pipeline execution complete"
        );

        Ok(response.content)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::context::FileContext;
    use crate::llm::{LlmResponse, ToolDefinition};
    use async_trait::async_trait;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockClient {
        response: String,
        call_count: Arc<AtomicUsize>,
    }

    impl MockClient {
        fn new(response: impl Into<String>) -> Self {
            Self {
                response: response.into(),
                call_count: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn call_count(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl LlmClient for MockClient {
        async fn chat(
            &self,
            _messages: &[Message],
            _tools: &[ToolDefinition],
            _options: &ChatOptions,
        ) -> Result<LlmResponse, LlmError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(LlmResponse::new(self.response.clone(), vec![]))
        }

        fn model_name(&self) -> &'static str {
            "mock"
        }
    }

    #[tokio::test]
    async fn test_execute_simple() {
        let client = MockClient::new("Test response");
        let pipeline = Pipeline::new(client);

        let context = GatheredContext::default();
        let result = pipeline
            .execute("You are helpful.", &context, "Say hello", false)
            .await
            .unwrap();

        assert_eq!(result, "Test response");
    }

    #[tokio::test]
    async fn test_execute_with_files() {
        let client = MockClient::new("Analyzed!");
        let pipeline = Pipeline::new(client);

        let context = GatheredContext {
            files: vec![
                FileContext {
                    path: PathBuf::from("src/main.rs"),
                    content: "fn main() {}".to_string(),
                },
                FileContext {
                    path: PathBuf::from("src/lib.rs"),
                    content: "pub mod foo;".to_string(),
                },
            ],
            git_diff: None,
            git_status: None,
            additional_context: None,
        };

        let result = pipeline
            .execute(
                "You are a code analyzer.",
                &context,
                "Explain this code",
                false,
            )
            .await
            .unwrap();

        assert_eq!(result, "Analyzed!");
    }

    #[tokio::test]
    async fn test_execute_with_git_diff() {
        let client = MockClient::new("Reviewed!");
        let pipeline = Pipeline::new(client);

        let context = GatheredContext {
            files: Vec::new(),
            git_diff: Some("+fn new_function() {}".to_string()),
            git_status: Some("M src/lib.rs".to_string()),
            additional_context: None,
        };

        let result = pipeline
            .execute(
                "You are a code reviewer.",
                &context,
                "Review this change",
                false,
            )
            .await
            .unwrap();

        assert_eq!(result, "Reviewed!");
    }

    #[tokio::test]
    async fn test_single_llm_call() {
        let client = MockClient::new("Response");
        let pipeline = Pipeline::new(client);

        let context = GatheredContext::default();
        pipeline
            .execute("System", &context, "User", false)
            .await
            .unwrap();

        // Verify only one LLM call was made (stateless, single-pass)
        assert_eq!(pipeline.client.call_count(), 1);
    }

    #[tokio::test]
    async fn test_execute_with_json_mode() {
        let client = MockClient::new("{\"status\": \"ok\"}");
        let pipeline = Pipeline::new(client);

        let context = GatheredContext::default();
        let result = pipeline
            .execute("Return JSON", &context, "Give me status", true)
            .await
            .unwrap();

        assert_eq!(result, "{\"status\": \"ok\"}");
    }

    #[test]
    fn test_format_context_empty() {
        let context = GatheredContext::default();
        let formatted = format_context(&context);
        assert!(formatted.is_empty());
    }

    #[test]
    fn test_format_context_with_files() {
        let context = GatheredContext {
            files: vec![FileContext {
                path: PathBuf::from("test.rs"),
                content: "fn test() {}".to_string(),
            }],
            git_diff: None,
            git_status: None,
            additional_context: None,
        };

        let formatted = format_context(&context);

        assert!(formatted.contains("## Files"));
        assert!(formatted.contains("test.rs"));
        assert!(formatted.contains("fn test() {}"));
    }

    #[test]
    fn test_format_context_with_git_diff() {
        let context = GatheredContext {
            files: Vec::new(),
            git_diff: Some("+added line".to_string()),
            git_status: None,
            additional_context: None,
        };

        let formatted = format_context(&context);

        assert!(formatted.contains("## Git Diff"));
        assert!(formatted.contains("+added line"));
    }

    #[test]
    fn test_format_context_with_all_fields() {
        let context = GatheredContext {
            files: vec![FileContext {
                path: PathBuf::from("code.rs"),
                content: "fn main() {}".to_string(),
            }],
            git_diff: Some("+new line".to_string()),
            git_status: Some("M code.rs".to_string()),
            additional_context: Some("Lint errors here".to_string()),
        };

        let formatted = format_context(&context);

        assert!(formatted.contains("## Files"));
        assert!(formatted.contains("## Git Diff"));
        assert!(formatted.contains("## Git Status"));
        assert!(formatted.contains("## Additional Context"));
        assert!(formatted.contains("Lint errors here"));
    }

    #[test]
    fn test_format_context_skips_empty_diff() {
        let context = GatheredContext {
            files: Vec::new(),
            git_diff: Some(String::new()),
            git_status: None,
            additional_context: None,
        };

        let formatted = format_context(&context);
        assert!(!formatted.contains("## Git Diff"));
    }
}
