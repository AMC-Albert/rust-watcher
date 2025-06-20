//! Test concurrent database operations
//!
//! Validates that ReDB supports the concurrent access patterns needed
//! for the filesystem cache implementation.

use rust_watcher::{DatabaseAdapter, DatabaseConfig, EventType, FileSystemEvent};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::task::JoinSet;

/// Test concurrent readers don't block each other
#[tokio::test]
async fn test_concurrent_readers() {
	let temp_dir = TempDir::new().expect("Failed to create temp directory");
	let config = DatabaseConfig {
		database_path: temp_dir.path().join("concurrent_readers.db"),
		event_retention: Duration::from_secs(3600),
		..Default::default()
	};

	let adapter = Arc::new(
		DatabaseAdapter::new(config)
			.await
			.expect("Failed to create adapter"),
	);

	// Insert test data
	for i in 0..50 {
		let event = FileSystemEvent::new(
			EventType::Create,
			PathBuf::from(format!("/test/concurrent_{}.txt", i)),
			false,
			Some(1024),
		);
		adapter
			.store_event(&event)
			.await
			.expect("Failed to store event");
	}

	// Spawn multiple concurrent readers
	let mut tasks = JoinSet::new();

	for reader_id in 0..10 {
		let adapter_clone = Arc::clone(&adapter);
		tasks.spawn(async move {
			let test_path = PathBuf::from(format!("/test/concurrent_{}.txt", reader_id % 5));

			// Each reader performs multiple operations
			for _ in 0..20 {
				let events = adapter_clone
					.get_events_for_path(&test_path)
					.await
					.expect("Failed to get events");
				assert!(!events.is_empty(), "Should find events for path");

				let stats = adapter_clone
					.get_stats()
					.await
					.expect("Failed to get stats");
				assert!(stats.total_events >= 50, "Should have at least 50 events");

				// Small delay to interleave operations
				tokio::time::sleep(Duration::from_millis(1)).await;
			}

			reader_id
		});
	}

	// Wait for all readers to complete
	let mut completed_readers = Vec::new();
	while let Some(result) = tasks.join_next().await {
		let reader_id = result.expect("Reader task failed");
		completed_readers.push(reader_id);
	}

	assert_eq!(completed_readers.len(), 10, "All readers should complete");
}

/// Test reader-writer concurrency
#[tokio::test]
async fn test_reader_writer_concurrency() {
	let temp_dir = TempDir::new().expect("Failed to create temp directory");
	let config = DatabaseConfig {
		database_path: temp_dir.path().join("reader_writer.db"),
		event_retention: Duration::from_secs(3600),
		..Default::default()
	};

	let adapter = Arc::new(
		DatabaseAdapter::new(config)
			.await
			.expect("Failed to create adapter"),
	);

	let mut tasks = JoinSet::new();

	// Spawn writers
	for writer_id in 0..5 {
		let adapter_clone = Arc::clone(&adapter);
		tasks.spawn(async move {
			for i in 0..20 {
				let event = FileSystemEvent::new(
					EventType::Create,
					PathBuf::from(format!("/test/writer_{}_{}.txt", writer_id, i)),
					false,
					Some(1024),
				);

				adapter_clone
					.store_event(&event)
					.await
					.expect("Failed to store event");

				// Small delay between writes
				tokio::time::sleep(Duration::from_millis(2)).await;
			}
			format!("writer_{}", writer_id)
		});
	}

	// Spawn readers
	for reader_id in 0..5 {
		let adapter_clone = Arc::clone(&adapter);
		tasks.spawn(async move {
			for _ in 0..30 {
				let _stats = adapter_clone
					.get_stats()
					.await
					.expect("Failed to get stats");

				// Health check during concurrent operations
				let health = adapter_clone
					.health_check()
					.await
					.expect("Health check failed");
				assert!(
					health,
					"Database should remain healthy during concurrent access"
				);

				tokio::time::sleep(Duration::from_millis(3)).await;
			}
			format!("reader_{}", reader_id)
		});
	}

	// Wait for all tasks to complete
	let mut completed_tasks = Vec::new();
	while let Some(result) = tasks.join_next().await {
		let task_id = result.expect("Task failed");
		completed_tasks.push(task_id);
	}

	assert_eq!(completed_tasks.len(), 10, "All tasks should complete");

	// Verify final state
	let final_stats = adapter
		.get_stats()
		.await
		.expect("Failed to get final stats");
	assert_eq!(
		final_stats.total_events, 100,
		"Should have 100 events total"
	);
}

/// Test concurrent writes don't corrupt data
#[tokio::test]
async fn test_concurrent_writers() {
	let temp_dir = TempDir::new().expect("Failed to create temp directory");
	let config = DatabaseConfig {
		database_path: temp_dir.path().join("concurrent_writers.db"),
		event_retention: Duration::from_secs(3600),
		..Default::default()
	};

	let adapter = Arc::new(
		DatabaseAdapter::new(config)
			.await
			.expect("Failed to create adapter"),
	);

	let mut tasks = JoinSet::new();

	// Spawn multiple writers writing to different paths
	for writer_id in 0..8 {
		let adapter_clone = Arc::clone(&adapter);
		tasks.spawn(async move {
			let mut event_count = 0;

			for i in 0..25 {
				let event = FileSystemEvent::new(
					EventType::Create,
					PathBuf::from(format!("/test/writer_{}_{}.txt", writer_id, i)),
					false,
					Some(1024 + i as u64),
				);

				adapter_clone
					.store_event(&event)
					.await
					.expect("Failed to store event");
				event_count += 1;

				// Vary timing to create more realistic concurrent access
				if i % 3 == 0 {
					tokio::time::sleep(Duration::from_millis(1)).await;
				}
			}

			(writer_id, event_count)
		});
	}

	// Collect results from all writers
	let mut total_written = 0;
	let mut completed_writers = 0;

	while let Some(result) = tasks.join_next().await {
		let (writer_id, event_count) = result.expect("Writer task failed");
		assert_eq!(
			event_count, 25,
			"Writer {} should have written 25 events",
			writer_id
		);
		total_written += event_count;
		completed_writers += 1;
	}

	assert_eq!(completed_writers, 8, "All writers should complete");
	assert_eq!(total_written, 200, "Should have written 200 events total");

	// Verify data integrity
	let final_stats = adapter
		.get_stats()
		.await
		.expect("Failed to get final stats");
	assert_eq!(
		final_stats.total_events, 200,
		"Database should contain all written events"
	);

	// Verify we can read back specific events
	for writer_id in 0..8 {
		for i in 0..25 {
			let path = PathBuf::from(format!("/test/writer_{}_{}.txt", writer_id, i));
			let events = adapter
				.get_events_for_path(&path)
				.await
				.expect("Failed to get events for path");
			assert_eq!(
				events.len(),
				1,
				"Should find exactly one event for each path"
			);
			assert_eq!(
				events[0].size,
				Some(1024 + i as u64),
				"Event size should match"
			);
		}
	}
}

/// Test database recovery after unclean shutdown simulation
#[tokio::test]
async fn test_database_recovery() {
	let temp_dir = TempDir::new().expect("Failed to create temp directory");
	let db_path = temp_dir.path().join("recovery_test.db");

	let config = DatabaseConfig {
		database_path: db_path.clone(),
		event_retention: Duration::from_secs(3600),
		..Default::default()
	};

	// Phase 1: Write some data
	{
		let adapter = DatabaseAdapter::new(config.clone())
			.await
			.expect("Failed to create adapter");

		for i in 0..50 {
			let event = FileSystemEvent::new(
				EventType::Create,
				PathBuf::from(format!("/test/recovery_{}.txt", i)),
				false,
				Some(1024),
			);
			adapter
				.store_event(&event)
				.await
				.expect("Failed to store event");
		}

		let stats = adapter.get_stats().await.expect("Failed to get stats");
		assert_eq!(
			stats.total_events, 50,
			"Should have 50 events before shutdown"
		);

		// Adapter drops here, simulating unclean shutdown
	}

	// Phase 2: Reopen database and verify recovery
	{
		let adapter = DatabaseAdapter::new(config)
			.await
			.expect("Failed to reopen database after simulated shutdown");

		let stats = adapter
			.get_stats()
			.await
			.expect("Failed to get stats after recovery");
		assert_eq!(
			stats.total_events, 50,
			"Should recover all 50 events after restart"
		);

		// Verify data integrity by reading specific events
		for i in 0..50 {
			let path = PathBuf::from(format!("/test/recovery_{}.txt", i));
			let events = adapter
				.get_events_for_path(&path)
				.await
				.expect("Failed to get events after recovery");
			assert_eq!(events.len(), 1, "Should find event {} after recovery", i);
		}

		// Verify we can continue writing
		let new_event = FileSystemEvent::new(
			EventType::Create,
			PathBuf::from("/test/post_recovery.txt"),
			false,
			Some(2048),
		);
		adapter
			.store_event(&new_event)
			.await
			.expect("Should be able to write after recovery");

		let final_stats = adapter
			.get_stats()
			.await
			.expect("Failed to get final stats");
		assert_eq!(
			final_stats.total_events, 51,
			"Should have 51 events after post-recovery write"
		);
	}
}
