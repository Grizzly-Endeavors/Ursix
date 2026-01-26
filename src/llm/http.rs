//! Shared HTTP utilities for LLM clients.

use std::time::Duration;

use reqwest::Client;

use super::LlmError;

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
#[allow(clippy::unwrap_used)]
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
}
