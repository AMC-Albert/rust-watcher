//! Common test utilities for the rust-watcher library

#![allow(unused_imports, dead_code)]

use rust_watcher::database::{
	DatabaseConfig, DatabaseStorage, EventRecord, MetadataRecord, RedbStorage,
};
use std::path::PathBuf;
use tempfile::TempDir;

/// Create a temporary directory for testing
pub fn setup_temp_dir() -> TempDir {
	TempDir::new().expect("Failed to create temp directory")
}

/// Create a test file with content
pub fn create_test_file(path: &std::path::Path, content: &str) -> std::io::Result<()> {
	std::fs::write(path, content)
}

/// Create multiple test files for testing
#[allow(dead_code)]
pub fn create_test_files(dir: &std::path::Path, count: usize) -> std::io::Result<Vec<PathBuf>> {
	let mut files = Vec::new();
	for i in 0..count {
		let file_path = dir.join(format!("test_file_{i}.txt"));
		create_test_file(&file_path, &format!("Content for file {i}"))?;
		files.push(file_path);
	}
	Ok(files)
}

/// Wait for a short duration to allow file system events to propagate
#[allow(dead_code)]
pub async fn wait_for_events() {
	tokio::time::sleep(std::time::Duration::from_millis(100)).await;
}

/// Create a short timeout for testing
#[allow(dead_code)]
pub async fn timeout_short() {
	tokio::time::sleep(std::time::Duration::from_millis(50)).await;
}

/// Create a longer timeout for testing
#[allow(dead_code)]
pub async fn timeout_long() {
	tokio::time::sleep(std::time::Duration::from_millis(500)).await;
}

/// Create multiple test filesystem events for testing
#[allow(dead_code)]
pub fn create_test_events(count: usize) -> Vec<rust_watcher::FileSystemEvent> {
	let mut events = Vec::new();
	for i in 0..count {
		let event = rust_watcher::FileSystemEvent {
			id: uuid::Uuid::new_v4(),
			event_type: if i % 2 == 0 {
				rust_watcher::EventType::Create
			} else {
				rust_watcher::EventType::Remove
			},
			path: PathBuf::from(format!("/test/file_{i}.txt")),
			is_directory: false,
			size: Some((i % 100) as u64 * 1024),
			timestamp: chrono::Utc::now(),
			move_data: None,
		};
		events.push(event);
	}
	events
}

/// Database testing utilities for large-scale directory scenarios
pub mod database {
	use rust_watcher::database::{DatabaseConfig, EventRecord, MetadataRecord, RedbStorage};
	use std::path::PathBuf;
	use tempfile::TempDir;
	/// Create a temporary database for testing
	pub async fn create_test_database() -> Result<(TempDir, RedbStorage), Box<dyn std::error::Error>>
	{
		let temp_dir = TempDir::new()?;
		let db_path = temp_dir.path().join("test.db");

		let config = DatabaseConfig::with_path(db_path);
		let storage = RedbStorage::new(config).await?;

		Ok((temp_dir, storage))
	}

	/// Create a test event record with specified parameters
	pub fn create_test_event(
		event_type: &str, path: PathBuf, is_directory: bool, size: Option<u64>, inode: Option<u64>,
		windows_id: Option<u64>,
	) -> EventRecord {
		// Use sequence_number=0 for test events unless a specific value is required by the test logic.
		let mut record = EventRecord::new(
			event_type.to_string(),
			path,
			is_directory,
			chrono::Duration::minutes(10),
			0,
		);
		record.size = size;
		record.inode = inode;
		record.windows_id = windows_id;
		record
	}

	/// Create a test metadata record
	pub fn create_test_metadata(
		path: PathBuf, is_directory: bool, size: Option<u64>, inode: Option<u64>,
	) -> MetadataRecord {
		let mut metadata = MetadataRecord::new(path, is_directory);
		metadata.size = size;
		metadata.inode = inode;
		metadata
	}

	/// Generate a large number of test events for performance testing
	pub fn generate_test_events(count: usize, base_path: &str) -> Vec<EventRecord> {
		(0..count)
			.map(|i| {
				create_test_event(
					"Create",
					PathBuf::from(format!("{base_path}/file_{i}.txt")),
					false,
					Some((i as u64) * 1024), // Varying file sizes
					Some(i as u64 + 10000),  // Sequential inodes
					None,
				)
			})
			.collect()
	}
	/// Helper to measure database operation performance
	pub async fn measure_database_operation<F, T>(
		operation: F,
	) -> Result<(T, std::time::Duration), Box<dyn std::error::Error>>
	where F: std::future::Future<Output = Result<T, Box<dyn std::error::Error>>> {
		let start = std::time::Instant::now();
		let result = operation.await?;
		let duration = start.elapsed();
		Ok((result, duration))
	}
}
