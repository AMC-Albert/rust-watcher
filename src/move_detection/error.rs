//! Move detection specific error types

use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MoveDetectionError {
	#[error("Move detection operation failed: {operation} on {path} - {details}")]
	OperationFailed {
		operation: String,
		path: String,
		details: String,
	},

	#[error("Move confidence too low: {confidence:.2} < {threshold:.2} for {path}")]
	ConfidenceTooLow {
		confidence: f64,
		threshold: f64,
		path: String,
	},

	#[error("Move detection timeout: operation {operation} exceeded {timeout:?}")]
	Timeout {
		operation: String,
		timeout: Duration,
	},

	#[error("Invalid move detection configuration: {parameter} - {reason}")]
	InvalidConfiguration { parameter: String, reason: String },

	#[error("Metadata extraction failed for {path}: {cause}")]
	MetadataExtractionFailed { path: String, cause: String },

	#[error("Move matching algorithm failed: {algorithm} - {details}")]
	MatchingFailed { algorithm: String, details: String },

	#[error("Resource limit exceeded in move detection: {resource} - {details}")]
	ResourceLimitExceeded { resource: String, details: String },
}

impl MoveDetectionError {
	/// Check if this error indicates that the operation should be retried
	pub fn is_retryable(&self) -> bool {
		matches!(
			self,
			MoveDetectionError::Timeout { .. }
				| MoveDetectionError::ResourceLimitExceeded { .. }
				| MoveDetectionError::MetadataExtractionFailed { .. }
		)
	}

	/// Check if this error is due to configuration issues
	pub fn is_configuration_error(&self) -> bool {
		matches!(self, MoveDetectionError::InvalidConfiguration { .. })
	}

	/// Get error category for logging and metrics
	pub fn category(&self) -> &'static str {
		match self {
			MoveDetectionError::OperationFailed { .. } => "operation",
			MoveDetectionError::ConfidenceTooLow { .. } => "confidence",
			MoveDetectionError::Timeout { .. } => "timeout",
			MoveDetectionError::InvalidConfiguration { .. } => "configuration",
			MoveDetectionError::MetadataExtractionFailed { .. } => "metadata",
			MoveDetectionError::MatchingFailed { .. } => "matching",
			MoveDetectionError::ResourceLimitExceeded { .. } => "resource_limit",
		}
	}

	/// Create a confidence error
	pub fn confidence_too_low(confidence: f64, threshold: f64, path: &str) -> Self {
		MoveDetectionError::ConfidenceTooLow {
			confidence,
			threshold,
			path: path.to_string(),
		}
	}

	/// Create a metadata extraction error
	pub fn metadata_extraction_failed(path: &str, cause: &str) -> Self {
		MoveDetectionError::MetadataExtractionFailed {
			path: path.to_string(),
			cause: cause.to_string(),
		}
	}

	/// Create a matching algorithm error
	pub fn matching_failed(algorithm: &str, details: &str) -> Self {
		MoveDetectionError::MatchingFailed {
			algorithm: algorithm.to_string(),
			details: details.to_string(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_move_detection_error_variants() {
		let confidence_error = MoveDetectionError::confidence_too_low(0.3, 0.5, "/test/path");
		assert!(!confidence_error.is_retryable());
		assert_eq!(confidence_error.category(), "confidence");

		let timeout_error = MoveDetectionError::Timeout {
			operation: "match_files".to_string(),
			timeout: Duration::from_secs(30),
		};
		assert!(timeout_error.is_retryable());
		assert_eq!(timeout_error.category(), "timeout");
	}

	#[test]
	fn test_error_categorization() {
		let config_error = MoveDetectionError::InvalidConfiguration {
			parameter: "timeout".to_string(),
			reason: "must be positive".to_string(),
		};
		assert!(config_error.is_configuration_error());
		assert!(!config_error.is_retryable());

		let metadata_error =
			MoveDetectionError::metadata_extraction_failed("/test", "permission denied");
		assert!(metadata_error.is_retryable());
		assert!(!metadata_error.is_configuration_error());
	}

	#[test]
	fn test_error_display() {
		let error = MoveDetectionError::confidence_too_low(0.25, 0.8, "/test/file.txt");
		let error_str = format!("{}", error);
		assert!(error_str.contains("0.25"));
		assert!(error_str.contains("0.80"));
		assert!(error_str.contains("/test/file.txt"));
	}
}
