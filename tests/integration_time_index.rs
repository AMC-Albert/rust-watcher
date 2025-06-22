//! Integration tests for time index repair and time-based event cleanup

use redb::ReadableMultimapTable;
use rust_watcher::database::adapter::DatabaseAdapter;
use rust_watcher::database::config::DatabaseConfig;
use rust_watcher::{EventType, FileSystemEvent};
use std::time::Duration;
use tempfile::TempDir;

#[tokio::test]
async fn test_time_index_repair() {
	let temp_dir = TempDir::new().expect("Failed to create temp directory");
	let db_path = temp_dir.path().join("repair_test.redb");
	// Use a very short retention for the test (1 second)
	let config = DatabaseConfig {
		database_path: db_path.clone(),
		event_retention: Duration::from_secs(1),
		..Default::default()
	};
	let adapter = DatabaseAdapter::new(config).await.expect("Failed to create adapter");

	// Insert events
	for i in 0..10 {
		let event = FileSystemEvent::new(
			EventType::Create,
			temp_dir.path().join(format!("file_{i}.txt")),
			false,
			Some(123),
		);
		adapter.store_event(&event).await.expect("Failed to store event");
	}

	// Simulate index corruption: manually clear the time index
	{
		let db = adapter.get_raw_database().await.expect("No raw db");
		let write_txn = db.begin_write().unwrap();
		let mut time_index = write_txn
			.open_multimap_table(rust_watcher::database::storage::tables::TIME_INDEX_TABLE)
			.unwrap();
		let mut to_remove = Vec::new();
		for entry in time_index.iter().unwrap() {
			let (bucket_guard, multimap_value) = entry.unwrap();
			let bucket_key = bucket_guard.value().to_vec();
			for value_guard in multimap_value.flatten() {
				let value = value_guard.value().to_vec();
				to_remove.push((bucket_key.clone(), value));
			}
		}
		for (bucket_key, value) in to_remove {
			time_index.remove(bucket_key.as_slice(), value.as_slice()).unwrap();
		}
		drop(time_index);
		write_txn.commit().unwrap();
	}

	// Wait for events to expire
	tokio::time::sleep(std::time::Duration::from_secs(2)).await;

	// Run repair
	let db = adapter.get_raw_database().await.expect("No raw db");
	rust_watcher::database::storage::maintenance::repair_time_index(&db)
		.await
		.expect("Repair failed");

	// Run cleanup and assert expired events are removed
	let db = adapter.get_raw_database().await.expect("No raw db");
	let cleaned = rust_watcher::database::storage::maintenance::cleanup_expired_events(
		&db,
		std::time::SystemTime::now(),
	)
	.await
	.expect("Cleanup failed");
	assert!(
		cleaned > 0,
		"Should have cleaned up some events after repair"
	);
}
