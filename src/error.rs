use std::time::Duration;
use thiserror::Error;

/// Core watcher error types
///
/// This enum contains only errors that are specific to the core watcher functionality.
/// Module-specific errors are defined in their respective modules:
/// - Database errors: `crate::database::DatabaseError`
/// - Move detection errors: `crate::move_detection::MoveDetectionError`
#[derive(Error, Debug)]
pub enum WatcherError {
	#[error("IO error: {0}")]
	Io(#[from] std::io::Error),

	#[error("Notify error: {0}")]
	Notify(#[from] notify::Error),

	#[error("JSON serialization error: {0}")]
	Json(#[from] serde_json::Error),

	#[error("Channel send error")]
	ChannelSend,

	#[error("Invalid path: {path}")]
	InvalidPath { path: String },

	#[error("Failed to send stop signal to watcher")]
	StopSignal,

	#[error("Watcher not initialized")]
	NotInitialized,

	// Core watcher-specific errors
	#[error("Permission denied: {operation} on {path} - {context}")]
	PermissionDenied {
		operation: String,
		path: String,
		context: String,
	},

	#[error(
		"Resource exhausted: {resource} - {details} (current: {current_usage}, limit: {limit})"
	)]
	ResourceExhausted {
		resource: String,
		details: String,
		current_usage: String,
		limit: String,
	},

	#[error("Filesystem error: {operation} failed on {path} - {cause}")]
	FilesystemError {
		operation: String,
		path: String,
		cause: String,
		error_code: Option<i32>,
	},

	#[error(
		"Configuration error: {parameter} - {reason} (expected: {expected}, actual: {actual})"
	)]
	ConfigurationError {
		parameter: String,
		reason: String,
		expected: String,
		actual: String,
	},

	#[error("Operation timeout: {operation} exceeded {timeout:?} (started: {start_time})")]
	Timeout {
		operation: String,
		timeout: Duration,
		start_time: String,
	},

	#[error("Recovery failed: {operation} after {attempts} attempts over {total_duration:?} - {last_error}")]
	RecoveryFailed {
		operation: String,
		attempts: u32,
		total_duration: Duration,
		last_error: String,
	},

	#[error("System resource unavailable: {resource} - {reason} (retry_after: {retry_after:?})")]
	SystemResourceUnavailable {
		resource: String,
		reason: String,
		retry_after: Option<Duration>,
	},

	#[error("Rate limit exceeded: {operation} - {current_rate}/{limit} per {window:?}")]
	RateLimitExceeded {
		operation: String,
		current_rate: u64,
		limit: u64,
		window: Duration,
	},

	#[error("Network error: {operation} failed - {cause}")]
	NetworkError {
		operation: String,
		cause: String,
		remote_endpoint: Option<String>,
	},

	#[error("Validation error: {field} is invalid - {reason} (value: {value})")]
	ValidationError {
		field: String,
		reason: String,
		value: String,
	},

	#[error("Concurrency error: {operation} - {details} (thread: {thread_id})")]
	ConcurrencyError {
		operation: String,
		details: String,
		thread_id: String,
	},

	// Module-specific errors (boxed for size optimization)
	#[error("Database error: {0}")]
	Database(#[from] Box<crate::database::DatabaseError>),

	#[error("Move detection error: {0}")]
	MoveDetection(#[from] crate::move_detection::MoveDetectionError),
}

/// Error recovery configuration
#[derive(Debug, Clone)]
pub struct ErrorRecoveryConfig {
	/// Maximum number of retry attempts for recoverable errors
	pub max_retries: u32,
	/// Initial retry delay
	pub initial_retry_delay: Duration,
	/// Maximum retry delay (for exponential backoff)
	pub max_retry_delay: Duration,
	/// Exponential backoff multiplier
	pub backoff_multiplier: f64,
	/// Whether to enable exponential backoff
	pub exponential_backoff: bool,
}

impl Default for ErrorRecoveryConfig {
	fn default() -> Self {
		Self {
			max_retries: 3,
			initial_retry_delay: Duration::from_millis(100),
			max_retry_delay: Duration::from_secs(30),
			backoff_multiplier: 2.0,
			exponential_backoff: true,
		}
	}
}

impl ErrorRecoveryConfig {
	/// Calculate the delay for a given retry attempt
	pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
		if !self.exponential_backoff {
			return self.initial_retry_delay;
		}

		let delay_ms = self.initial_retry_delay.as_millis() as f64
			* self.backoff_multiplier.powi(attempt as i32);

		let delay = Duration::from_millis(delay_ms as u64);
		std::cmp::min(delay, self.max_retry_delay)
	}
}

impl WatcherError {
	/// Check if this error indicates that the operation should be retried
	pub fn is_retryable(&self) -> bool {
		match self {
			// Network and transient I/O errors
			WatcherError::Io(io_err) => matches!(
				io_err.kind(),
				std::io::ErrorKind::TimedOut
					| std::io::ErrorKind::ConnectionRefused
					| std::io::ErrorKind::ConnectionAborted
					| std::io::ErrorKind::Interrupted
					| std::io::ErrorKind::WouldBlock
			),
			// Recoverable notify errors (temporary resource issues)
			WatcherError::Notify(_) => true, // Most notify errors are recoverable

			// Resource exhaustion that might resolve itself
			WatcherError::ResourceExhausted { .. } => true,

			// Transient filesystem errors
			WatcherError::FilesystemError { .. } => true,

			// Database errors that might be retryable
			WatcherError::Database(db_err) => db_err.is_retryable(),

			// Move detection errors that might be retryable
			WatcherError::MoveDetection(move_err) => move_err.is_retryable(),

			// Channel send errors (receiver might reconnect)
			WatcherError::ChannelSend => true,

			// Timeout errors are usually retryable
			WatcherError::Timeout { .. } => true,

			// Rate limiting is temporary
			WatcherError::RateLimitExceeded { .. } => true,

			// System resources might become available
			WatcherError::SystemResourceUnavailable { .. } => true,

			// Network errors might be transient
			WatcherError::NetworkError { .. } => true,

			// Non-retryable errors
			WatcherError::PermissionDenied { .. } => false,
			WatcherError::InvalidPath { .. } => false,
			WatcherError::ConfigurationError { .. } => false,
			WatcherError::NotInitialized => false,
			WatcherError::StopSignal => false,
			WatcherError::RecoveryFailed { .. } => false,
			WatcherError::Json(_) => false,
			WatcherError::ValidationError { .. } => false,
			WatcherError::ConcurrencyError { .. } => true, // Can retry on different thread
		}
	}

	/// Check if this error indicates a critical system failure
	pub fn is_critical(&self) -> bool {
		match self {
			WatcherError::Database(db_err) => db_err.is_corruption(),
			WatcherError::RecoveryFailed { .. } => true,
			WatcherError::PermissionDenied { .. } => true,
			_ => false,
		}
	}

	/// Check if this error is due to resource limitations
	pub fn is_resource_limit(&self) -> bool {
		match self {
			WatcherError::ResourceExhausted { .. } => true,
			WatcherError::Database(db_err) => db_err.is_resource_limit(),
			WatcherError::RateLimitExceeded { .. } => true,
			WatcherError::SystemResourceUnavailable { .. } => true,
			_ => false,
		}
	}

	/// Check if this error is related to configuration issues
	pub fn is_configuration_error(&self) -> bool {
		match self {
			WatcherError::ConfigurationError { .. } | WatcherError::InvalidPath { .. } => true,
			WatcherError::MoveDetection(move_err) => move_err.is_configuration_error(),
			_ => false,
		}
	}

	/// Get error category for logging and metrics
	pub fn category(&self) -> &'static str {
		match self {
			WatcherError::Io(_) => "io",
			WatcherError::Notify(_) => "notify",
			WatcherError::Json(_) => "serialization",
			WatcherError::ChannelSend => "channel",
			WatcherError::InvalidPath { .. } => "configuration",
			WatcherError::StopSignal => "shutdown",
			WatcherError::NotInitialized => "initialization",
			WatcherError::PermissionDenied { .. } => "permission",
			WatcherError::ResourceExhausted { .. } => "resource",
			WatcherError::FilesystemError { .. } => "filesystem",
			WatcherError::ConfigurationError { .. } => "configuration",
			WatcherError::Timeout { .. } => "timeout",
			WatcherError::RecoveryFailed { .. } => "recovery",
			WatcherError::Database(_) => "database",
			WatcherError::MoveDetection(_) => "move_detection",
			WatcherError::SystemResourceUnavailable { .. } => "system_resource",
			WatcherError::RateLimitExceeded { .. } => "rate_limit",
			WatcherError::NetworkError { .. } => "network",
			WatcherError::ValidationError { .. } => "validation",
			WatcherError::ConcurrencyError { .. } => "concurrency",
		}
	}

	// Constructor methods for common error patterns
	/// Create a permission denied error from an I/O error
	pub fn from_permission_denied(operation: &str, path: &str, _io_err: std::io::Error) -> Self {
		WatcherError::PermissionDenied {
			operation: operation.to_string(),
			path: path.to_string(),
			context: "IO operation failed due to insufficient permissions".to_string(),
		}
	}

	/// Create a filesystem error from an operation and cause
	pub fn filesystem_error(operation: &str, cause: &str) -> Self {
		WatcherError::FilesystemError {
			operation: operation.to_string(),
			path: "unknown".to_string(),
			cause: cause.to_string(),
			error_code: None,
		}
	}

	/// Create a filesystem error with path context
	pub fn filesystem_error_with_path(
		operation: &str, path: &str, cause: &str, error_code: Option<i32>,
	) -> Self {
		WatcherError::FilesystemError {
			operation: operation.to_string(),
			path: path.to_string(),
			cause: cause.to_string(),
			error_code,
		}
	}

	/// Create a resource exhausted error
	pub fn resource_exhausted(resource: &str, details: &str) -> Self {
		WatcherError::ResourceExhausted {
			resource: resource.to_string(),
			details: details.to_string(),
			current_usage: "unknown".to_string(),
			limit: "unknown".to_string(),
		}
	}

	/// Create a resource exhausted error with usage details
	pub fn resource_exhausted_with_usage(
		resource: &str, details: &str, current_usage: &str, limit: &str,
	) -> Self {
		WatcherError::ResourceExhausted {
			resource: resource.to_string(),
			details: details.to_string(),
			current_usage: current_usage.to_string(),
			limit: limit.to_string(),
		}
	}

	/// Create a timeout error
	pub fn timeout(operation: &str, timeout: Duration) -> Self {
		let start_time = format!("{:?}", std::time::SystemTime::now());
		WatcherError::Timeout { operation: operation.to_string(), timeout, start_time }
	}

	/// Create a configuration error
	pub fn configuration_error(
		parameter: &str, reason: &str, expected: &str, actual: &str,
	) -> Self {
		WatcherError::ConfigurationError {
			parameter: parameter.to_string(),
			reason: reason.to_string(),
			expected: expected.to_string(),
			actual: actual.to_string(),
		}
	}

	/// Create a validation error
	pub fn validation_error(field: &str, reason: &str, value: &str) -> Self {
		WatcherError::ValidationError {
			field: field.to_string(),
			reason: reason.to_string(),
			value: value.to_string(),
		}
	}

	/// Create a network error
	pub fn network_error(operation: &str, cause: &str, remote_endpoint: Option<String>) -> Self {
		WatcherError::NetworkError {
			operation: operation.to_string(),
			cause: cause.to_string(),
			remote_endpoint,
		}
	}

	/// Create a concurrency error
	pub fn concurrency_error(operation: &str, details: &str) -> Self {
		let thread_id = format!("{:?}", std::thread::current().id());
		WatcherError::ConcurrencyError {
			operation: operation.to_string(),
			details: details.to_string(),
			thread_id,
		}
	}
}

// Custom From implementation for boxed database errors
impl From<crate::database::DatabaseError> for WatcherError {
	fn from(err: crate::database::DatabaseError) -> Self {
		WatcherError::Database(Box::new(err))
	}
}

pub type Result<T> = std::result::Result<T, WatcherError>;

#[cfg(test)]
mod tests {
	use super::*;
	use std::io;

	#[test]
	fn test_error_variants() {
		// Test that core error variants can be created
		let io_error = WatcherError::Io(io::Error::new(io::ErrorKind::NotFound, "file not found"));
		let channel_error = WatcherError::ChannelSend;
		let invalid_path = WatcherError::InvalidPath { path: "/invalid".to_string() };

		// Test error messages
		assert!(io_error.to_string().contains("IO error"));
		assert!(channel_error.to_string().contains("Channel send error"));
		assert!(invalid_path.to_string().contains("Invalid path"));
	}

	#[test]
	fn test_from_conversions() {
		// Test automatic conversions
		let io_err = io::Error::new(io::ErrorKind::PermissionDenied, "access denied");
		let watcher_err: WatcherError = io_err.into();

		match watcher_err {
			WatcherError::Io(_) => (), // Expected
			_ => panic!("Expected IO error variant"),
		}
	}

	#[test]
	fn test_error_categorization() {
		let timeout_error = WatcherError::timeout("test_op", Duration::from_secs(30));
		assert!(timeout_error.is_retryable());
		assert_eq!(timeout_error.category(), "timeout");

		let config_error =
			WatcherError::configuration_error("path", "invalid", "/valid", "/invalid");
		assert!(!config_error.is_retryable());
		assert!(config_error.is_configuration_error());
		assert_eq!(config_error.category(), "configuration");
	}

	#[test]
	fn test_error_constructor_methods() {
		let perm_error = WatcherError::from_permission_denied(
			"read_file",
			"/test/path",
			io::Error::new(io::ErrorKind::PermissionDenied, "access denied"),
		);
		assert!(!perm_error.is_retryable());
		assert!(perm_error.is_critical());

		let resource_error = WatcherError::resource_exhausted_with_usage(
			"memory",
			"allocation failed",
			"100MB",
			"64MB",
		);
		assert!(resource_error.is_retryable());
		assert!(resource_error.is_resource_limit());
	}

	#[test]
	fn test_error_recovery_config() {
		let config = ErrorRecoveryConfig::default();
		assert_eq!(config.max_retries, 3);
		assert_eq!(config.initial_retry_delay, Duration::from_millis(100));

		// Test delay calculation
		assert_eq!(config.delay_for_attempt(0), Duration::from_millis(100));
		assert_eq!(config.delay_for_attempt(1), Duration::from_millis(200));
		assert_eq!(config.delay_for_attempt(2), Duration::from_millis(400));

		// Test with large attempt number (should be capped)
		let large_delay = config.delay_for_attempt(20);
		assert!(large_delay <= config.max_retry_delay);
	}
}
