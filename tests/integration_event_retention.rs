//! Integration test for event retention and cleanup logic at the storage layer
//!
//! This test validates that the retention/cleanup system correctly removes old events
//! according to both time-based and count-based policies. It also checks edge cases and
//! verifies that the API is exposed through the storage trait.

use rust_watcher::database::storage::core::{DatabaseStorage, RedbStorage};
use rust_watcher::database::storage::event_retention::{cleanup_old_events, EventRetentionConfig};
use rust_watcher::database::types::EventRecord;
use std::path::PathBuf;
use tempfile::tempdir;

#[tokio::test]
async fn test_event_retention_cleanup_storage() {
	let temp_dir = tempdir().expect("Failed to create temp directory");
	let db_path = temp_dir.path().join(format!("retention_test-{}.redb", uuid::Uuid::new_v4()));
	let config = rust_watcher::database::DatabaseConfig {
		database_path: db_path,
		..rust_watcher::database::DatabaseConfig::for_small_directories()
	};
	let mut storage = RedbStorage::new(config).await.expect("Failed to create storage");

	use chrono::Utc;
	let now = Utc::now();

	// Insert events with explicit timestamps
	let old_event = EventRecord {
		event_id: uuid::Uuid::new_v4(),
		sequence_number: 0,
		event_type: "Create".to_string(),
		path: PathBuf::from("/old/file.txt"),
		timestamp: now - chrono::Duration::seconds(120), // 2 minutes ago
		is_directory: false,
		size: None,
		inode: None,
		windows_id: None,
		content_hash: None,
		confidence: None,
		detection_method: None,
		expires_at: now - chrono::Duration::seconds(60), // already expired
	};
	let recent_event = EventRecord {
		event_id: uuid::Uuid::new_v4(),
		sequence_number: 0,
		event_type: "Create".to_string(),
		path: PathBuf::from("/recent/file.txt"),
		timestamp: now,
		is_directory: false,
		size: None,
		inode: None,
		windows_id: None,
		content_hash: None,
		confidence: None,
		detection_method: None,
		expires_at: now + chrono::Duration::seconds(120), // not expired
	};
	storage.store_event(&old_event).await.expect("Failed to store old event");
	storage.store_event(&recent_event).await.expect("Failed to store recent event");

	// Run cleanup with time-based retention
	let retention_cfg = EventRetentionConfig {
		max_event_age: std::time::Duration::from_secs(61), // 61 seconds
		max_events: None,
		background: false,
		background_interval: None,
	};
	let removed = cleanup_old_events(&mut storage, &retention_cfg).await.expect("Cleanup failed");
	// Should remove at least the old event
	assert!(removed >= 1, "Expected at least one event to be removed");

	// Insert more events to exceed count limit
	for i in 0..10 {
		let event = EventRecord {
			event_id: uuid::Uuid::new_v4(),
			sequence_number: 0,
			event_type: format!("Create_{i}"),
			path: PathBuf::from(format!("/file_{i}.txt")),
			timestamp: now,
			is_directory: false,
			size: None,
			inode: None,
			windows_id: None,
			content_hash: None,
			confidence: None,
			detection_method: None,
			expires_at: now + chrono::Duration::seconds(120),
		};
		storage.store_event(&event).await.expect("Failed to store event");
	}

	// Run cleanup with count-based retention
	let retention_cfg = EventRetentionConfig {
		max_event_age: std::time::Duration::from_secs(3600), // Keep all by age
		max_events: Some(5),
		background: false,
		background_interval: None,
	};
	let removed = cleanup_old_events(&mut storage, &retention_cfg).await.expect("Cleanup failed");
	// Should remove enough events to leave only 5
	assert!(removed >= 5, "Expected at least five events to be removed");
}

#[tokio::test]
async fn test_event_retention_duplicates_and_out_of_order() {
	let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
	let db_path = temp_dir.path().join(format!(
		"retention_test_dupes-{}.redb",
		uuid::Uuid::new_v4()
	));
	let config = rust_watcher::database::DatabaseConfig {
		database_path: db_path,
		..rust_watcher::database::DatabaseConfig::for_small_directories()
	};
	let mut storage = RedbStorage::new(config).await.expect("Failed to create storage");

	use chrono::Utc;
	let now = Utc::now();

	// Insert duplicate events (same path/type/timestamp)
	let event1 = EventRecord {
		event_id: uuid::Uuid::new_v4(),
		sequence_number: 0,
		event_type: "Modify".to_string(),
		path: PathBuf::from("/dupe/file.txt"),
		timestamp: now - chrono::Duration::seconds(3600), // 1 hour ago
		is_directory: false,
		size: None,
		inode: None,
		windows_id: None,
		content_hash: None,
		confidence: None,
		detection_method: None,
		expires_at: now - chrono::Duration::seconds(1800), // expired
	};
	let event2 = event1.clone();
	storage.store_event(&event1).await.expect("Failed to store event1");
	storage.store_event(&event2).await.expect("Failed to store event2");

	// Insert out-of-order events (future and past)
	let future_event = EventRecord {
		event_id: uuid::Uuid::new_v4(),
		sequence_number: 0,
		event_type: "Modify".to_string(),
		path: PathBuf::from("/future/file.txt"),
		timestamp: now + chrono::Duration::seconds(3600), // 1 hour in the future
		is_directory: false,
		size: None,
		inode: None,
		windows_id: None,
		content_hash: None,
		confidence: None,
		detection_method: None,
		expires_at: now + chrono::Duration::seconds(7200), // not expired
	};
	let past_event = EventRecord {
		event_id: uuid::Uuid::new_v4(),
		sequence_number: 0,
		event_type: "Modify".to_string(),
		path: PathBuf::from("/past/file.txt"),
		timestamp: now - chrono::Duration::seconds(7200), // 2 hours ago
		is_directory: false,
		size: None,
		inode: None,
		windows_id: None,
		content_hash: None,
		confidence: None,
		detection_method: None,
		expires_at: now - chrono::Duration::seconds(3600), // expired
	};
	storage.store_event(&future_event).await.expect("Failed to store future_event");
	storage.store_event(&past_event).await.expect("Failed to store past_event");

	// Run cleanup with a retention window that should only keep the future event
	let retention_cfg = EventRetentionConfig {
		max_event_age: std::time::Duration::from_secs(1800), // 30 min
		max_events: None,
		background: false,
		background_interval: None,
	};
	let _ = cleanup_old_events(&mut storage, &retention_cfg).await.expect("Cleanup failed");

	let remaining = storage.count_events().await.expect("Count failed");
	// Only the future event should remain
	assert_eq!(
		remaining, 1,
		"Expected only the future event to remain, found {remaining}"
	);
}
