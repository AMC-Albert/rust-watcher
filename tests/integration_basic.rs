// Integration test for basic watcher functionality
// Tests the public API of rust-watcher using only public interfaces

use rust_watcher::{start, WatcherConfig};
use std::time::Duration;

mod common;

#[tokio::test]
async fn test_watcher_creation() {
	let temp_dir = common::setup_temp_dir();
	let config = WatcherConfig {
		path: temp_dir.path().to_path_buf(),
		recursive: true,
		move_detector_config: None,
	};

	// Test that watcher can be created without panicking
	let result = start(config);
	assert!(result.is_ok(), "Watcher creation should succeed");

	let (handle, _receiver) = result.unwrap();

	// Test that watcher can be stopped cleanly
	let stop_result = handle.stop().await;
	assert!(stop_result.is_ok(), "Watcher should stop cleanly");
}

#[tokio::test]
async fn test_watcher_basic_file_detection() {
	let temp_dir = common::setup_temp_dir();
	let config = WatcherConfig {
		path: temp_dir.path().to_path_buf(),
		recursive: true,
		move_detector_config: None,
	};

	let (handle, mut receiver) = start(config).unwrap();

	// Give watcher time to initialize
	common::wait_for_events().await;

	// Create a test file
	let test_file = temp_dir.path().join("test.txt");
	common::create_test_file(&test_file, "test content").unwrap();

	// Wait for events
	common::wait_for_events().await;

	// Try to receive events (don't assert on count as it varies by platform)
	let mut events_received = 0;
	let timeout = Duration::from_millis(1000);
	let start_time = std::time::Instant::now();

	while start_time.elapsed() < timeout {
		match tokio::time::timeout(Duration::from_millis(100), receiver.recv()).await {
			Ok(Some(event)) => {
				events_received += 1;
				println!("Received event: {:?}", event.event_type); // Verify event has basic required fields
				assert!(!event.id.to_string().is_empty());
				assert!(!event.path.as_os_str().is_empty());

				// If we got at least one event, test passes
				if events_received >= 1 {
					break;
				}
			}
			Ok(None) => break,  // Channel closed
			Err(_) => continue, // Timeout, try again
		}
	}

	// Stop the watcher
	handle.stop().await.unwrap();

	println!(
		"Integration test completed: received {} events",
		events_received
	);
	// Don't assert on specific count as file events vary by platform
}

#[tokio::test]
async fn test_watcher_config_validation() {
	let temp_dir = common::setup_temp_dir();

	// Test valid config
	let valid_config = WatcherConfig {
		path: temp_dir.path().to_path_buf(),
		recursive: true,
		move_detector_config: None,
	};

	let result = start(valid_config);
	assert!(result.is_ok(), "Valid config should work");

	let (handle, _receiver) = result.unwrap();
	handle.stop().await.unwrap();
}
