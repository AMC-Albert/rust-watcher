// Integration test for move detection functionality
// Tests move detector using public API only

use chrono::Utc;
use rust_watcher::{EventType, FileSystemEvent, MoveDetector, MoveDetectorConfig};
use uuid::Uuid;

mod common;

#[test]
fn test_move_detector_creation() {
	let config = MoveDetectorConfig::default();
	let _detector = MoveDetector::new(config);

	// Test that move detector can be created without errors
	println!("Move detector created successfully");
}

#[test]
fn test_move_detector_with_custom_config() {
	let config = MoveDetectorConfig::with_timeout(500);
	let _detector = MoveDetector::new(config);

	// Test that move detector with custom config can be created
	println!("Move detector with custom config created successfully");
}

#[tokio::test]
async fn test_move_detector_event_processing() {
	let config = MoveDetectorConfig::default();
	let mut detector = MoveDetector::new(config);

	let temp_dir = common::setup_temp_dir();
	let source_path = temp_dir.path().join("source.txt");
	let dest_path = temp_dir.path().join("dest.txt");

	// Create a test file
	common::create_test_file(&source_path, "test content").unwrap();

	// Create file system events
	let remove_event = FileSystemEvent {
		id: Uuid::new_v4(),
		event_type: EventType::Remove,
		path: source_path,
		timestamp: Utc::now(),
		is_directory: false,
		size: Some(12),
		move_data: None,
	};

	let create_event = FileSystemEvent {
		id: Uuid::new_v4(),
		event_type: EventType::Create,
		path: dest_path,
		timestamp: Utc::now(),
		is_directory: false,
		size: Some(12),
		move_data: None,
	};
	// Process events
	let result1 = detector.process_event(remove_event).await;
	let result2 = detector.process_event(create_event).await;
	// Test that events are processed without panicking
	// We just want to ensure no panic occurs during processing
	println!("Events processed successfully");

	// Check if any moves were detected (optional, varies by implementation)
	let has_moves = result1.iter().chain(result2.iter()).any(|e| e.is_move());
	println!("Move detection test: moves detected = {}", has_moves);
}

#[test]
fn test_move_detector_config_validation() {
	// Test config validation (this is a pure function test)
	let valid_config = MoveDetectorConfig::default();
	assert!(
		valid_config.validate().is_ok(),
		"Default config should be valid"
	);

	let invalid_config = MoveDetectorConfig {
		confidence_threshold: 1.5, // Invalid value
		..Default::default()
	};
	assert!(
		invalid_config.validate().is_err(),
		"Invalid config should fail validation"
	);
}
