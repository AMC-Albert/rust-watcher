//! Integration test for cleanup of redundant watch-specific nodes and orphaned shared nodes
//!
//! This test registers overlapping watches, inserts both shared and watch-specific nodes, triggers
//! shared cache optimization, and verifies that redundant and orphaned nodes are removed.

use chrono::Utc;
use redb::ReadableTable;
use rust_watcher::database::storage::multi_watch::MultiWatchDatabase;
use rust_watcher::database::types::{FilesystemNode, WatchMetadata, WatchPermissions};
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::tempdir;
use uuid::Uuid;

fn make_watch(root: &str) -> WatchMetadata {
	WatchMetadata {
		watch_id: Uuid::new_v4(),
		root_path: PathBuf::from(root),
		created_at: Utc::now(),
		last_scan: None,
		node_count: 0,
		is_active: true,
		config_hash: 0,
		permissions: Some(WatchPermissions {
			can_read: true,
			can_write: true,
			can_delete: true,
			can_manage: true,
		}),
	}
}

#[tokio::test]
async fn test_cleanup_redundant_and_orphaned_nodes() {
	let temp_dir = tempdir().expect("Failed to create temp dir");
	let db_path = temp_dir.path().join("cleanup_test.db");
	let db = redb::Database::create(&db_path).expect("Failed to create database");
	let db = Arc::new(db);
	let multi_watch = MultiWatchDatabase::new(db.clone());

	// Register overlapping watches
	let w1 = make_watch("/a/b");
	let w2 = make_watch("/a/b/c");
	multi_watch.register_watch(&w1).await.expect("register_watch w1");
	multi_watch.register_watch(&w2).await.expect("register_watch w2");

	// Insert a redundant watch-specific node (same path as overlap)
	let node_path = PathBuf::from("/a/b");
	let node = FilesystemNode::new(
		node_path.clone(),
		&std::fs::metadata(temp_dir.path()).unwrap(),
	);
	let hash_from_node = node.computed.path_hash;
	let hash_from_util = rust_watcher::database::types::calculate_path_hash(&node_path);
	println!(
		"[TEST DEBUG] node.computed.path_hash = {hash_from_node:x}, calculate_path_hash = {hash_from_util:x}"
	);
	let key = bincode::serialize(&node.computed.path_hash).unwrap();
	let value = bincode::serialize(&node).unwrap();
	{
		let write_txn = db.begin_write().unwrap();
		{
			let mut table = write_txn
				.open_table(rust_watcher::database::storage::tables::MULTI_WATCH_FS_CACHE)
				.unwrap();
			table.insert(key.as_slice(), value.as_slice()).unwrap();
		}
		write_txn.commit().unwrap();
	}

	// Insert an orphaned shared node (reference_count == 0)
	let orphan_node = node.clone();
	let orphan_info = rust_watcher::database::types::SharedNodeInfo {
		node: orphan_node,
		watching_scopes: vec![],
		reference_count: 0,
		last_shared_update: Utc::now(),
	};
	let orphan_key = node.computed.path_hash.to_le_bytes(); // Use 8-byte LE encoding for key
	let orphan_value = bincode::serialize(&rust_watcher::database::types::UnifiedNode::Shared {
		shared_info: orphan_info,
	})
	.unwrap();
	{
		let write_txn = db.begin_write().unwrap();
		{
			let mut table = write_txn
				.open_table(rust_watcher::database::storage::tables::SHARED_NODES)
				.unwrap();
			table.insert(orphan_key.as_slice(), orphan_value.as_slice()).unwrap();
		}
		write_txn.commit().unwrap();
	}

	// Trigger optimization (which includes cleanup)
	multi_watch.optimize_shared_cache().await;

	// Check that the redundant watch-specific node is removed
	let fs_cache_count = {
		let read_txn = db.begin_read().unwrap();
		let table = read_txn
			.open_table(rust_watcher::database::storage::tables::MULTI_WATCH_FS_CACHE)
			.unwrap();
		table.iter().unwrap().count()
	};
	assert_eq!(
		fs_cache_count, 0,
		"Expected redundant watch-specific node to be removed"
	);

	// Check that the orphaned shared node is removed, but valid shared nodes remain
	let shared_nodes_keys = {
		let read_txn = db.begin_read().unwrap();
		let table = read_txn
			.open_table(rust_watcher::database::storage::tables::SHARED_NODES)
			.unwrap();
		table
			.iter()
			.unwrap()
			.map(|entry| entry.unwrap().0.value().to_vec())
			.collect::<Vec<_>>()
	};
	assert_eq!(
		shared_nodes_keys.len(),
		1,
		"Expected only one valid shared node to remain after orphan cleanup"
	);
	// Optionally, check that the remaining key is not the orphaned node's key
	assert_ne!(
		shared_nodes_keys[0],
		node.computed.path_hash.to_le_bytes().to_vec(),
		"Orphaned shared node key should not remain"
	);
}
