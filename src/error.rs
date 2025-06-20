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

		// Test error messages
		assert!(io_error.to_string().contains("IO error"));
		assert!(channel_error.to_string().contains("Channel send error"));
		assert!(invalid_path.to_string().contains("Invalid path"));
		assert!(stop_signal.to_string().contains("stop signal"));
		assert!(not_initialized.to_string().contains("not initialized"));
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
}
