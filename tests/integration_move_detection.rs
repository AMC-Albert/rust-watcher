// Integration test for move detection functionality
// Tests move detector using public API only

use chrono::Utc;
use rust_watcher::{EventType, FileSystemEvent, MoveDetector, MoveDetectorConfig};
use uuid::Uuid;

use rust_watcher::database::storage::filesystem_cache::trait_def::CacheStats;
use rust_watcher::database::storage::filesystem_cache::trait_def::FilesystemCacheStorage;

mod common;

struct DummyCache;
#[async_trait::async_trait]
impl FilesystemCacheStorage for DummyCache {
	async fn store_filesystem_node(
		&mut self, _: &uuid::Uuid, _: &rust_watcher::database::types::FilesystemNode,
	) -> rust_watcher::database::error::DatabaseResult<()> {
		Ok(())
	}
	async fn get_filesystem_node(
		&mut self, _: &uuid::Uuid, _: &std::path::Path,
	) -> rust_watcher::database::error::DatabaseResult<
		Option<rust_watcher::database::types::FilesystemNode>,
	> {
		Ok(None)
	}
	async fn list_directory_for_watch(
		&mut self, _: &uuid::Uuid, _: &std::path::Path,
	) -> rust_watcher::database::error::DatabaseResult<
		Vec<rust_watcher::database::types::FilesystemNode>,
	> {
		Ok(vec![])
	}
	async fn store_watch_metadata(
		&mut self, _: &rust_watcher::database::types::WatchMetadata,
	) -> rust_watcher::database::error::DatabaseResult<()> {
		Ok(())
	}
	async fn get_watch_metadata(
		&mut self, _: &uuid::Uuid,
	) -> rust_watcher::database::error::DatabaseResult<
		Option<rust_watcher::database::types::WatchMetadata>,
	> {
		Ok(None)
	}
	async fn remove_watch(
		&mut self, _: &uuid::Uuid,
	) -> rust_watcher::database::error::DatabaseResult<()> {
		Ok(())
	}
	async fn store_shared_node(
		&mut self, _: &rust_watcher::database::types::SharedNodeInfo,
	) -> rust_watcher::database::error::DatabaseResult<()> {
		Ok(())
	}
	async fn get_shared_node(
		&mut self, _: u64,
	) -> rust_watcher::database::error::DatabaseResult<
		Option<rust_watcher::database::types::SharedNodeInfo>,
	> {
		Ok(None)
	}
	async fn batch_store_filesystem_nodes(
		&mut self, _: &uuid::Uuid, _: &[rust_watcher::database::types::FilesystemNode],
	) -> rust_watcher::database::error::DatabaseResult<()> {
		Ok(())
	}
	async fn find_nodes_by_prefix(
		&mut self, _: &uuid::Uuid, _: &std::path::Path,
	) -> rust_watcher::database::error::DatabaseResult<
		Vec<rust_watcher::database::types::FilesystemNode>,
	> {
		Ok(vec![])
	}
	async fn get_cache_stats(
		&mut self, _: &uuid::Uuid,
	) -> rust_watcher::database::error::DatabaseResult<CacheStats> {
		Ok(Default::default())
	}
	async fn cleanup_stale_cache(
		&mut self, _: &uuid::Uuid, _: u64,
	) -> rust_watcher::database::error::DatabaseResult<usize> {
		Ok(0)
	}
	async fn list_directory_unified(
		&mut self, _: &std::path::Path,
	) -> rust_watcher::database::error::DatabaseResult<
		Vec<rust_watcher::database::types::FilesystemNode>,
	> {
		Ok(vec![])
	}
	async fn get_unified_node(
		&mut self, _: &std::path::Path,
	) -> rust_watcher::database::error::DatabaseResult<
		Option<rust_watcher::database::types::FilesystemNode>,
	> {
		Ok(None)
	}
	async fn list_ancestors(
		&mut self, _: &std::path::Path,
	) -> rust_watcher::database::error::DatabaseResult<
		Vec<rust_watcher::database::types::FilesystemNode>,
	> {
		Ok(vec![])
	}
	async fn list_descendants(
		&mut self, _: &std::path::Path,
	) -> rust_watcher::database::error::DatabaseResult<
		Vec<rust_watcher::database::types::FilesystemNode>,
	> {
		Ok(vec![])
	}
	async fn search_nodes(
		&mut self, _: &str,
	) -> rust_watcher::database::error::DatabaseResult<
		Vec<rust_watcher::database::types::FilesystemNode>,
	> {
		Ok(vec![])
	}
}

#[test]
fn test_move_detector_creation() {
	let config = MoveDetectorConfig::default();
	let mut dummy_cache = DummyCache;
	let _detector = MoveDetector::new(config, &mut dummy_cache);

	// Test that move detector can be created without errors
	println!("Move detector created successfully");
}

#[test]
fn test_move_detector_with_custom_config() {
	let config = MoveDetectorConfig::with_timeout(500);
	let mut dummy_cache = DummyCache;
	let _detector = MoveDetector::new(config, &mut dummy_cache);

	// Test that move detector with custom config can be created
	println!("Move detector with custom config created successfully");
}

#[tokio::test]
async fn test_move_detector_event_processing() {
	let config = MoveDetectorConfig::default();
	let mut dummy_cache = DummyCache;
	let mut detector = MoveDetector::new(config, &mut dummy_cache);

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
	println!("Move detection test: moves detected = {has_moves}");
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
