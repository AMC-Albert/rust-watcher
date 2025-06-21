//! Stress tests for concurrent cache access patterns
//!
//! This file provides a framework for stress testing the filesystem cache under concurrent access scenarios.
//!
//! All tests are currently stubs and should be implemented as the cache and multi-watch APIs are developed.

use chrono::Utc;
use rust_watcher::database::storage::core::{DatabaseStorage, RedbStorage};
use rust_watcher::database::types::{
	CacheInfo, ComputedProperties, FilesystemNode, NodeMetadata, NodeType,
};
use std::sync::Arc;
use std::time::SystemTime;
use tempfile::tempdir;
use uuid::Uuid;

fn make_test_node(path: &str) -> FilesystemNode {
	FilesystemNode {
		path: path.into(),
		node_type: NodeType::File { size: 42, content_hash: None, mime_type: None },
		metadata: NodeMetadata {
			modified_time: SystemTime::now(),
			created_time: Some(SystemTime::now()),
			accessed_time: Some(SystemTime::now()),
			permissions: 0o644,
			inode: None,
			windows_id: None,
		},
		cache_info: CacheInfo {
			cached_at: Utc::now(),
			last_verified: Utc::now(),
			cache_version: 1,
			needs_refresh: false,
		},
		computed: ComputedProperties {
			depth_from_root: 1,
			path_hash: 0,
			parent_hash: None,
			canonical_name: "test".to_string(),
		},
	}
}

#[tokio::test]
async fn stress_concurrent_cache_reads() {
	let temp_dir = tempdir().expect("Failed to create temp dir");
	let db_path = temp_dir.path().join("stress_reads.redb");
	let config = rust_watcher::database::config::DatabaseConfig {
		database_path: db_path,
		..Default::default()
	};
	let storage = Arc::new(tokio::sync::Mutex::new(
		RedbStorage::new(config).await.unwrap(),
	));
	let watch_id = Uuid::new_v4();
	let node = make_test_node("/read/file.txt");
	storage
		.lock()
		.await
		.store_filesystem_node(&watch_id, &node, "test")
		.await
		.unwrap();

	let mut handles = vec![];
	for _ in 0..16 {
		let storage = storage.clone();
		let path = node.path.clone();
		handles.push(tokio::spawn(async move {
			for _ in 0..100 {
				let _ = storage.lock().await.get_filesystem_node(&watch_id, &path).await.unwrap();
			}
		}));
	}
	for h in handles {
		h.await.unwrap();
	}
}

#[tokio::test]
async fn stress_concurrent_cache_writes() {
	let temp_dir = tempdir().expect("Failed to create temp dir");
	let db_path = temp_dir.path().join("stress_writes.redb");
	let config = rust_watcher::database::config::DatabaseConfig {
		database_path: db_path,
		..Default::default()
	};
	let storage = Arc::new(tokio::sync::Mutex::new(
		RedbStorage::new(config).await.unwrap(),
	));
	let watch_id = Uuid::new_v4();
	let mut handles = vec![];
	for i in 0..16 {
		let storage = storage.clone();
		handles.push(tokio::spawn(async move {
			for j in 0..100 {
				let node = make_test_node(&format!("/write/file_{i}_{j}.txt"));
				storage
					.lock()
					.await
					.store_filesystem_node(&watch_id, &node, "test")
					.await
					.unwrap();
			}
		}));
	}
	for h in handles {
		h.await.unwrap();
	}
}

#[tokio::test]
async fn stress_concurrent_cache_read_write_mix() {
	let temp_dir = tempdir().expect("Failed to create temp dir");
	let db_path = temp_dir.path().join("stress_mix.redb");
	let config = rust_watcher::database::config::DatabaseConfig {
		database_path: db_path,
		..Default::default()
	};
	let storage = Arc::new(tokio::sync::Mutex::new(
		RedbStorage::new(config).await.unwrap(),
	));
	let watch_id = Uuid::new_v4();
	let node = make_test_node("/mix/file.txt");
	storage
		.lock()
		.await
		.store_filesystem_node(&watch_id, &node, "test")
		.await
		.unwrap();

	let mut handles = vec![];
	// Readers
	for _ in 0..8 {
		let storage = storage.clone();
		let path = node.path.clone();
		handles.push(tokio::spawn(async move {
			for _ in 0..100 {
				let _ = storage.lock().await.get_filesystem_node(&watch_id, &path).await.unwrap();
			}
		}));
	}
	// Writers
	for i in 0..8 {
		let storage = storage.clone();
		handles.push(tokio::spawn(async move {
			for j in 0..100 {
				let node = make_test_node(&format!("/mix/file_{i}_{j}.txt"));
				storage
					.lock()
					.await
					.store_filesystem_node(&watch_id, &node, "test")
					.await
					.unwrap();
			}
		}));
	}
	for h in handles {
		h.await.unwrap();
	}
}
