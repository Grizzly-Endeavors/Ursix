//! Stateless single-pass execution pipeline
//!
//! The pipeline provides a simple, stateless interface for making single LLM calls
//! without tool execution or message history management. This is useful for tasks
//! that don't require iterative tool use, such as code explanation or summarization.

use thiserror::Error;

use crate::context::InputContext;
use crate::llm::{ChatOptions, LlmClient, LlmError, Message, RetryConfig, Role, with_retry};
use crate::output::{ExitCode, ToExitCode};

/// Error type for pipeline execution
#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("LLM error: {0}")]
    Llm(#[from] LlmError),
    #[error("token limit exceeded: {0}")]
    TokenLimit(String),
}

impl ToExitCode for PipelineError {
    fn to_exit_code(&self) -> ExitCode {
        match self {
            Self::Llm(e) => e.to_exit_code(),
            Self::TokenLimit(_) => ExitCode::UserError,
        }
    }
}

/// Stateless single-pass execution pipeline
///
/// The pipeline makes a single LLM call without tools or message history mutation.
/// Use this for simple tasks that don't require iterative tool execution.
pub struct Pipeline<L: LlmClient> {
    client: L,
}

impl<L: LlmClient + Clone + 'static> Pipeline<L> {
    /// Create a new pipeline with the given LLM client
    #[must_use]
    pub fn new(client: L) -> Self {
        Self { client }
    }

    /// Execute a single-pass LLM call
    ///
    /// Builds messages from the system prompt, context, and user request,
    /// then makes a single LLM call with no tools. Automatically retries
    /// on transient failures according to the retry config.
    ///
    /// # Arguments
    /// * `system_prompt` - The system prompt defining the LLM's behavior
    /// * `context` - Input context to include in the prompt
    /// * `user_request` - The user's request
    /// * `json_mode` - If true, enforce JSON output at the API level
    /// * `retry_config` - Configuration for retry behavior on transient failures
    ///
    /// # Errors
    /// Returns error if the LLM call fails after all retry attempts
    pub async fn execute(
        &self,
        system_prompt: &str,
        context: &InputContext,
        user_request: &str,
        json_mode: bool,
        retry_config: &RetryConfig,
    ) -> Result<String, PipelineError> {
        let user_content = if context.is_empty() {
            user_request.to_string()
        } else {
            format!("{}\n\n{}", context.content, user_request)
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
            content_len = context.content.len(),
            json_mode,
            max_retries = retry_config.max_retries,
            "executing pipeline"
        );

        let client = self.client.clone();
        let response = with_retry(retry_config, || {
            let msgs = messages.clone();
            let opts = options.clone();
            let c = client.clone();
            async move { c.chat(&msgs, &[], &opts).await }
        })
        .await?;

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
    use crate::llm::{LlmResponse, ToolDefinition};
    use async_trait::async_trait;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Clone)]
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
        let retry_config = RetryConfig::no_retry();

        let context = InputContext::default();
        let result = pipeline
            .execute(
                "You are helpful.",
                &context,
                "Say hello",
                false,
                &retry_config,
            )
            .await
            .unwrap();

        assert_eq!(result, "Test response");
    }

    #[tokio::test]
    async fn test_execute_with_content() {
        let client = MockClient::new("Analyzed!");
        let pipeline = Pipeline::new(client);
        let retry_config = RetryConfig::no_retry();

        let context = InputContext::new("fn main() {}");

        let result = pipeline
            .execute(
                "You are a code analyzer.",
                &context,
                "Explain this code",
                false,
                &retry_config,
            )
            .await
            .unwrap();

        assert_eq!(result, "Analyzed!");
    }

    #[tokio::test]
    async fn test_single_llm_call() {
        let client = MockClient::new("Response");
        let pipeline = Pipeline::new(client);
        let retry_config = RetryConfig::no_retry();

        let context = InputContext::default();
        pipeline
            .execute("System", &context, "User", false, &retry_config)
            .await
            .unwrap();

        // Verify only one LLM call was made (stateless, single-pass)
        assert_eq!(pipeline.client.call_count(), 1);
    }

    #[tokio::test]
    async fn test_execute_with_json_mode() {
        let client = MockClient::new("{\"status\": \"ok\"}");
        let pipeline = Pipeline::new(client);
        let retry_config = RetryConfig::no_retry();

        let context = InputContext::default();
        let result = pipeline
            .execute(
                "Return JSON",
                &context,
                "Give me status",
                true,
                &retry_config,
            )
            .await
            .unwrap();

        assert_eq!(result, "{\"status\": \"ok\"}");
    }

    #[test]
    fn test_pipeline_error_to_exit_code_llm_parse() {
        use crate::llm::LlmError;
        let err = PipelineError::Llm(LlmError::Parse("bad json".to_string()));
        assert_eq!(err.to_exit_code(), ExitCode::PermanentError);
    }

    #[test]
    fn test_pipeline_error_to_exit_code_llm_api() {
        use crate::llm::LlmError;
        let err = PipelineError::Llm(LlmError::Api("auth failed".to_string()));
        assert_eq!(err.to_exit_code(), ExitCode::PermanentError);
    }

    #[test]
    fn test_pipeline_error_to_exit_code_token_limit() {
        let err = PipelineError::TokenLimit("input too large".to_string());
        assert_eq!(err.to_exit_code(), ExitCode::UserError);
    }
}
