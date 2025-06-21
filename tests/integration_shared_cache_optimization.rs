//! Integration test for shared cache optimization in MultiWatchDatabase
//!
//! This test registers overlapping watches, triggers shared cache optimization, and verifies that
//! shared nodes are created in the SHARED_NODES table. This is a minimal test and does not cover
//! all edge cases or error handling.

use chrono::Utc;
use redb::ReadableTable;
use rust_watcher::database::storage::multi_watch::MultiWatchDatabase;
use rust_watcher::database::types::{WatchMetadata, WatchPermissions};
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
async fn test_shared_cache_optimization_creates_shared_nodes() {
	let temp_dir = tempdir().expect("Failed to create temp dir");
	let db_path = temp_dir.path().join("shared_cache_opt_test.db");
	let db = redb::Database::create(&db_path).expect("Failed to create database");
	let db = Arc::new(db);
	let multi_watch = MultiWatchDatabase::new(db.clone());

	// Register overlapping watches
	let w1 = make_watch("/a/b");
	let w2 = make_watch("/a/b/c");
	multi_watch
		.register_watch(&w1)
		.await
		.expect("register_watch w1");
	multi_watch
		.register_watch(&w2)
		.await
		.expect("register_watch w2");

	// Trigger optimization
	multi_watch.optimize_shared_cache().await;

	// Check that a shared node was created for the overlap
	let shared_nodes = {
		let read_txn = db.begin_read().expect("begin_read");
		let table = read_txn
			.open_table(rust_watcher::database::storage::tables::SHARED_NODES)
			.expect("open_table");
		table.iter().expect("iter").count()
	};
	assert!(
		shared_nodes > 0,
		"Expected at least one shared node to be created, found {}",
		shared_nodes
	);
}
