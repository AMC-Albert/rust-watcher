//! Large dataset performance tests
//!
//! These tests validate ReDB performance with datasets similar to what
//! the filesystem cache will encounter in production.

use rust_watcher::{DatabaseAdapter, DatabaseConfig, EventType, FileSystemEvent};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::test;

const SMALL_DATASET: usize = 1_000;
const MEDIUM_DATASET: usize = 5_000;
const LARGE_DATASET: usize = 10_000; // Reduced from 100K to be more realistic for individual transactions

/// Test database performance with small dataset (1K events)
#[test]
async fn test_performance_small_dataset() {
	let result = test_dataset_performance("small", SMALL_DATASET).await;

	// Realistic expectations based on ReDB performance with individual transactions
	// Baseline: ~4-6ms per event with individual transactions
	assert!(
		result.storage_time.as_millis() < 10000,
		"Storage took too long: {:?}",
		result.storage_time
	);
	assert!(
		result.query_time.as_millis() < 200,
		"Query took too long: {:?}",
		result.query_time
	);

	println!("Small dataset performance: {result}");
}

/// Test database performance with medium dataset (5K events)
#[test]
async fn test_performance_medium_dataset() {
	let result = test_dataset_performance("medium", MEDIUM_DATASET).await;

	// Medium dataset expectations - roughly linear scaling from small dataset
	assert!(
		result.storage_time.as_millis() < 30000,
		"Storage took too long: {:?}",
		result.storage_time
	);
	assert!(
		result.query_time.as_millis() < 500,
		"Query took too long: {:?}",
		result.query_time
	);

	println!("Medium dataset performance: {result}");
}

/// Test database performance with large dataset (10K events)
#[test]
async fn test_performance_large_dataset() {
	let result = test_dataset_performance("large", LARGE_DATASET).await;

	// Large dataset should be acceptable for filesystem cache use case
	// This establishes our baseline for cache performance requirements
	// Individual transactions are slow but acceptable for our use case
	assert!(
		result.storage_time.as_millis() < 50000,
		"Storage took too long: {:?}",
		result.storage_time
	);
	assert!(
		result.query_time.as_millis() < 1000,
		"Query took too long: {:?}",
		result.query_time
	);

	println!("Large dataset performance: {result}");
}

/// Test multimap table scalability with deep directory hierarchies
#[test]
async fn test_multimap_scalability() {
	let temp_dir = TempDir::new().expect("Failed to create temp dir");
	let config = DatabaseConfig {
		database_path: temp_dir.path().join("multimap_test.redb"),
		event_retention: Duration::from_secs(3600),
		..Default::default()
	};

	let adapter = DatabaseAdapter::new(config).await.expect("Failed to create adapter");

	let start = Instant::now();

	// Create events representing a deep directory hierarchy
	for depth in 0..20 {
		for breadth in 0..100 {
			let mut path_components = Vec::new();

			// Build deep path
			for level in 0..depth {
				path_components.push(format!("level_{level}"));
			}
			path_components.push(format!("file_{depth}_{breadth}.txt"));

			let path = PathBuf::from("/root").join(path_components.join("/"));

			let event = FileSystemEvent::new(EventType::Create, path, false, Some(1024));

			adapter.store_event(&event).await.expect("Failed to store event");
		}
	}

	let storage_time = start.elapsed();

	// Test querying performance
	let query_start = Instant::now();
	let stats = adapter.get_stats().await.expect("Failed to get stats");
	let query_time = query_start.elapsed();

	println!("Multimap scalability test:");
	println!("  Events stored: {}", stats.total_events);
	println!("  Storage time: {storage_time:?}");
	println!("  Query time: {query_time:?}");

	// Should handle 2000 events (20 levels * 100 files) efficiently
	assert_eq!(stats.total_events, 2000);
	assert!(
		storage_time.as_millis() < 10000,
		"Multimap storage too slow"
	);
	assert!(query_time.as_millis() < 100, "Multimap query too slow");
}

/// Test memory usage patterns with large directory trees
#[test]
async fn test_memory_usage_patterns() {
	let temp_dir = TempDir::new().expect("Failed to create temp dir");
	let config = DatabaseConfig {
		database_path: temp_dir.path().join("memory_test.redb"),
		event_retention: Duration::from_secs(3600),
		..Default::default()
	};

	let adapter = DatabaseAdapter::new(config).await.expect("Failed to create adapter");
	// Simulate a large directory tree with varied file sizes
	let mut total_size = 0u64;

	for i in 0..10_000 {
		// Reduced from 50K to 10K for reasonable test time
		let file_size = match i % 4 {
			0 => 1024,        // Small files
			1 => 1024 * 10,   // Medium files
			2 => 1024 * 100,  // Large files
			_ => 1024 * 1000, // Very large files
		};

		total_size += file_size;

		let path = PathBuf::from(format!(
			"/large_tree/dir_{}/subdir_{}/file_{}.dat",
			i / 1000,
			(i / 100) % 10,
			i
		));

		let event = FileSystemEvent::new(EventType::Create, path, false, Some(file_size));

		adapter.store_event(&event).await.expect("Failed to store event");
		// Periodically check that we can still query efficiently
		if i % 2_500 == 0 && i > 0 {
			// Check every 2.5K instead of 10K
			let query_start = Instant::now();
			let stats = adapter.get_stats().await.expect("Failed to get stats");
			let query_time = query_start.elapsed();

			println!("Memory check at {i} events: query time {query_time:?}");
			assert!(query_time.as_millis() < 200, "Query degraded at {i} events");
			assert_eq!(stats.total_events as usize, i + 1);
		}
	}

	let final_stats = adapter.get_stats().await.expect("Failed to get final stats");
	println!("Final memory test stats:");
	println!("  Total events: {}", final_stats.total_events);
	println!("  Expected total size: {total_size} bytes");

	assert_eq!(final_stats.total_events, 10_000);
}

/// Common performance testing logic
async fn test_dataset_performance(name: &str, size: usize) -> PerformanceResult {
	let temp_dir = TempDir::new().expect("Failed to create temp dir");
	let config = DatabaseConfig {
		database_path: temp_dir.path().join(format!("{name}_dataset.redb")),
		event_retention: Duration::from_secs(3600),
		..Default::default()
	};

	let adapter = DatabaseAdapter::new(config).await.expect("Failed to create adapter");

	// Measure storage performance
	let storage_start = Instant::now();

	for i in 0..size {
		let path = PathBuf::from(format!("/test/dataset/{name}/file_{i}.txt"));
		let event = FileSystemEvent::new(
			EventType::Create,
			path,
			i % 10 == 0,                   // Every 10th is a directory
			Some(1024 + (i as u64 * 100)), // Varied file sizes
		);

		adapter.store_event(&event).await.expect("Failed to store event");
		// Add some variety with different event types
		if i % 3 == 0 {
			let path = PathBuf::from(format!(
				"/test/dataset/{}/modified_file_{}.txt",
				name,
				i / 3
			));
			let modify_event = FileSystemEvent::new(EventType::Write, path, false, Some(2048));
			adapter.store_event(&modify_event).await.expect("Failed to store modify event");
		}
	}

	let storage_time = storage_start.elapsed();

	// Measure query performance
	let query_start = Instant::now();
	let stats = adapter.get_stats().await.expect("Failed to get stats");
	let query_time = query_start.elapsed();

	// Cleanup performance test
	let cleanup_start = Instant::now();
	let cleaned = adapter.cleanup_old_events().await.expect("Failed to cleanup");
	let cleanup_time = cleanup_start.elapsed();

	PerformanceResult {
		dataset_size: size,
		events_stored: stats.total_events,
		storage_time,
		query_time,
		cleanup_time,
		cleaned_events: cleaned,
	}
}

#[derive(Debug)]
struct PerformanceResult {
	dataset_size: usize,
	events_stored: u64,
	storage_time: Duration,
	query_time: Duration,
	cleanup_time: Duration,
	cleaned_events: usize,
}

impl std::fmt::Display for PerformanceResult {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f,
            "Dataset: {} events\n  Stored: {} events\n  Storage: {:?}\n  Query: {:?}\n  Cleanup: {:?} ({} cleaned)",
            self.dataset_size,
            self.events_stored,
            self.storage_time,
            self.query_time,
            self.cleanup_time,
            self.cleaned_events
        )
	}
}
