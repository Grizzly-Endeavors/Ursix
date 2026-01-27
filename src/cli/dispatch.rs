//! LLM client creation and pipeline dispatch helpers
//!
//! This module provides the infrastructure for creating LLM clients based on
//! configuration and running the pipeline with the appropriate provider.

use anyhow::Result;

use super::CliError;
use crate::config::{Config, Provider};
use crate::context::InputContext;
use crate::llm::ollama::OllamaClient;
use crate::llm::openai::OpenAiClient;
use crate::llm::{HttpClientConfig, LlmClient, RetryConfig, SharedHttpClient};
use crate::pipeline::Pipeline;

/// Check if a URL points to localhost
fn is_local_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.contains("localhost") || lower.contains("127.0.0.1") || lower.contains("[::1]")
}

/// Create an [`OpenAiClient`] from config with a shared HTTP client, handling API key requirements
fn create_openai_client_with_http(
    config: &Config,
    http: &SharedHttpClient,
    url: &str,
) -> Result<OpenAiClient> {
    if let Some(ref key) = config.api_key {
        Ok(OpenAiClient::with_http_client_and_api_key(
            http.clone(),
            url,
            &config.model,
            key,
        ))
    } else if is_local_url(url) {
        Ok(OpenAiClient::with_http_client(
            http.clone(),
            url,
            &config.model,
        ))
    } else {
        Err(CliError::Config(anyhow::anyhow!(
            "API key required for remote OpenAI endpoints. \
             Set URSIX_API_KEY environment variable or pass it via --provider NAME URL API-KEY."
        ))
        .into())
    }
}

/// Create an [`OpenAiClient`] from config, handling API key requirements
///
/// This creates a new HTTP client internally. For connection reuse, use
/// [`create_openai_client_with_http`] instead.
pub(crate) fn create_openai_client(config: &Config, url: &str) -> Result<OpenAiClient> {
    if let Some(ref key) = config.api_key {
        Ok(OpenAiClient::with_api_key(
            url,
            &config.model,
            key,
            config.timeout_secs,
        ))
    } else if is_local_url(url) {
        Ok(OpenAiClient::new(url, &config.model, config.timeout_secs))
    } else {
        Err(CliError::Config(anyhow::anyhow!(
            "API key required for remote OpenAI endpoints. \
             Set URSIX_API_KEY environment variable or pass it via --provider NAME URL API-KEY."
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
    let http = SharedHttpClient::new(&HttpClientConfig::with_timeout(config.timeout_secs));
    let provider_url = config.effective_provider_url();

    match config.provider {
        Provider::Ollama => {
            let client = OllamaClient::with_http_client(http, provider_url, &config.model);
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
            let client = create_openai_client_with_http(config, &http, provider_url)?;
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
