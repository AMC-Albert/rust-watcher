pub mod database;
mod error;
mod events;
mod move_detection;
mod retry;
mod watcher;

pub use database::{DatabaseConfig, DatabaseStorage, RedbStorage};
pub use error::{ErrorRecoveryConfig, Result, WatcherError};
pub use events::{EventType, FileSystemEvent, MoveDetectionMethod, MoveEvent};
pub use move_detection::{MoveDetector, MoveDetectorConfig};
pub use retry::{RetryConfigBuilder, RetryManager, RetryableOperation};
pub use watcher::{start, WatcherConfig, WatcherHandle};

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn test_basic_types_exist() {
		let _event_type = EventType::Create;
		let _method = MoveDetectionMethod::FileSystemEvent;
		println!("Basic types test passed");
	}

	#[test]
	fn test_simple_math() {
		// Simplest possible test to verify test runner works
		assert_eq!(2 + 2, 4);
		println!("Math test passed");
	}

	#[test]
	fn test_string_operations() {
		// Test basic string operations
		let s = "test".to_string();
		assert_eq!(s.len(), 4);
		println!("String test passed");
	}
}
