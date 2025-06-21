//! Retry mechanism for handling transient errors
//!
//! Provides utilities for implementing exponential backoff and retry logic
//! for recoverable errors in the filesystem watcher.

use crate::error::{ErrorRecoveryConfig, Result, WatcherError};
use std::future::Future;
use std::pin::Pin;
use tracing::{debug, warn};

/// A trait for operations that can be retried
pub trait RetryableOperation<T> {
	/// Execute the operation, returning a result
	fn execute(&mut self) -> Pin<Box<dyn Future<Output = Result<T>> + Send + '_>>;

	/// Get a description of the operation for logging
	fn operation_name(&self) -> &str;
}

/// Retry manager that handles the retry logic with exponential backoff
#[derive(Debug, Default)]
pub struct RetryManager {
	config: ErrorRecoveryConfig,
}

impl RetryManager {
	/// Create a new retry manager with the given configuration
	pub fn new(config: ErrorRecoveryConfig) -> Self {
		Self { config }
	}
}

impl RetryManager {
	/// Execute an operation with retry logic
	pub async fn execute<T, F>(&self, mut operation: F) -> Result<T>
	where F: RetryableOperation<T> {
		let mut attempt = 0;
		let start_time = std::time::Instant::now();

		loop {
			match operation.execute().await {
				Ok(result) => {
					if attempt > 0 {
						debug!(
							"Operation '{}' succeeded after {} attempts in {:?}",
							operation.operation_name(),
							attempt + 1,
							start_time.elapsed()
						);
					}
					return Ok(result);
				}
				Err(error) => {
					// Check if the error is retryable
					if !error.is_retryable() {
						debug!(
							"Operation '{}' failed with non-retryable error: {}",
							operation.operation_name(),
							error
						);
						return Err(error);
					}

					// Check if we've exceeded max retries
					if attempt >= self.config.max_retries {
						warn!(
							"Operation '{}' failed after {} attempts over {:?}, giving up",
							operation.operation_name(),
							attempt + 1,
							start_time.elapsed()
						);
						return Err(WatcherError::RecoveryFailed {
							operation: operation.operation_name().to_string(),
							attempts: attempt + 1,
							total_duration: start_time.elapsed(),
							last_error: error.to_string(),
						});
					}

					// Calculate delay and wait
					let delay = self.config.delay_for_attempt(attempt);
					warn!(
						"Operation '{}' failed (attempt {}), retrying in {:?}: {}",
						operation.operation_name(),
						attempt + 1,
						delay,
						error
					);

					tokio::time::sleep(delay).await;
					attempt += 1;
				}
			}
		}
	}
	/// Execute a simple async closure with retry logic
	pub async fn execute_simple<T, F, Fut>(
		&self, operation_name: &str, mut operation_fn: F,
	) -> Result<T>
	where
		F: FnMut() -> Fut + Send,
		Fut: Future<Output = Result<T>> + Send,
	{
		let mut attempt = 0;
		let start_time = std::time::Instant::now();

		loop {
			match operation_fn().await {
				Ok(result) => {
					if attempt > 0 {
						debug!(
							"Simple operation '{}' succeeded after {} attempts in {:?}",
							operation_name,
							attempt + 1,
							start_time.elapsed()
						);
					}
					return Ok(result);
				}
				Err(error) => {
					if !error.is_retryable() {
						debug!(
							"Simple operation '{}' failed with non-retryable error: {}",
							operation_name, error
						);
						return Err(error);
					}

					if attempt >= self.config.max_retries {
						warn!(
							"Simple operation '{}' failed after {} attempts over {:?}, giving up",
							operation_name,
							attempt + 1,
							start_time.elapsed()
						);
						return Err(WatcherError::RecoveryFailed {
							operation: operation_name.to_string(),
							attempts: attempt + 1,
							total_duration: start_time.elapsed(),
							last_error: error.to_string(),
						});
					}

					let delay = self.config.delay_for_attempt(attempt);
					warn!(
						"Simple operation '{}' failed (attempt {}), retrying in {:?}: {}",
						operation_name,
						attempt + 1,
						delay,
						error
					);

					tokio::time::sleep(delay).await;
					attempt += 1;
				}
			}
		}
	}
}

/// A builder for creating retry configurations
#[derive(Debug)]
pub struct RetryConfigBuilder {
	config: ErrorRecoveryConfig,
}

impl RetryConfigBuilder {
	/// Create a new builder with default configuration
	pub fn new() -> Self {
		Self { config: ErrorRecoveryConfig::default() }
	}

	/// Set the maximum number of retry attempts
	pub fn max_retries(mut self, max_retries: u32) -> Self {
		self.config.max_retries = max_retries;
		self
	}

	/// Set the initial retry delay
	pub fn initial_delay(mut self, delay: std::time::Duration) -> Self {
		self.config.initial_retry_delay = delay;
		self
	}

	/// Set the maximum retry delay
	pub fn max_delay(mut self, delay: std::time::Duration) -> Self {
		self.config.max_retry_delay = delay;
		self
	}

	/// Set the backoff multiplier
	pub fn backoff_multiplier(mut self, multiplier: f64) -> Self {
		self.config.backoff_multiplier = multiplier;
		self
	}

	/// Enable or disable exponential backoff
	pub fn exponential_backoff(mut self, enabled: bool) -> Self {
		self.config.exponential_backoff = enabled;
		self
	}

	/// Build the configuration
	pub fn build(self) -> ErrorRecoveryConfig {
		self.config
	}
}

impl Default for RetryConfigBuilder {
	fn default() -> Self {
		Self::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::atomic::{AtomicU32, Ordering};
	use std::sync::Arc;
	use std::time::Duration;

	struct TestOperation {
		name: String,
		counter: Arc<AtomicU32>,
		fail_until: u32,
	}

	impl RetryableOperation<String> for TestOperation {
		fn execute(&mut self) -> Pin<Box<dyn Future<Output = Result<String>> + Send + '_>> {
			Box::pin(async move {
				let count = self.counter.fetch_add(1, Ordering::SeqCst);
				if count < self.fail_until {
					Err(WatcherError::ResourceExhausted {
						resource: "test".to_string(),
						details: format!("attempt {}", count + 1),
						current_usage: "unknown".to_string(),
						limit: "unknown".to_string(),
					})
				} else {
					Ok(format!("success after {} attempts", count + 1))
				}
			})
		}

		fn operation_name(&self) -> &str {
			&self.name
		}
	}

	#[tokio::test]
	async fn test_retry_success_after_failures() {
		let config = RetryConfigBuilder::new()
			.max_retries(3)
			.initial_delay(Duration::from_millis(1))
			.exponential_backoff(false)
			.build();

		let retry_manager = RetryManager::new(config);
		let counter = Arc::new(AtomicU32::new(0));

		let operation = TestOperation {
			name: "test_operation".to_string(),
			counter: counter.clone(),
			fail_until: 2, // Fail first 2 attempts, succeed on 3rd
		};

		let result = retry_manager.execute(operation).await;
		assert!(result.is_ok());
		assert_eq!(result.unwrap(), "success after 3 attempts");
		assert_eq!(counter.load(Ordering::SeqCst), 3);
	}

	#[tokio::test]
	async fn test_retry_max_attempts_exceeded() {
		let config = RetryConfigBuilder::new()
			.max_retries(2)
			.initial_delay(Duration::from_millis(1))
			.build();

		let retry_manager = RetryManager::new(config);
		let counter = Arc::new(AtomicU32::new(0));

		let operation = TestOperation {
			name: "failing_operation".to_string(),
			counter: counter.clone(),
			fail_until: 10, // Always fail
		};

		let result = retry_manager.execute(operation).await;
		assert!(result.is_err());

		match result.unwrap_err() {
			WatcherError::RecoveryFailed { attempts, .. } => {
				assert_eq!(attempts, 3); // Initial attempt + 2 retries
			}
			_ => panic!("Expected RecoveryFailed error"),
		}
	}

	#[tokio::test]
	async fn test_retry_non_retryable_error() {
		let config = RetryConfigBuilder::new()
			.max_retries(3)
			.initial_delay(Duration::from_millis(1))
			.build();
		let retry_manager = RetryManager::new(config);
		let result: Result<()> = retry_manager
			.execute_simple("non_retryable", || async {
				Err(WatcherError::PermissionDenied {
					operation: "test".to_string(),
					path: "/test".to_string(),
					context: "test operation".to_string(),
				})
			})
			.await;

		assert!(result.is_err());
		match result.unwrap_err() {
			WatcherError::PermissionDenied { .. } => {
				// Expected - should not retry
			}
			_ => panic!("Expected PermissionDenied error"),
		}
	}

	#[test]
	fn test_retry_config_builder() {
		let config = RetryConfigBuilder::new()
			.max_retries(5)
			.initial_delay(Duration::from_millis(200))
			.max_delay(Duration::from_secs(10))
			.backoff_multiplier(1.5)
			.exponential_backoff(true)
			.build();

		assert_eq!(config.max_retries, 5);
		assert_eq!(config.initial_retry_delay, Duration::from_millis(200));
		assert_eq!(config.max_retry_delay, Duration::from_secs(10));
		assert_eq!(config.backoff_multiplier, 1.5);
		assert!(config.exponential_backoff);
	}

	#[test]
	fn test_delay_calculation() {
		let config = RetryConfigBuilder::new()
			.initial_delay(Duration::from_millis(100))
			.backoff_multiplier(2.0)
			.max_delay(Duration::from_millis(500))
			.build();

		assert_eq!(config.delay_for_attempt(0), Duration::from_millis(100));
		assert_eq!(config.delay_for_attempt(1), Duration::from_millis(200));
		assert_eq!(config.delay_for_attempt(2), Duration::from_millis(400));
		assert_eq!(config.delay_for_attempt(3), Duration::from_millis(500)); // Capped
	}
}
