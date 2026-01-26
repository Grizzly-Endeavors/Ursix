//! LLM client creation and pipeline dispatch helpers
//!
//! This module provides the infrastructure for creating LLM clients based on
//! configuration and running the pipeline with the appropriate provider.

use anyhow::Result;

use super::CliError;
use crate::config::{Config, Provider};
use crate::context::InputContext;
use crate::llm::LlmClient;
use crate::llm::RetryConfig;
use crate::llm::ollama::OllamaClient;
use crate::llm::openai::OpenAiClient;
use crate::pipeline::Pipeline;

/// Check if a URL points to localhost
fn is_local_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.contains("localhost") || lower.contains("127.0.0.1") || lower.contains("[::1]")
}

/// Create an [`OpenAiClient`] from config, handling API key requirements
pub(crate) fn create_openai_client(config: &Config) -> Result<OpenAiClient> {
    if let Some(ref key) = config.openai_api_key {
        Ok(OpenAiClient::with_api_key(
            &config.openai_url,
            &config.model,
            key,
            config.timeout_secs,
        ))
    } else if is_local_url(&config.openai_url) {
        Ok(OpenAiClient::new(
            &config.openai_url,
            &config.model,
            config.timeout_secs,
        ))
    } else {
        Err(CliError::Config(anyhow::anyhow!(
            "OpenAI API key required for remote endpoints. \
             Set OPENAI_API_KEY environment variable or use --openai-api-key flag."
        ))
        .into())
    }
}

/// Run the pipeline with the appropriate provider
pub(crate) async fn run_pipeline(
    config: &Config,
    system_prompt: &str,
    context: &InputContext,
    user_request: &str,
    json_mode: bool,
) -> Result<String> {
    match config.provider {
        Provider::Ollama => {
            let client = OllamaClient::new(&config.ollama_url, &config.model, config.timeout_secs);
            run_pipeline_with_client(
                client,
                system_prompt,
                context,
                user_request,
                json_mode,
                &config.retry_config,
            )
            .await
        }
        Provider::OpenAi => {
            let client = create_openai_client(config)?;
            run_pipeline_with_client(
                client,
                system_prompt,
                context,
                user_request,
                json_mode,
                &config.retry_config,
            )
            .await
        }
    }
}

/// Run the pipeline with a specific LLM client
async fn run_pipeline_with_client<C: LlmClient + Clone + 'static>(
    client: C,
    system_prompt: &str,
    context: &InputContext,
    user_request: &str,
    json_mode: bool,
    retry_config: &RetryConfig,
) -> Result<String> {
    let pipeline = Pipeline::new(client);
    pipeline
        .execute(
            system_prompt,
            context,
            user_request,
            json_mode,
            retry_config,
        )
        .await
        .map_err(Into::into)
}
