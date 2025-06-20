use std::time::Duration;
use thiserror::Error;

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

	// Enhanced error categories for comprehensive error handling
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
	#[error("Database error: {0}")]
	Database(Box<crate::database::DatabaseError>),

	#[error("Move detection error: {operation} on {path} - {details} (confidence: {confidence})")]
	MoveDetection {
		operation: String,
		path: String,
		details: String,
		confidence: f64,
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
			WatcherError::FilesystemError { .. } => true, // Database errors that might be retryable
			WatcherError::Database(db_err) => db_err.is_retryable(),

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
			WatcherError::MoveDetection { .. } => false,
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
		matches!(
			self,
			WatcherError::ConfigurationError { .. } | WatcherError::InvalidPath { .. }
		)
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
			WatcherError::MoveDetection { .. } => "move_detection",
			WatcherError::SystemResourceUnavailable { .. } => "system_resource",
			WatcherError::RateLimitExceeded { .. } => "rate_limit",
			WatcherError::NetworkError { .. } => "network",
			WatcherError::ValidationError { .. } => "validation",
			WatcherError::ConcurrencyError { .. } => "concurrency",
		}
	}
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
		operation: &str,
		path: &str,
		cause: &str,
		error_code: Option<i32>,
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
		resource: &str,
		details: &str,
		current_usage: &str,
		limit: &str,
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
		WatcherError::Timeout {
			operation: operation.to_string(),
			timeout,
			start_time,
		}
	}

	/// Create a configuration error
	pub fn configuration_error(
		parameter: &str,
		reason: &str,
		expected: &str,
		actual: &str,
	) -> Self {
		WatcherError::ConfigurationError {
			parameter: parameter.to_string(),
			reason: reason.to_string(),
			expected: expected.to_string(),
			actual: actual.to_string(),
		}
	}

	/// Create a move detection error
	pub fn move_detection_error(
		operation: &str,
		path: &str,
		details: &str,
		confidence: f64,
	) -> Self {
		WatcherError::MoveDetection {
			operation: operation.to_string(),
			path: path.to_string(),
			details: details.to_string(),
			confidence,
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
		// Test that all error variants can be created
		let io_error = WatcherError::Io(io::Error::new(io::ErrorKind::NotFound, "file not found"));
		let channel_error = WatcherError::ChannelSend;
		let invalid_path = WatcherError::InvalidPath {
			path: "/invalid".to_string(),
		};
		let stop_signal = WatcherError::StopSignal;
		let not_initialized = WatcherError::NotInitialized;

		// Test enhanced error variants
		let permission_denied = WatcherError::PermissionDenied {
			operation: "read".to_string(),
			path: "/protected".to_string(),
			context: "filesystem access".to_string(),
		};

		let resource_exhausted = WatcherError::ResourceExhausted {
			resource: "memory".to_string(),
			details: "allocation failed".to_string(),
			current_usage: "1GB".to_string(),
			limit: "512MB".to_string(),
		};

		let filesystem_error = WatcherError::FilesystemError {
			operation: "watch".to_string(),
			path: "/tmp".to_string(),
			cause: "device full".to_string(),
			error_code: Some(28), // ENOSPC
		};

		// Test error messages
		assert!(io_error.to_string().contains("IO error"));
		assert!(channel_error.to_string().contains("Channel send error"));
		assert!(invalid_path.to_string().contains("Invalid path"));
		assert!(stop_signal.to_string().contains("stop signal"));
		assert!(not_initialized.to_string().contains("not initialized"));
		assert!(permission_denied.to_string().contains("Permission denied"));
		assert!(resource_exhausted
			.to_string()
			.contains("Resource exhausted"));
		assert!(filesystem_error.to_string().contains("Filesystem error"));
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
	fn test_result_type() {
		// Test the Result type alias
		let success: Result<i32> = Ok(42);
		let failure: Result<i32> = Err(WatcherError::NotInitialized);

		assert!(success.is_ok());
		assert!(failure.is_err());
		if let Ok(value) = success {
			assert_eq!(value, 42);
		}
	}

	#[test]
	fn test_error_categorization() {
		// Test retryable errors
		let timeout_error = WatcherError::Timeout {
			operation: "test".to_string(),
			timeout: Duration::from_secs(5),
			start_time: "2025-01-01T00:00:00Z".to_string(),
		};
		assert!(timeout_error.is_retryable());
		assert_eq!(timeout_error.category(), "timeout");

		let resource_error = WatcherError::ResourceExhausted {
			resource: "memory".to_string(),
			details: "out of memory".to_string(),
			current_usage: "2GB".to_string(),
			limit: "1GB".to_string(),
		};
		assert!(resource_error.is_retryable());
		assert!(resource_error.is_resource_limit());
		assert_eq!(resource_error.category(), "resource");

		// Test non-retryable errors
		let permission_error = WatcherError::PermissionDenied {
			operation: "read".to_string(),
			path: "/root".to_string(),
			context: "insufficient privileges".to_string(),
		};
		assert!(!permission_error.is_retryable());
		assert!(permission_error.is_critical());
		assert_eq!(permission_error.category(), "permission");

		let config_error = WatcherError::ConfigurationError {
			parameter: "path".to_string(),
			reason: "invalid format".to_string(),
			expected: "absolute path".to_string(),
			actual: "relative/path".to_string(),
		};
		assert!(!config_error.is_retryable());
		assert!(config_error.is_configuration_error());
		assert_eq!(config_error.category(), "configuration");
	}

	#[test]
	fn test_error_constructor_methods() {
		// Test the convenience constructors
		let fs_error = WatcherError::filesystem_error_with_path(
			"read",
			"/tmp/test",
			"file not found",
			Some(2),
		);
		if let WatcherError::FilesystemError {
			operation,
			path,
			cause,
			error_code,
		} = fs_error
		{
			assert_eq!(operation, "read");
			assert_eq!(path, "/tmp/test");
			assert_eq!(cause, "file not found");
			assert_eq!(error_code, Some(2));
		} else {
			panic!("Expected FilesystemError variant");
		}

		let resource_error = WatcherError::resource_exhausted_with_usage(
			"file descriptors",
			"limit reached",
			"1024",
			"1024",
		);
		if let WatcherError::ResourceExhausted {
			resource,
			details,
			current_usage,
			limit,
		} = resource_error
		{
			assert_eq!(resource, "file descriptors");
			assert_eq!(details, "limit reached");
			assert_eq!(current_usage, "1024");
			assert_eq!(limit, "1024");
		} else {
			panic!("Expected ResourceExhausted variant");
		}

		let validation_error =
			WatcherError::validation_error("timeout_ms", "must be positive", "-100");
		if let WatcherError::ValidationError {
			field,
			reason,
			value,
		} = validation_error
		{
			assert_eq!(field, "timeout_ms");
			assert_eq!(reason, "must be positive");
			assert_eq!(value, "-100");
		} else {
			panic!("Expected ValidationError variant");
		}
	}

	#[test]
	fn test_error_recovery_config() {
		let config = ErrorRecoveryConfig::default();
		assert_eq!(config.max_retries, 3);
		assert_eq!(config.initial_retry_delay, Duration::from_millis(100));
		assert_eq!(config.max_retry_delay, Duration::from_secs(30));
		assert_eq!(config.backoff_multiplier, 2.0);
		assert!(config.exponential_backoff);

		// Test delay calculation
		assert_eq!(config.delay_for_attempt(0), Duration::from_millis(100));
		assert_eq!(config.delay_for_attempt(1), Duration::from_millis(200));
		assert_eq!(config.delay_for_attempt(2), Duration::from_millis(400));

		// Test with large attempt number (should be capped)
		let large_delay = config.delay_for_attempt(20);
		assert!(large_delay <= config.max_retry_delay);
	}
}
