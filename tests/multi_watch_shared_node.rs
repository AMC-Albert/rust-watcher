//! Tests for shared node management in MultiWatchDatabase

use chrono::Utc;
use rust_watcher::database::storage::multi_watch::implementation::MultiWatchDatabase;
use rust_watcher::database::types::{
	CacheInfo, ComputedProperties, FilesystemNode, NodeMetadata, NodeType, SharedNodeInfo,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;
use tempfile::tempdir;
use uuid::Uuid;

#[tokio::test]
async fn test_store_and_get_shared_node() {
	let temp_dir = tempdir().expect("Failed to create temp dir");
	let db_path = temp_dir.path().join("shared_node_test.db");
	let db = redb::Database::create(&db_path).expect("Failed to create database");
	let db = Arc::new(db);
	let multi_watch = MultiWatchDatabase::new(db.clone());

	let path = PathBuf::from("/tmp/shared_file.txt");
	let path_hash = rust_watcher::database::types::calculate_path_hash(&path);
	let node = FilesystemNode {
		path: path.clone(),
		node_type: NodeType::File {
			size: 42,
			content_hash: None,
			mime_type: None,
		},
		metadata: NodeMetadata {
			modified_time: SystemTime::now(),
			created_time: None,
			accessed_time: None,
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
			path_hash,
			parent_hash: None,
			canonical_name: "shared_file.txt".to_string(),
		},
	};
	let shared_info = SharedNodeInfo {
		node: node.clone(),
		watching_scopes: vec![Uuid::new_v4()],
		reference_count: 1,
		last_shared_update: Utc::now(),
	};

	multi_watch
		.store_shared_node(&shared_info)
		.await
		.expect("store_shared_node");
	let retrieved = multi_watch
		.get_shared_node(path_hash)
		.await
		.expect("get_shared_node");
	assert!(retrieved.is_some());
	let retrieved = retrieved.unwrap();
	assert_eq!(retrieved.node.path, node.path);
	assert_eq!(retrieved.reference_count, 1);
}

#[tokio::test]
async fn test_remove_watch_cleans_up_shared_node() {
	let temp_dir = tempdir().expect("Failed to create temp dir");
	let db_path = temp_dir.path().join("remove_watch_test.db");
	let db = redb::Database::create(&db_path).expect("Failed to create database");
	let db = Arc::new(db);
	let multi_watch = MultiWatchDatabase::new(db.clone());

	let watch_id = Uuid::new_v4();
	let path = PathBuf::from("/tmp/shared_file.txt");
	let path_hash = rust_watcher::database::types::calculate_path_hash(&path);
	let node = FilesystemNode {
		path: path.clone(),
		node_type: NodeType::File {
			size: 42,
			content_hash: None,
			mime_type: None,
		},
		metadata: NodeMetadata {
			modified_time: SystemTime::now(),
			created_time: None,
			accessed_time: None,
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
			path_hash,
			parent_hash: None,
			canonical_name: "shared_file.txt".to_string(),
		},
	};
	let mut shared_info = SharedNodeInfo {
		node: node.clone(),
		watching_scopes: vec![watch_id],
		reference_count: 1,
		last_shared_update: Utc::now(),
	};
	multi_watch
		.store_shared_node(&shared_info)
		.await
		.expect("store_shared_node");
	// Remove the only watch, should delete the shared node
	multi_watch
		.remove_watch(&watch_id)
		.await
		.expect("remove_watch");
	let retrieved = multi_watch
		.get_shared_node(path_hash)
		.await
		.expect("get_shared_node");
	assert!(retrieved.is_none());

	// Now test with two watches, only one removed
	let watch_id2 = Uuid::new_v4();
	shared_info.watching_scopes = vec![watch_id, watch_id2];
	shared_info.reference_count = 2;
	multi_watch
		.store_shared_node(&shared_info)
		.await
		.expect("store_shared_node");
	multi_watch
		.remove_watch(&watch_id)
		.await
		.expect("remove_watch");
	let retrieved = multi_watch
		.get_shared_node(path_hash)
		.await
		.expect("get_shared_node");
	assert!(retrieved.is_some());
	let retrieved = retrieved.unwrap();
	assert_eq!(retrieved.reference_count, 1);
	assert_eq!(retrieved.watching_scopes, vec![watch_id2]);
}
