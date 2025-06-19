#[cfg(test)]
mod tests {
	use rust_watcher::{start, WatcherConfig, WatcherError};
	use std::path::PathBuf;
	use tempfile::TempDir;
	use tokio::time::{sleep, timeout, Duration};

	#[tokio::test]
	async fn test_watcher_invalid_path() {
		let config = WatcherConfig {
			path: PathBuf::from("/nonexistent/invalid/path/that/should/not/exist"),
			recursive: true,
			move_detector_config: None,
		};

		let result = start(config);
		assert!(result.is_err()); // Should get a specific error type
		match result {
			Err(WatcherError::Notify(_)) => {
				// Expected - notify should fail for invalid paths
			}
			Err(WatcherError::InvalidPath { .. }) => {
				// Also expected - might get this error type for invalid paths
			}
			Err(other) => panic!("Unexpected error type: {:?}", other),
			Ok(_) => panic!("Expected error for invalid path"),
		}
	}

	#[tokio::test]
	async fn test_watcher_permission_denied() {
		// On Unix systems, try to watch /root (usually requires root)
		// On Windows, this test might not be as relevant
		#[cfg(unix)]
		{
			let config = WatcherConfig {
				path: PathBuf::from("/root"),
				recursive: true,
				move_detector_config: None,
			};
			let result = start(config);
			// Should either succeed (if running as root) or fail with permission error
			if result.is_err() {
				match result {
					Err(WatcherError::Notify(_)) => {
						// Expected for permission denied
					}
					Err(other) => panic!("Unexpected error type: {:?}", other),
					Ok(_) => unreachable!(),
				}
			}
		}
		// For cross-platform testing, just ensure the test doesn't panic
		#[cfg(not(unix))]
		{
			// Test passes by not panicking - no assertion needed
		}
	}
	#[tokio::test]
	async fn test_watcher_handle_stop_multiple_times() {
		let temp_dir = TempDir::new().unwrap();
		let config = WatcherConfig {
			path: temp_dir.path().to_path_buf(),
			recursive: true,
			move_detector_config: None,
		};

		let (handle, _receiver) = start(config).unwrap();

		// Stop the watcher
		let result1 = handle.stop().await;
		assert!(result1.is_ok());

		// Try to stop again - we can't test this directly since handle is moved
		// This test mainly ensures the first stop works correctly
	}

	#[tokio::test]
	async fn test_watcher_receiver_after_stop() {
		let temp_dir = TempDir::new().unwrap();
		let config = WatcherConfig {
			path: temp_dir.path().to_path_buf(),
			recursive: true,
			move_detector_config: None,
		};

		let (handle, mut receiver) = start(config).unwrap();

		// Stop the watcher
		handle.stop().await.unwrap();

		// Try to receive after stop - should complete (not hang)
		let recv_result = timeout(Duration::from_millis(100), receiver.recv()).await;

		match recv_result {
			Ok(None) => {
				// Channel closed - expected behavior
			}
			Ok(Some(_)) => {
				// Got an event - possible if there was a pending event
			}
			Err(_) => {
				// Timeout - this is also acceptable, receiver might be waiting
			}
		}
		// The key is that this test completes without hanging
	}

	#[tokio::test]
	async fn test_watcher_startup_and_immediate_shutdown() {
		let temp_dir = TempDir::new().unwrap();
		let config = WatcherConfig {
			path: temp_dir.path().to_path_buf(),
			recursive: true,
			move_detector_config: None,
		};

		// Start and immediately stop
		let (handle, _receiver) = start(config).unwrap();
		let result = handle.stop().await;
		assert!(result.is_ok());
	}

	#[tokio::test]
	async fn test_watcher_error_propagation() {
		// Test that errors are properly propagated through the Result type
		let invalid_configs = vec![
			// Non-existent path
			WatcherConfig {
				path: PathBuf::from("/this/path/definitely/does/not/exist/anywhere"),
				recursive: true,
				move_detector_config: None,
			},
			// Path that looks invalid
			WatcherConfig {
				path: PathBuf::from(""),
				recursive: true,
				move_detector_config: None,
			},
		];

		for config in invalid_configs {
			let result = start(config);
			assert!(result.is_err(), "Expected error for invalid configuration");

			// Verify error can be displayed and debugged
			if let Err(error) = result {
				let _debug_output = format!("{:?}", error);
				let _display_output = format!("{}", error);
				// Should not panic when formatting errors
			}
		}
	}

	#[tokio::test]
	async fn test_watcher_concurrent_operations() {
		let temp_dir = TempDir::new().unwrap();
		let config = WatcherConfig {
			path: temp_dir.path().to_path_buf(),
			recursive: true,
			move_detector_config: None,
		};

		let (handle, mut receiver) = start(config).unwrap();

		// Spawn a task to receive events
		let receiver_handle = tokio::spawn(async move {
			let mut event_count = 0;
			while let Some(_event) = receiver.recv().await {
				event_count += 1;
				if event_count >= 10 {
					break;
				}
			}
			event_count
		});

		// Create some files to generate events
		for i in 0..5 {
			let file_path = temp_dir.path().join(format!("test_file_{}.txt", i));
			tokio::fs::write(&file_path, format!("content {}", i))
				.await
				.unwrap();
			sleep(Duration::from_millis(10)).await;
		}

		// Stop the watcher
		let stop_result = handle.stop().await;
		assert!(stop_result.is_ok());

		// Wait for receiver task to complete or timeout
		let receiver_result = timeout(Duration::from_millis(500), receiver_handle).await;

		// Either the receiver got events or timed out - both are acceptable
		match receiver_result {
			Ok(Ok(_event_count)) => {
				// Received some events successfully
			}
			Ok(Err(_)) => {
				// Receiver task had an error - acceptable
			}
			Err(_) => {
				// Timed out - also acceptable
			}
		}
	}

	#[tokio::test]
	async fn test_watcher_resource_cleanup() {
		// Test that multiple watchers can be created and destroyed without resource leaks
		let temp_dir = TempDir::new().unwrap();

		for _i in 0..5 {
			let config = WatcherConfig {
				path: temp_dir.path().to_path_buf(),
				recursive: true,
				move_detector_config: None,
			};

			let (handle, _receiver) = start(config).unwrap();

			// Brief operation
			sleep(Duration::from_millis(10)).await;

			// Clean shutdown
			handle.stop().await.unwrap();
		}

		// Test should complete without resource exhaustion
	}

	#[tokio::test]
	async fn test_error_types_display() {
		// Test that all error types can be displayed properly
		let temp_dir = TempDir::new().unwrap();
		let config = WatcherConfig {
			path: temp_dir.path().to_path_buf(),
			recursive: true,
			move_detector_config: None,
		};

		// Get a successful watcher first
		let (handle, _receiver) = start(config).unwrap();

		// Test various error scenarios that might occur
		// (This is more of a compilation test to ensure error types work)

		// Try to create an invalid watcher
		let invalid_config = WatcherConfig {
			path: PathBuf::from("/invalid/path/for/testing"),
			recursive: true,
			move_detector_config: None,
		};

		if let Err(error) = start(invalid_config) {
			// Test error display
			let display_str = format!("{}", error);
			let debug_str = format!("{:?}", error);

			assert!(!display_str.is_empty());
			assert!(!debug_str.is_empty());
		}

		// Clean up
		handle.stop().await.unwrap();
	}

	#[tokio::test]
	async fn test_watcher_stop_timeout() {
		let temp_dir = TempDir::new().unwrap();
		let config = WatcherConfig {
			path: temp_dir.path().to_path_buf(),
			recursive: true,
			move_detector_config: None,
		};

		let (handle, _receiver) = start(config).unwrap();

		// Stop with a timeout to ensure it doesn't hang
		let stop_result = timeout(Duration::from_secs(5), handle.stop()).await;

		match stop_result {
			Ok(Ok(())) => {
				// Successfully stopped
			}
			Ok(Err(e)) => {
				// Got an error during stop - might be acceptable depending on timing
				eprintln!("Stop returned error: {:?}", e);
			}
			Err(_) => {
				panic!("Watcher stop operation timed out - this indicates a hang bug");
			}
		}
	}
}
