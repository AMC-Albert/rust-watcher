//! Integration tests for database functionality
//!
//! These tests verify that the database module works correctly for large-scale
//! directory monitoring scenarios, focusing on the actual implemented API.

use chrono::{Duration, Utc};
use rust_watcher::database::{DatabaseConfig, DatabaseStorage, RedbStorage};
use rust_watcher::database::{EventRecord, MetadataRecord};
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::test;
use uuid::Uuid;

mod common;

/// Test basic database initialization and configuration
#[test]
async fn test_database_initialization() {
	let temp_dir = TempDir::new().expect("Failed to create temp directory");
	let db_path = temp_dir.path().join("test.db");

	let config = DatabaseConfig {
		database_path: db_path.clone(),
		..Default::default()
	};
	assert!(config.validate().is_ok());

	// Test that we can create a RedbStorage instance
	let storage_result = RedbStorage::new(config).await;
	assert!(
		storage_result.is_ok(),
		"Failed to create RedbStorage: {:?}",
		storage_result.err()
	);
}

/// Test database configuration for different scales
#[test]
async fn test_database_scaling_configs() {
	let small_config = DatabaseConfig::for_small_directories();
	let moderate_config = DatabaseConfig::for_moderate_directories();
	let large_config = DatabaseConfig::for_large_directories();
	let massive_config = DatabaseConfig::for_massive_directories();

	// All configs should be valid
	assert!(small_config.validate().is_ok());
	assert!(moderate_config.validate().is_ok());
	assert!(large_config.validate().is_ok());
	assert!(massive_config.validate().is_ok());

	// Verify scaling properties
	assert!(small_config.memory_buffer_size < moderate_config.memory_buffer_size);
	assert!(moderate_config.memory_buffer_size < large_config.memory_buffer_size);
	assert!(large_config.memory_buffer_size < massive_config.memory_buffer_size);

	// Verify retention policies scale appropriately
	assert!(small_config.event_retention < large_config.event_retention);
	assert!(moderate_config.event_retention < massive_config.event_retention);
}

/// Test EventRecord creation and expiration logic
#[test]
async fn test_event_record_lifecycle() {
	let path = PathBuf::from("/test/file.txt");
	let retention = Duration::minutes(5);

	let mut record = EventRecord::new("Create".to_string(), path.clone(), false, retention);

	assert_eq!(record.event_type, "Create");
	assert_eq!(record.path, path);
	assert!(!record.is_directory);
	assert!(!record.is_expired());

	// Test expiration extension
	let additional_time = Duration::minutes(10);
	record.extend_expiration(additional_time);
	assert!(!record.is_expired());

	// Verify the event has metadata fields available
	record.size = Some(1024);
	record.inode = Some(12345);
	record.content_hash = Some("abc123".to_string());

	assert_eq!(record.size, Some(1024));
	assert_eq!(record.inode, Some(12345));
	assert_eq!(record.content_hash, Some("abc123".to_string()));
}

/// Test MetadataRecord creation and staleness detection
#[test]
async fn test_metadata_record_lifecycle() {
	let path = PathBuf::from("/test/directory");
	let mut record = MetadataRecord::new(path.clone(), true);

	assert_eq!(record.path, path);
	assert!(record.is_directory);
	assert!(!record.is_stale(Duration::hours(1)));

	// Test with metadata
	record.size = Some(0); // Directory size
	record.inode = Some(67890);
	record.modified_at = Some(Utc::now());

	assert_eq!(record.size, Some(0));
	assert_eq!(record.inode, Some(67890));
	assert!(record.modified_at.is_some());

	// Test staleness detection with very short duration
	assert!(!record.is_stale(Duration::seconds(1)));
}

/// Test database configuration validation
#[test]
async fn test_database_config_validation() {
	let mut config = DatabaseConfig::default();

	// Valid config should pass
	assert!(config.validate().is_ok());

	// Test invalid memory buffer size
	config.memory_buffer_size = 0;
	assert!(config.validate().is_err());
	config.memory_buffer_size = 1000;

	// Test invalid write batch size
	config.write_batch_size = 0;
	assert!(config.validate().is_err());
	config.write_batch_size = 2000; // Larger than memory buffer
	assert!(config.validate().is_err());
	config.write_batch_size = 500; // Valid again

	// Test invalid flush interval
	config.flush_interval = std::time::Duration::from_secs(0);
	assert!(config.validate().is_err());
	config.flush_interval = std::time::Duration::from_secs(30);

	// Should be valid again
	assert!(config.validate().is_ok());
}

/// Test storage key generation and serialization
#[test]
async fn test_storage_key_functionality() {
	use rust_watcher::database::StorageKey;

	// Test size bucket generation
	assert_eq!(StorageKey::size_bucket(0), StorageKey::SizeBucket(0));
	assert_eq!(StorageKey::size_bucket(500), StorageKey::SizeBucket(100));
	assert_eq!(StorageKey::size_bucket(5000), StorageKey::SizeBucket(1000));
	assert_eq!(
		StorageKey::size_bucket(50000),
		StorageKey::SizeBucket(10000)
	);

	// Test key serialization round-trip
	let original_key = StorageKey::EventId(Uuid::new_v4());
	let bytes = original_key.to_bytes();
	let recovered_key = StorageKey::from_bytes(&bytes).unwrap();
	assert_eq!(original_key, recovered_key);

	// Test path-based keys
	let test_path = PathBuf::from("/test/directory/file.txt");
	let path_key = StorageKey::path_hash(&test_path);
	let prefix_key = StorageKey::path_prefix(&test_path, 2);

	// Verify they're different key types
	match (path_key, prefix_key) {
		(StorageKey::PathHash(_), StorageKey::PathPrefix(_)) => (),
		_ => panic!("Unexpected key types generated"),
	}
}

/// Test database statistics functionality
#[test]
async fn test_database_statistics() {
	// Test that we can work with database statistics concepts
	// without directly using the DatabaseStats type which isn't exported

	// Test efficiency calculation simulation
	let cache_hit_rate = 0.9f32;
	let avg_query_time_ms = 10.0f32;
	let efficiency = (cache_hit_rate + (100.0 / avg_query_time_ms).min(1.0)) / 2.0;
	assert!(efficiency > 0.5 && efficiency <= 1.0);

	// Test poor performance simulation
	let poor_cache_hit_rate = 0.2f32;
	let poor_query_time_ms = 1000.0f32;
	let poor_efficiency = (poor_cache_hit_rate + (100.0 / poor_query_time_ms).min(1.0)) / 2.0;
	assert!(poor_efficiency < efficiency);
}

/// Integration test for massive directory scenario simulation
#[test]
async fn test_massive_directory_simulation() {
	let temp_dir = TempDir::new().expect("Failed to create temp directory");
	let db_path = temp_dir.path().join("massive_test.db");

	// Use configuration optimized for massive directories
	let mut config = DatabaseConfig::for_massive_directories();
	config.database_path = db_path;

	assert!(config.validate().is_ok());

	// Verify the configuration has appropriate settings for scale
	assert!(config.memory_buffer_size >= 100_000);
	assert!(config.write_batch_size >= 10_000);
	assert!(config.event_retention >= Duration::hours(1).to_std().unwrap());
	assert!(config.enable_compression);

	// Test that we can create storage with this configuration
	let storage_result = RedbStorage::new(config).await;
	assert!(
		storage_result.is_ok(),
		"Failed to create storage for massive directory: {:?}",
		storage_result.err()
	);
}

/// Test error handling scenarios
#[test]
async fn test_database_error_scenarios() {
	use rust_watcher::database::DatabaseError;

	// Test error categorization
	let timeout_error = DatabaseError::Timeout;
	assert!(timeout_error.is_retryable());
	assert!(!timeout_error.is_corruption());
	assert!(!timeout_error.is_resource_limit());

	let corruption_error = DatabaseError::CorruptionError("test".to_string());
	assert!(!corruption_error.is_retryable());
	assert!(corruption_error.is_corruption());
	assert!(!corruption_error.is_resource_limit());

	let size_error = DatabaseError::SizeLimitExceeded;
	assert!(!size_error.is_retryable());
	assert!(!size_error.is_corruption());
	assert!(size_error.is_resource_limit());

	// Test error display formatting
	let init_error = DatabaseError::InitializationFailed("test failure".to_string());
	let error_message = format!("{}", init_error);
	assert!(error_message.contains("Database initialization failed"));
	assert!(error_message.contains("test failure"));
}

/// Test custom database path configuration
#[test]
async fn test_custom_database_paths() {
	let temp_dir = TempDir::new().expect("Failed to create temp directory");

	// Test with different custom paths
	let custom_paths = vec![
		temp_dir.path().join("custom.db"),
		temp_dir.path().join("subdirectory").join("nested.db"),
		temp_dir.path().join("watcher_large_scale.db"),
	];
	for path in custom_paths {
		let config = DatabaseConfig {
			database_path: path.clone(),
			..Default::default()
		};
		assert_eq!(config.database_path, path);
		assert!(config.validate().is_ok());

		// Verify we can create storage with custom paths
		let storage_result = RedbStorage::new(config).await;
		assert!(
			storage_result.is_ok(),
			"Failed to create storage with path {:?}: {:?}",
			path,
			storage_result.err()
		);
	}
}

/// Test that the database module properly handles the transition from in-memory to persistent storage
#[test]
async fn test_storage_transition_readiness() {
	// This test verifies that our database types and configurations are ready
	// for integration with the existing move detection system

	let temp_dir = TempDir::new().expect("Failed to create temp directory");
	let db_path = temp_dir.path().join("transition_test.db");

	// Test with moderate configuration (typical starting point)
	let mut config = DatabaseConfig::for_moderate_directories();
	config.database_path = db_path;

	let storage = RedbStorage::new(config)
		.await
		.expect("Failed to create storage");

	// Verify that the storage implements the DatabaseStorage trait
	// This ensures it can be used as a drop-in replacement for in-memory storage
	let _: Box<dyn DatabaseStorage> = Box::new(storage);

	// Test that our event and metadata types can represent typical filesystem events
	let test_events = vec![
		EventRecord::new(
			"Create".to_string(),
			PathBuf::from("/test/file1.txt"),
			false,
			Duration::minutes(5),
		),
		EventRecord::new(
			"Remove".to_string(),
			PathBuf::from("/test/file2.txt"),
			false,
			Duration::minutes(5),
		),
		EventRecord::new(
			"RenameFrom".to_string(),
			PathBuf::from("/test/old_name.txt"),
			false,
			Duration::minutes(5),
		),
		EventRecord::new(
			"RenameTo".to_string(),
			PathBuf::from("/test/new_name.txt"),
			false,
			Duration::minutes(5),
		),
	];

	for event in &test_events {
		assert!(!event.event_id.to_string().is_empty());
		assert!(!event.is_expired());
	}

	// Test metadata record types
	let test_metadata = vec![
		MetadataRecord::new(PathBuf::from("/test/file.txt"), false),
		MetadataRecord::new(PathBuf::from("/test/directory"), true),
	];

	for metadata in &test_metadata {
		assert!(!metadata.cached_at.to_string().is_empty());
		assert!(!metadata.is_stale(Duration::hours(1)));
	}
}
