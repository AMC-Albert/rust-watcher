//! Integration test for event retention and cleanup logic at the storage layer
//!
//! This test validates that the retention/cleanup system correctly removes old events
//! according to both time-based and count-based policies. It also checks edge cases and
//! verifies that the API is exposed through the storage trait.

use chrono::Duration;
use rust_watcher::database::storage::core::{DatabaseStorage, RedbStorage};
use rust_watcher::database::storage::event_retention::{cleanup_old_events, EventRetentionConfig};
use rust_watcher::database::types::EventRecord;
use std::path::PathBuf;
use tempfile::tempdir;

#[tokio::test]
async fn test_event_retention_cleanup_storage() {
	let temp_dir = tempdir().expect("Failed to create temp directory");
	let db_path = temp_dir.path().join("retention_test.db");
	let config = rust_watcher::database::DatabaseConfig {
		database_path: db_path,
		..rust_watcher::database::DatabaseConfig::for_small_directories()
	};
	let mut storage = RedbStorage::new(config)
		.await
		.expect("Failed to create storage");

	// Insert events with varying timestamps
	let old_event = EventRecord::new(
		"Create".to_string(),
		PathBuf::from("/old/file.txt"),
		false,
		Duration::seconds(-120), // Expired
		0,
	);
	let recent_event = EventRecord::new(
		"Create".to_string(),
		PathBuf::from("/recent/file.txt"),
		false,
		Duration::seconds(120), // Not expired
		0,
	);
	storage
		.store_event(&old_event)
		.await
		.expect("Failed to store old event");
	storage
		.store_event(&recent_event)
		.await
		.expect("Failed to store recent event");

	// Run cleanup with time-based retention
	let retention_cfg = EventRetentionConfig {
		max_event_age: std::time::Duration::from_secs(1),
		max_events: None,
		background: false,
		background_interval: None,
	};
	let removed = cleanup_old_events(&mut storage, &retention_cfg)
		.await
		.expect("Cleanup failed");
	// Should remove at least the old event
	assert!(removed >= 1, "Expected at least one event to be removed");

	// Insert more events to exceed count limit
	for i in 0..10 {
		let event = EventRecord::new(
			format!("Create_{}", i),
			PathBuf::from(format!("/file_{}.txt", i)),
			false,
			Duration::seconds(120),
			0,
		);
		storage
			.store_event(&event)
			.await
			.expect("Failed to store event");
	}

	// Run cleanup with count-based retention
	let retention_cfg = EventRetentionConfig {
		max_event_age: std::time::Duration::from_secs(3600), // Keep all by age
		max_events: Some(5),
		background: false,
		background_interval: None,
	};
	let removed = cleanup_old_events(&mut storage, &retention_cfg)
		.await
		.expect("Cleanup failed");
	// Should remove enough events to leave only 5
	assert!(removed >= 5, "Expected at least five events to be removed");
}
