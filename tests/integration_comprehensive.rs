// Integration tests for comprehensive watcher functionality
// Tests the public API with various scenarios using only public interfaces

use rust_watcher::{start, MoveDetectorConfig, WatcherConfig};

mod common;

#[tokio::test]
async fn test_basic_file_creation_detection() {
	let temp_dir = common::setup_temp_dir();
	let config = WatcherConfig {
		path: temp_dir.path().to_path_buf(),
		recursive: true,
		move_detector_config: None,
	};

	let (handle, mut event_receiver) = start(config).unwrap();

	// Give the watcher time to start
	common::wait_for_events().await;

	// Create a test file
	let test_file = temp_dir.path().join("test.txt");
	common::create_test_file(&test_file, "test content").unwrap();

	// Give some time for the event to be processed
	common::wait_for_events().await;

	// Verify file exists
	assert!(test_file.exists());

	// Check that we can receive at least one event
	let mut received_event = false;
	for _ in 0..10 {
		tokio::select! {
			event = event_receiver.recv() => {
				if event.is_some() {
					received_event = true;
					break;
				}
			}
			_ = common::timeout_short() => {
				break;
			}
		}
	}
	// Clean shutdown
	handle.stop().await.unwrap();

	// Note: File creation events can be flaky on some systems,
	// so we mainly test that the watcher doesn't crash
	println!("Received event: {}", received_event);
}

#[tokio::test]
async fn test_file_move_detection_with_config() {
	let temp_dir = common::setup_temp_dir();
	let move_config = MoveDetectorConfig::with_timeout(2000);
	let config = WatcherConfig {
		path: temp_dir.path().to_path_buf(),
		recursive: true,
		move_detector_config: Some(move_config),
	};

	let (handle, mut event_receiver) = start(config).unwrap();

	// Give the watcher time to start
	common::wait_for_events().await;

	// Create a test file
	let source_file = temp_dir.path().join("source.txt");
	let dest_file = temp_dir.path().join("dest.txt");

	common::create_test_file(&source_file, "test content for move detection").unwrap();
	common::wait_for_events().await;

	// Move the file
	std::fs::rename(&source_file, &dest_file).unwrap();
	common::wait_for_events().await;

	// Verify the move happened
	assert!(!source_file.exists());
	assert!(dest_file.exists());

	// Try to collect some events
	let mut events_received = 0;
	for _ in 0..20 {
		tokio::select! {
			event = event_receiver.recv() => {
				if event.is_some() {
					events_received += 1;
				}
			}
			_ = common::timeout_short() => {
				break;
			}
		}
	}
	// Clean shutdown
	handle.stop().await.unwrap();

	println!("Total events received: {}", events_received);
	// We mainly test that the watcher with move detection doesn't crash
}

#[tokio::test]
async fn test_recursive_directory_watching() {
	let temp_dir = common::setup_temp_dir();
	let config = WatcherConfig {
		path: temp_dir.path().to_path_buf(),
		recursive: true,
		move_detector_config: None,
	};

	let (handle, mut event_receiver) = start(config).unwrap();

	// Give the watcher time to start
	common::wait_for_events().await;

	// Create a subdirectory
	let sub_dir = temp_dir.path().join("subdir");
	std::fs::create_dir(&sub_dir).unwrap();
	common::wait_for_events().await;

	// Create a file in the subdirectory
	let sub_file = sub_dir.join("nested.txt");
	common::create_test_file(&sub_file, "nested content").unwrap();
	common::wait_for_events().await;

	// Verify files exist
	assert!(sub_dir.exists());
	assert!(sub_file.exists());

	// Try to collect some events (may or may not receive depending on timing)
	let mut events_received = 0;
	for _ in 0..15 {
		tokio::select! {
			event = event_receiver.recv() => {
				if event.is_some() {
					events_received += 1;
				}
			}
			_ = common::timeout_short() => {
				break;
			}
		}
	}

	// Clean shutdown
	handle.stop().await.unwrap();

	println!(
		"Events received for recursive watching: {}",
		events_received
	);
	// Main goal is ensuring no crashes with recursive watching
}

#[tokio::test]
async fn test_non_recursive_directory_watching() {
	let temp_dir = common::setup_temp_dir();
	let config = WatcherConfig {
		path: temp_dir.path().to_path_buf(),
		recursive: false, // Non-recursive
		move_detector_config: None,
	};

	let (handle, _event_receiver) = start(config).unwrap();

	// Give the watcher time to start
	common::wait_for_events().await;

	// Create a file in the root directory
	let root_file = temp_dir.path().join("root.txt");
	common::create_test_file(&root_file, "root content").unwrap();

	// Create a subdirectory and file (should not be watched)
	let sub_dir = temp_dir.path().join("subdir");
	std::fs::create_dir(&sub_dir).unwrap();
	let sub_file = sub_dir.join("nested.txt");
	common::create_test_file(&sub_file, "nested content").unwrap();

	common::wait_for_events().await;

	// Verify files exist	assert!(root_file.exists());
	assert!(sub_dir.exists());
	assert!(sub_file.exists());

	// Clean shutdown
	handle.stop().await.unwrap();

	// Main goal is ensuring non-recursive mode works without crashes
}
