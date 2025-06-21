//! Integration tests for multi-watch scenarios
//!
//! This file provides a framework for testing multi-watch database operations, including watch registration, metadata management, and shared node coordination.
//!
//! All tests are currently stubs and should be implemented as the multi-watch API is developed.

use chrono::Utc;
use rust_watcher::database::storage::multi_watch::MultiWatchDatabase;
use rust_watcher::database::types::{SharedNodeInfo, WatchMetadata};
use tempfile::tempdir;
use uuid::Uuid;

#[tokio::test]
async fn test_register_and_list_watches() {
	let temp_dir = tempdir().expect("Failed to create temp dir");
	let db_path = temp_dir
		.path()
		.join(format!("multi_watch_test-{}.db", uuid::Uuid::new_v4()));
	let db = redb::Database::create(&db_path).expect("Failed to create database");
	let db = std::sync::Arc::new(db);
	let multi_watch = MultiWatchDatabase::new(db.clone());

	// Register two watches
	let watch1 = WatchMetadata {
		watch_id: Uuid::new_v4(),
		root_path: temp_dir.path().join("watch1"),
		created_at: Utc::now(),
		last_scan: None,
		node_count: 0,
		is_active: true,
		config_hash: 123,
		permissions: None,
	};
	let watch2 = WatchMetadata {
		watch_id: Uuid::new_v4(),
		root_path: temp_dir.path().join("watch2"),
		created_at: Utc::now(),
		last_scan: None,
		node_count: 0,
		is_active: true,
		config_hash: 456,
		permissions: None,
	};
	multi_watch
		.register_watch(&watch1)
		.await
		.expect("register_watch 1");
	multi_watch
		.register_watch(&watch2)
		.await
		.expect("register_watch 2");

	// List watches and verify
	let watches = multi_watch.list_watches().await.expect("list_watches");
	assert_eq!(watches.len(), 2);
	assert!(watches.iter().any(|w| w.watch_id == watch1.watch_id));
	assert!(watches.iter().any(|w| w.watch_id == watch2.watch_id));

	// Get metadata for one watch
	let meta = multi_watch
		.get_watch_metadata(&watch1.watch_id)
		.await
		.expect("get_watch_metadata");
	assert!(meta.is_some());
	assert_eq!(meta.unwrap().config_hash, 123);
}

#[tokio::test]
async fn test_remove_watch() {
	let temp_dir = tempdir().expect("Failed to create temp dir");
	let db_path = temp_dir.path().join(format!(
		"multi_watch_test_remove-{}.db",
		uuid::Uuid::new_v4()
	));
	let db = redb::Database::create(&db_path).expect("Failed to create database");
	let db = std::sync::Arc::new(db);
	let multi_watch = MultiWatchDatabase::new(db.clone());

	// Register a watch
	let watch = WatchMetadata {
		watch_id: Uuid::new_v4(),
		root_path: temp_dir.path().join("watch"),
		created_at: Utc::now(),
		last_scan: None,
		node_count: 0,
		is_active: true,
		config_hash: 789,
		permissions: None,
	};
	multi_watch
		.register_watch(&watch)
		.await
		.expect("register_watch");

	// Add a shared node referencing this watch
	let shared_info = SharedNodeInfo {
		node: rust_watcher::database::types::FilesystemNode {
			path: watch.root_path.clone(),
			node_type: rust_watcher::database::types::NodeType::Directory {
				child_count: 0,
				total_size: 0,
				max_depth: 0,
			},
			metadata: rust_watcher::database::types::NodeMetadata {
				modified_time: std::time::SystemTime::now(),
				created_time: None,
				accessed_time: None,
				permissions: 0,
				inode: None,
				windows_id: None,
			},
			cache_info: rust_watcher::database::types::CacheInfo {
				cached_at: Utc::now(),
				last_verified: Utc::now(),
				cache_version: 1,
				needs_refresh: false,
			},
			computed: rust_watcher::database::types::ComputedProperties {
				depth_from_root: 0,
				path_hash: rust_watcher::database::types::calculate_path_hash(&watch.root_path),
				parent_hash: None,
				canonical_name: "watch".to_string(),
			},
		},
		watching_scopes: vec![watch.watch_id],
		reference_count: 1,
		last_shared_update: Utc::now(),
	};
	multi_watch
		.store_shared_node(&shared_info)
		.await
		.expect("store_shared_node");

	// Remove the watch
	multi_watch
		.remove_watch(&watch.watch_id)
		.await
		.expect("remove_watch");

	// Assert the watch is gone
	let watches = multi_watch.list_watches().await.expect("list_watches");
	assert!(!watches.iter().any(|w| w.watch_id == watch.watch_id));

	// Assert the shared node is removed (reference_count == 0)
	let path_hash = rust_watcher::database::types::calculate_path_hash(&watch.root_path);
	let shared = multi_watch
		.get_shared_node(path_hash)
		.await
		.expect("get_shared_node");
	assert!(shared.is_none() || shared.as_ref().unwrap().reference_count == 0);
}

#[tokio::test]
async fn test_shared_node_management() {
	let temp_dir = tempdir().expect("Failed to create temp dir");
	let db_path = temp_dir.path().join(format!(
		"multi_watch_test_shared-{}.db",
		uuid::Uuid::new_v4()
	));
	let db = redb::Database::create(&db_path).expect("Failed to create database");
	let db = std::sync::Arc::new(db);
	let multi_watch = MultiWatchDatabase::new(db.clone());

	// Register two watches
	let watch1 = WatchMetadata {
		watch_id: Uuid::new_v4(),
		root_path: temp_dir.path().join("watch1"),
		created_at: Utc::now(),
		last_scan: None,
		node_count: 0,
		is_active: true,
		config_hash: 111,
		permissions: None,
	};
	let watch2 = WatchMetadata {
		watch_id: Uuid::new_v4(),
		root_path: temp_dir.path().join("watch2"),
		created_at: Utc::now(),
		last_scan: None,
		node_count: 0,
		is_active: true,
		config_hash: 222,
		permissions: None,
	};
	multi_watch
		.register_watch(&watch1)
		.await
		.expect("register_watch1");
	multi_watch
		.register_watch(&watch2)
		.await
		.expect("register_watch2");

	// Add a shared node referencing both watches
	let shared_info = SharedNodeInfo {
		node: rust_watcher::database::types::FilesystemNode {
			path: temp_dir.path().join("shared"),
			node_type: rust_watcher::database::types::NodeType::Directory {
				child_count: 0,
				total_size: 0,
				max_depth: 0,
			},
			metadata: rust_watcher::database::types::NodeMetadata {
				modified_time: std::time::SystemTime::now(),
				created_time: None,
				accessed_time: None,
				permissions: 0,
				inode: None,
				windows_id: None,
			},
			cache_info: rust_watcher::database::types::CacheInfo {
				cached_at: Utc::now(),
				last_verified: Utc::now(),
				cache_version: 1,
				needs_refresh: false,
			},
			computed: rust_watcher::database::types::ComputedProperties {
				depth_from_root: 0,
				path_hash: rust_watcher::database::types::calculate_path_hash(
					&temp_dir.path().join("shared"),
				),
				parent_hash: None,
				canonical_name: "shared".to_string(),
			},
		},
		watching_scopes: vec![watch1.watch_id, watch2.watch_id],
		reference_count: 2,
		last_shared_update: Utc::now(),
	};
	multi_watch
		.store_shared_node(&shared_info)
		.await
		.expect("store_shared_node");

	// Remove one watch
	multi_watch
		.remove_watch(&watch1.watch_id)
		.await
		.expect("remove_watch1");
	let path_hash = rust_watcher::database::types::calculate_path_hash(&shared_info.node.path);
	let shared = multi_watch
		.get_shared_node(path_hash)
		.await
		.expect("get_shared_node");
	assert!(shared.is_some());
	let shared = shared.unwrap();
	assert_eq!(shared.reference_count, 1);
	assert_eq!(shared.watching_scopes, vec![watch2.watch_id]);

	// Remove the second watch
	multi_watch
		.remove_watch(&watch2.watch_id)
		.await
		.expect("remove_watch2");
	let shared = multi_watch
		.get_shared_node(path_hash)
		.await
		.expect("get_shared_node");
	assert!(shared.is_none() || shared.as_ref().unwrap().reference_count == 0);
}
