//! Shared HTTP utilities for LLM clients.

use std::sync::Arc;
use std::time::Duration;

use reqwest::Client;

use super::LlmError;

/// Configuration for HTTP client connection pooling
#[derive(Debug, Clone)]
pub(crate) struct HttpClientConfig {
    /// Request timeout in seconds
    pub timeout_secs: u64,
    /// Maximum idle connections per host (default: 10)
    pub pool_max_idle_per_host: usize,
    /// HTTP/2 keep-alive interval in seconds (default: 30)
    pub http2_keep_alive_secs: u64,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 60,
            pool_max_idle_per_host: 10,
            http2_keep_alive_secs: 30,
        }
    }
}

impl HttpClientConfig {
    /// Create a config with the specified timeout and default pool settings
    #[must_use]
    pub(crate) fn with_timeout(timeout_secs: u64) -> Self {
        Self {
            timeout_secs,
            ..HttpClientConfig::default()
        }
    }
}

/// Shared HTTP client wrapper for connection reuse across LLM clients
///
/// This wrapper uses `Arc` internally, making `Clone` cheap and allowing
/// multiple LLM clients to share the same underlying connection pool.
#[derive(Clone)]
pub(crate) struct SharedHttpClient {
    client: Arc<Client>,
    timeout_secs: u64,
}

impl SharedHttpClient {
    /// Create a new shared HTTP client with the specified configuration
    #[must_use]
    pub(crate) fn new(config: &HttpClientConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .pool_max_idle_per_host(config.pool_max_idle_per_host)
            .http2_keep_alive_interval(Duration::from_secs(config.http2_keep_alive_secs))
            .build()
            .unwrap_or_else(|e| {
                tracing::error!(error = %e, "failed to build HTTP client with config, using default");
                Client::new()
            });

        Self {
            client: Arc::new(client),
            timeout_secs: config.timeout_secs,
        }
    }

    /// Get a reference to the underlying HTTP client
    #[must_use]
    pub(crate) fn client(&self) -> &Client {
        &self.client
    }

    /// Get the configured timeout in seconds
    #[must_use]
    pub(crate) fn timeout_secs(&self) -> u64 {
        self.timeout_secs
    }
}

/// Check if a URL is using insecure HTTP for a remote (non-localhost) server.
fn is_insecure_remote_url(url: &str) -> bool {
    let url_lower = url.to_lowercase();
    url_lower.starts_with("http://")
        && !url_lower.contains("localhost")
        && !url_lower.contains("127.0.0.1")
        && !url_lower.contains("[::1]")
}

/// Warn if a URL uses insecure HTTP for a remote server.
pub(crate) fn warn_if_insecure_remote(url: &str) {
    if is_insecure_remote_url(url) {
        tracing::warn!(
            url = %url,
            "using unencrypted HTTP for non-localhost API; consider using HTTPS"
        );
    }
}

/// Build an HTTP client with the specified timeout.
///
/// Falls back to a default client if building with timeout fails.
pub(crate) fn build_http_client(timeout_secs: u64) -> Client {
    Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .unwrap_or_else(|e| {
            tracing::error!(error = %e, "failed to build HTTP client with timeout, using default");
            Client::new()
        })
}

/// Map a reqwest error to an [`LlmError`], detecting timeouts.
pub(crate) fn map_request_error(e: reqwest::Error, timeout_secs: u64) -> LlmError {
    if e.is_timeout() {
        LlmError::Timeout(timeout_secs)
    } else {
        LlmError::Request(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insecure_url_detection() {
        // Remote HTTP URLs are insecure
        assert!(is_insecure_remote_url("http://api.example.com:11434"));
        assert!(is_insecure_remote_url("http://192.168.1.1:11434"));
        assert!(is_insecure_remote_url("http://10.0.0.1:8080"));

        // Localhost HTTP is fine
        assert!(!is_insecure_remote_url("http://localhost:11434"));
        assert!(!is_insecure_remote_url("http://127.0.0.1:11434"));
        assert!(!is_insecure_remote_url("http://[::1]:11434"));

        // HTTPS is always fine
        assert!(!is_insecure_remote_url("https://api.example.com"));
        assert!(!is_insecure_remote_url("https://192.168.1.1:8080"));
    }

    #[test]
    fn test_build_http_client() {
        let client = build_http_client(30);
        // Client should be created successfully
        assert!(client.get("http://localhost").build().is_ok());
    }

    #[test]
    fn test_http_client_config_default() {
        let config = HttpClientConfig::default();
        assert_eq!(config.timeout_secs, 60);
        assert_eq!(config.pool_max_idle_per_host, 10);
        assert_eq!(config.http2_keep_alive_secs, 30);
    }

    #[test]
    fn test_http_client_config_with_timeout() {
        let config = HttpClientConfig::with_timeout(120);
        assert_eq!(config.timeout_secs, 120);
        // Other values should be defaults
        assert_eq!(config.pool_max_idle_per_host, 10);
        assert_eq!(config.http2_keep_alive_secs, 30);
    }

    #[test]
    fn test_shared_http_client_clone_is_cheap() {
        let config = HttpClientConfig::default();
        let client1 = SharedHttpClient::new(&config);
        let client2 = client1.clone();

        // Both should point to the same underlying Arc
        assert!(Arc::ptr_eq(&client1.client, &client2.client));
    }

    #[test]
    fn test_shared_http_client_builds_successfully() {
        let config = HttpClientConfig::with_timeout(45);
        let shared = SharedHttpClient::new(&config);

        // Verify timeout is stored
        assert_eq!(shared.timeout_secs(), 45);

        // Client should be usable
        assert!(shared.client().get("http://localhost").build().is_ok());
    }
}
