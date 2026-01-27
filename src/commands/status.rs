//! The `status` command implementation
//!
//! Validates configuration and tests provider connectivity.

use std::time::Instant;

use anyhow::Result;

use crate::config::{Config, DEFAULT_GEMINI_URL, DEFAULT_OPENAI_URL, Provider};
use crate::llm::gemini::GeminiClient;
use crate::llm::ollama::OllamaClient;
use crate::llm::openai::OpenAiClient;
use crate::llm::{HttpClientConfig, SharedHttpClient};
use crate::output::{
    CommandOutput, ExitCode, ExitStatus, OutputMode, StatusCheck, StatusResult, VerboseInfo,
};

/// Timeout for status connectivity checks (10 seconds)
const STATUS_TIMEOUT_SECS: u64 = 10;

/// Options for the status command
#[derive(Debug, Clone)]
pub struct StatusOptions {
    /// Show detailed status information
    pub verbose: bool,
}

/// Run the status command
///
/// # Errors
/// This function returns `Ok` with an appropriate exit code for all cases.
/// Actual errors (e.g., failed checks) are reflected in the exit code, not as `Err`.
pub async fn cmd_status(
    config: &Config,
    options: StatusOptions,
    output_mode: OutputMode,
) -> Result<ExitCode> {
    let provider_url = config.effective_provider_url().to_string();
    let api_key_set = config.api_key.is_some();

    let mut result = StatusResult::new(
        config.provider.to_string(),
        &provider_url,
        &config.model,
        api_key_set,
    );

    // Check 1: Config - already loaded successfully if we got here
    result.add_check(StatusCheck::pass(
        "config",
        "configuration loaded successfully",
    ));

    // Check 2: Provider type and URL validation
    result.add_check(StatusCheck::pass(
        "provider",
        format!("{} at {}", config.provider, provider_url),
    ));

    // Check 3: API key requirements
    let api_key_check = check_api_key(config, &provider_url);
    let api_key_ok = api_key_check.passed;
    result.add_check(api_key_check);

    // Check 4: Connectivity (only if API key check passed)
    if api_key_ok {
        let connectivity_result = check_connectivity(config, &provider_url, options.verbose).await;
        result.add_check(connectivity_result.check);

        // Add verbose info if available
        if let Some(verbose) = connectivity_result.verbose {
            result.set_verbose(verbose);
        }
    } else {
        result.add_check(StatusCheck::fail(
            "connectivity",
            "cannot test connectivity without API key",
        ));
    }

    println!("{}", result.render(output_mode));
    Ok(result.exit_code())
}

/// Check if API key is properly configured
fn check_api_key(config: &Config, provider_url: &str) -> StatusCheck {
    match config.provider {
        Provider::Ollama => {
            // Ollama doesn't require an API key
            StatusCheck::pass("api_key", "not required for Ollama")
        }
        Provider::OpenAi => {
            // Check if this is a remote OpenAI endpoint (not localhost)
            let is_remote = is_remote_url(provider_url);
            let is_official_openai = provider_url.starts_with(DEFAULT_OPENAI_URL);

            if config.api_key.is_some() {
                StatusCheck::pass("api_key", "API key is set")
            } else if is_official_openai || is_remote {
                StatusCheck::fail("api_key", "API key required for remote OpenAI endpoint")
                    .with_details("Set URSIX_API_KEY or OPENAI_API_KEY environment variable")
            } else {
                // Local OpenAI-compatible endpoint (like vLLM, LM Studio)
                StatusCheck::pass(
                    "api_key",
                    "not set (optional for local OpenAI-compatible endpoint)",
                )
            }
        }
        Provider::Gemini => {
            // Gemini always requires an API key (no local server option)
            let is_official_gemini = provider_url.starts_with(DEFAULT_GEMINI_URL);

            if config.api_key.is_some() {
                StatusCheck::pass("api_key", "API key is set")
            } else {
                let details = if is_official_gemini {
                    "Set URSIX_API_KEY or GOOGLE_API_KEY environment variable"
                } else {
                    "Set URSIX_API_KEY environment variable"
                };
                StatusCheck::fail("api_key", "API key required for Gemini API")
                    .with_details(details)
            }
        }
    }
}

/// Result of a connectivity check
struct ConnectivityResult {
    check: StatusCheck,
    verbose: Option<VerboseInfo>,
}

/// Check connectivity to the provider
async fn check_connectivity(
    config: &Config,
    provider_url: &str,
    verbose: bool,
) -> ConnectivityResult {
    let http = SharedHttpClient::new(&HttpClientConfig::with_timeout(STATUS_TIMEOUT_SECS));
    let start = Instant::now();

    let models_result = match config.provider {
        Provider::Ollama => {
            let client = OllamaClient::with_http_client(http, provider_url, &config.model);
            client.list_models().await
        }
        Provider::OpenAi => {
            let client = if let Some(ref key) = config.api_key {
                OpenAiClient::with_http_client_and_api_key(http, provider_url, &config.model, key)
            } else {
                OpenAiClient::with_http_client(http, provider_url, &config.model)
            };
            client.list_models().await
        }
        Provider::Gemini => {
            // Gemini always requires an API key; if we get here, api_key check passed
            let key = config.api_key.as_ref().map_or("", String::as_str);
            let client = GeminiClient::with_http_client(http, provider_url, &config.model, key);
            client.list_models().await
        }
    };

    // Response time in milliseconds - safe to truncate as u128 won't exceed u64 for realistic durations
    #[allow(clippy::cast_possible_truncation)]
    let elapsed_ms = start.elapsed().as_millis() as u64;

    match models_result {
        Ok(models) => {
            let model_available = models.iter().any(|m| m == &config.model);

            let message = if model_available {
                format!("connected, model '{}' available", config.model)
            } else {
                format!(
                    "connected, but model '{}' not found ({} models available)",
                    config.model,
                    models.len()
                )
            };

            let check = if model_available {
                StatusCheck::pass("connectivity", message)
            } else {
                // Model not found is a warning, not a failure
                // The provider is reachable, which is the main point
                StatusCheck::pass("connectivity", message)
            };

            let verbose_info = if verbose {
                Some(VerboseInfo {
                    response_time_ms: Some(elapsed_ms),
                    available_models: Some(models),
                    model_available: Some(model_available),
                })
            } else {
                None
            };

            ConnectivityResult {
                check,
                verbose: verbose_info,
            }
        }
        Err(e) => {
            let message = format_connectivity_error(&e);
            let check = StatusCheck::fail("connectivity", message);

            ConnectivityResult {
                check,
                verbose: None,
            }
        }
    }
}

/// Format a connectivity error for human display
fn format_connectivity_error(error: &crate::llm::LlmError) -> String {
    use crate::llm::LlmError;

    match error {
        LlmError::Timeout(secs) => format!("connection timed out after {secs} seconds"),
        LlmError::Request(e) => {
            if e.is_connect() {
                "connection refused (is the provider running?)".to_string()
            } else {
                format!("request failed: {e}")
            }
        }
        LlmError::Api(msg) => {
            if msg.contains("401") {
                "authentication failed (check API key)".to_string()
            } else if msg.contains("403") {
                "access forbidden (check API key permissions)".to_string()
            } else {
                format!("API error: {msg}")
            }
        }
        LlmError::Parse(msg) => format!("unexpected response: {msg}"),
    }
}

/// Check if a URL points to a remote (non-localhost) server
fn is_remote_url(url: &str) -> bool {
    let url_lower = url.to_lowercase();
    !url_lower.contains("localhost")
        && !url_lower.contains("127.0.0.1")
        && !url_lower.contains("[::1]")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_is_remote_url() {
        assert!(is_remote_url("https://api.openai.com/v1"));
        assert!(is_remote_url("http://192.168.1.1:8080"));
        assert!(!is_remote_url("http://localhost:11434"));
        assert!(!is_remote_url("http://127.0.0.1:8080"));
        assert!(!is_remote_url("http://[::1]:8080"));
    }

    #[test]
    fn test_check_api_key_ollama() {
        let config = Config {
            provider: Provider::Ollama,
            api_key: None,
            ..Config::default()
        };
        let check = check_api_key(&config, "http://localhost:11434");
        assert!(check.passed);
        assert!(check.message.contains("not required"));
    }

    #[test]
    fn test_check_api_key_openai_remote_without_key() {
        let config = Config {
            provider: Provider::OpenAi,
            api_key: None,
            ..Config::default()
        };
        let check = check_api_key(&config, "https://api.openai.com/v1");
        assert!(!check.passed);
        assert!(check.message.contains("required"));
    }

    #[test]
    fn test_check_api_key_openai_remote_with_key() {
        let config = Config {
            provider: Provider::OpenAi,
            api_key: Some("sk-test".to_string()),
            ..Config::default()
        };
        let check = check_api_key(&config, "https://api.openai.com/v1");
        assert!(check.passed);
        assert!(check.message.contains("set"));
    }

    #[test]
    fn test_check_api_key_openai_local_without_key() {
        let config = Config {
            provider: Provider::OpenAi,
            api_key: None,
            ..Config::default()
        };
        let check = check_api_key(&config, "http://localhost:8080/v1");
        assert!(check.passed);
        assert!(check.message.contains("optional"));
    }

    #[test]
    fn test_check_api_key_gemini_without_key() {
        let config = Config {
            provider: Provider::Gemini,
            api_key: None,
            ..Config::default()
        };
        let check = check_api_key(&config, "https://generativelanguage.googleapis.com/v1beta");
        assert!(!check.passed);
        assert!(check.message.contains("required"));
    }

    #[test]
    fn test_check_api_key_gemini_with_key() {
        let config = Config {
            provider: Provider::Gemini,
            api_key: Some("test-key".to_string()),
            ..Config::default()
        };
        let check = check_api_key(&config, "https://generativelanguage.googleapis.com/v1beta");
        assert!(check.passed);
        assert!(check.message.contains("set"));
    }

    #[test]
    fn test_format_connectivity_error_timeout() {
        let error = crate::llm::LlmError::Timeout(10);
        let msg = format_connectivity_error(&error);
        assert!(msg.contains("timed out"));
        assert!(msg.contains("10"));
    }

    #[test]
    fn test_format_connectivity_error_auth() {
        let error = crate::llm::LlmError::Api("401: Invalid API key".to_string());
        let msg = format_connectivity_error(&error);
        assert!(msg.contains("authentication failed"));
    }
}
