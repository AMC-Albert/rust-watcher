// Integration tests for error handling scenarios
// Tests the public API error handling using only public interfaces

use rust_watcher::{start, WatcherConfig, WatcherError};
use std::path::PathBuf;

mod common;

#[test]
fn test_watcher_invalid_path() {
	common::run_async_test(async {
		let config = WatcherConfig {
			path: PathBuf::from("/nonexistent/invalid/path/that/should/not/exist"),
			recursive: true,
			move_detector_config: None,
		};

		let result = start(config);
		assert!(result.is_err());

		match result.unwrap_err() {
			WatcherError::InvalidPath { path } => {
				assert!(path.contains("nonexistent"));
			}
			other => panic!("Expected InvalidPath error, got: {:?}", other),
		}
	});
}

#[test]
fn test_watcher_stop_after_start() {
	common::run_async_test(async {
		let temp_dir = common::setup_temp_dir();
		let config = WatcherConfig {
			path: temp_dir.path().to_path_buf(),
			recursive: true,
			move_detector_config: None,
		};

		let (handle, _receiver) = start(config).unwrap();

		// Test that watcher can be stopped cleanly
		let stop_result = handle.stop().await;
		assert!(stop_result.is_ok(), "Watcher should stop cleanly");
	});
}

#[test]
fn test_watcher_multiple_start_stop() {
	common::run_async_test(async {
		let temp_dir = common::setup_temp_dir();

		for i in 0..3 {
			let config = WatcherConfig {
				path: temp_dir.path().to_path_buf(),
				recursive: true,
				move_detector_config: None,
			};

			let (handle, _receiver) = start(config)
				.unwrap_or_else(|e| panic!("Failed to start watcher on iteration {}: {:?}", i, e));

			common::wait_for_events().await;

			let stop_result = handle.stop().await;
			assert!(
				stop_result.is_ok(),
				"Watcher should stop cleanly on iteration {}",
				i
			);
		}
	});
}

#[cfg(unix)]
#[test]
fn test_watcher_permission_denied() {
	common::run_async_test(async {
		// On Unix systems, try to watch /root (usually requires root)
		let config = WatcherConfig {
			path: PathBuf::from("/root"),
			recursive: true,
			move_detector_config: None,
		};
		let result = start(config);

		// Should either succeed (if running as root) or fail with permission error
		if result.is_err() {
			match result.unwrap_err() {
				WatcherError::Notify(_) => {
					// Expected for permission denied
				}
				other => panic!("Unexpected error type: {:?}", other),
			}
		}
		// If it succeeds, we're running as root - clean up
		else {
			let (handle, _) = result.unwrap();
			let _ = handle.stop().await;
		}
	});
}
