//! Integration tests for filesystem cache functionality
//!
//! These tests validate the correctness and performance of the filesystem cache layer:
//! - Node insert and retrieval
//! - Directory hierarchy queries
//! - Batch insert
//! - Prefix/subtree queries

use rust_watcher::database::types::FilesystemNode;
use rust_watcher::database::{DatabaseConfig, DatabaseStorage, RedbStorage};
use tempfile::TempDir;
use uuid::Uuid;

#[tokio::test]
async fn test_filesystem_node_insert_and_retrieve() {
	let temp_dir = TempDir::new().expect("Failed to create temp directory");
	let db_path = temp_dir.path().join(format!("fs_cache_test-{}.redb", uuid::Uuid::new_v4()));
	let config = DatabaseConfig { database_path: db_path, ..Default::default() };
	let mut storage = RedbStorage::new(config).await.expect("Failed to create storage");
	let watch_id = Uuid::new_v4();

	// Create a test node
	let node_path = temp_dir.path().join("foo.txt");
	std::fs::write(&node_path, b"test").unwrap();
	let metadata = std::fs::metadata(&node_path).unwrap();
	let node = FilesystemNode::new(node_path.clone(), &metadata);

	// Store and retrieve
	storage.store_filesystem_node(&watch_id, &node).await.expect("store");
	let retrieved = storage.get_filesystem_node(&watch_id, &node_path).await.expect("get");
	assert!(retrieved.is_some());
	let retrieved = retrieved.unwrap();
	assert_eq!(retrieved.path, node.path);
	assert_eq!(retrieved.node_type, node.node_type);
}

#[tokio::test]
async fn test_filesystem_hierarchy_and_list_directory() {
	let temp_dir = TempDir::new().expect("Failed to create temp directory");
	let db_path = temp_dir
		.path()
		.join(format!("fs_cache_hierarchy-{}.redb", uuid::Uuid::new_v4()));
	let config = DatabaseConfig { database_path: db_path, ..Default::default() };
	let mut storage = RedbStorage::new(config).await.expect("Failed to create storage");
	let watch_id = Uuid::new_v4();

	// Create parent and child nodes
	let parent_path = temp_dir.path().join("parent");
	let child_path = parent_path.join("child.txt");
	std::fs::create_dir(&parent_path).unwrap();
	std::fs::write(&child_path, b"child").unwrap();
	let parent_meta = std::fs::metadata(&parent_path).unwrap();
	let child_meta = std::fs::metadata(&child_path).unwrap();
	let parent_node = FilesystemNode::new(parent_path.clone(), &parent_meta);
	let child_node = FilesystemNode::new(child_path.clone(), &child_meta);

	// Store both
	storage
		.store_filesystem_node(&watch_id, &parent_node)
		.await
		.expect("store parent");
	storage
		.store_filesystem_node(&watch_id, &child_node)
		.await
		.expect("store child");

	// List directory
	let children = storage.list_directory_for_watch(&watch_id, &parent_path).await.expect("list");
	assert_eq!(children.len(), 1);
	assert_eq!(children[0].path, child_path);
}

#[tokio::test]
async fn test_get_node_api() {
	let temp_dir = TempDir::new().expect("Failed to create temp directory");
	let db_path = temp_dir.path().join(format!("fs_cache_get_node-{}.redb", uuid::Uuid::new_v4()));
	let config = DatabaseConfig { database_path: db_path, ..Default::default() };
	let mut storage = RedbStorage::new(config).await.expect("Failed to create storage");
	let watch_id = Uuid::new_v4();

	// Create and store a node
	let node_path = temp_dir.path().join("bar.txt");
	std::fs::write(&node_path, b"test").unwrap();
	let metadata = std::fs::metadata(&node_path).unwrap();
	let node = FilesystemNode::new(node_path.clone(), &metadata);
	storage.store_filesystem_node(&watch_id, &node).await.expect("store");

	// get_node should return the same as get_filesystem_node
	let retrieved = storage.get_node(&watch_id, &node_path).await.expect("get_node");
	assert!(retrieved.is_some());
	let retrieved = retrieved.unwrap();
	assert_eq!(retrieved.path, node.path);
	assert_eq!(retrieved.node_type, node.node_type);

	// get_node for a missing node should return None
	let missing_path = temp_dir.path().join("does_not_exist.txt");
	let missing = storage.get_node(&watch_id, &missing_path).await.expect("get_node missing");
	assert!(missing.is_none());
}

// Additional tests for batch insert, prefix queries, and edge cases can be added here.
