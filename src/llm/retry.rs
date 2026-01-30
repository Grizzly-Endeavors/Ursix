//! Retry logic with exponential backoff for LLM calls

use std::time::Duration;

use tokio::time::sleep;
use tracing::{info, warn};

use super::LlmError;

/// Configuration for retry behavior
#[derive(Debug, Clone)]
pub(crate) struct RetryConfig {
    /// Maximum number of retry attempts (0 = no retries)
    pub max_retries: u32,
    /// Initial delay before first retry
    pub initial_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Multiplier for exponential backoff
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
        }
    }
}

impl RetryConfig {
    /// Create a config that disables retries
    #[must_use]
    pub(crate) fn no_retry() -> Self {
        Self {
            max_retries: 0,
            ..Self::default()
        }
    }
}

/// Execute a fallible async operation with retry logic
///
/// Only retries if the error is marked as retryable via `LlmError::is_retryable()`.
///
/// # Errors
/// Returns the last error if all retries are exhausted or if a non-retryable error occurs
pub(crate) async fn with_retry<F, Fut, T>(config: &RetryConfig, operation: F) -> Result<T, LlmError>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, LlmError>>,
{
    let mut attempts = 0;
    let mut delay = config.initial_delay;

    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) if !e.is_retryable() => return Err(e),
            Err(e) if attempts >= config.max_retries => {
                warn!(
                    attempts = attempts + 1,
                    error = %e,
                    "max retries exceeded"
                );
                return Err(e);
            }
            Err(e) => {
                attempts += 1;
                info!(
                    attempt = attempts,
                    max_retries = config.max_retries,
                    delay_ms = delay.as_millis(),
                    error = %e,
                    "retrying after transient error"
                );

                // Add jitter (+-25%)
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "precision loss acceptable for jitter calculation"
                )]
                let jitter = delay.as_millis() as f64 * (rand::random::<f64>() * 0.5 - 0.25);
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "precision loss acceptable for duration calculation"
                )]
                let jittered_delay =
                    Duration::from_millis((delay.as_millis() as f64 + jitter) as u64);
                sleep(jittered_delay).await;

                // Exponential backoff
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "duration fits in u64 range"
                )]
                #[expect(
                    clippy::cast_sign_loss,
                    reason = "unsigned conversion acceptable for duration"
                )]
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "precision loss acceptable for backoff calculation"
                )]
                let next_delay = Duration::from_millis(
                    (delay.as_millis() as f64 * config.backoff_multiplier) as u64,
                );
                delay = next_delay.min(config.max_delay);
            }
        }
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code uses unwrap for clarity")]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Instant;

    use super::*;

    #[tokio::test]
    async fn test_with_retry_succeeds_immediately_on_first_attempt() {
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = Arc::clone(&call_count);

        let config = RetryConfig::default();
        let result = with_retry(&config, || {
            let count = Arc::clone(&call_count_clone);
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok::<_, LlmError>("success".to_string())
            }
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_with_retry_retries_on_retryable_error() {
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = Arc::clone(&call_count);

        let config = RetryConfig {
            max_retries: 3,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            backoff_multiplier: 2.0,
        };

        let result = with_retry(&config, || {
            let count = Arc::clone(&call_count_clone);
            async move {
                let current = count.fetch_add(1, Ordering::SeqCst);
                if current < 2 {
                    // Fail first two attempts with retryable error
                    Err(LlmError::Api("rate limit exceeded".to_string()))
                } else {
                    // Succeed on third attempt
                    Ok::<_, LlmError>("success".to_string())
                }
            }
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_with_retry_does_not_retry_on_non_retryable_error() {
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = Arc::clone(&call_count);

        let config = RetryConfig {
            max_retries: 3,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            backoff_multiplier: 2.0,
        };

        let result = with_retry(&config, || {
            let count = Arc::clone(&call_count_clone);
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                // Parse errors are not retryable
                Err::<String, _>(LlmError::Parse("invalid json".to_string()))
            }
        })
        .await;

        assert!(result.is_err());
        // Should only be called once (no retries for non-retryable errors)
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_with_retry_max_retries_is_respected() {
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = Arc::clone(&call_count);

        let config = RetryConfig {
            max_retries: 2,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            backoff_multiplier: 2.0,
        };

        let result = with_retry(&config, || {
            let count = Arc::clone(&call_count_clone);
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                // Always fail with retryable error
                Err::<String, _>(LlmError::Api("rate limit exceeded".to_string()))
            }
        })
        .await;

        assert!(result.is_err());
        // Initial attempt + max_retries = 1 + 2 = 3 total calls
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_with_retry_no_retry_config() {
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = Arc::clone(&call_count);

        let config = RetryConfig::no_retry();

        let result = with_retry(&config, || {
            let count = Arc::clone(&call_count_clone);
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Err::<String, _>(LlmError::Api("rate limit exceeded".to_string()))
            }
        })
        .await;

        assert!(result.is_err());
        // With no_retry config, should only be called once
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_with_retry_exponential_backoff_timing() {
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = Arc::clone(&call_count);

        // Use precise timing values for predictable testing
        let config = RetryConfig {
            max_retries: 2,
            initial_delay: Duration::from_millis(50),
            max_delay: Duration::from_secs(10),
            backoff_multiplier: 2.0,
        };

        let start = Instant::now();
        let result = with_retry(&config, || {
            let count = Arc::clone(&call_count_clone);
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Err::<String, _>(LlmError::Api("rate limit exceeded".to_string()))
            }
        })
        .await;

        let elapsed = start.elapsed();
        assert!(result.is_err());

        // With jitter +-25%:
        // First delay: 50ms * (0.75 to 1.25) = 37.5ms to 62.5ms
        // Second delay: 100ms * (0.75 to 1.25) = 75ms to 125ms
        // Total: ~112.5ms to 187.5ms
        // Allow some tolerance for test execution overhead
        assert!(
            elapsed >= Duration::from_millis(80),
            "elapsed time {elapsed:?} should be at least 80ms (got some backoff)"
        );
        assert!(
            elapsed <= Duration::from_millis(300),
            "elapsed time {elapsed:?} should be under 300ms (not too much backoff)"
        );
    }

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.initial_delay, Duration::from_millis(500));
        assert_eq!(config.max_delay, Duration::from_secs(30));
        assert!((config.backoff_multiplier - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_retry_config_no_retry() {
        let config = RetryConfig::no_retry();
        assert_eq!(config.max_retries, 0);
    }
}
