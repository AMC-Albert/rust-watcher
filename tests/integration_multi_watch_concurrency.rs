//! Integration test for concurrent watch registration and removal in MultiWatchDatabase
//!
//! This test simulates concurrent registration and removal of watches to expose race conditions
//! and verify correct shared node and transaction state. It is not a substitute for full stress testing,
//! but will catch common logic errors and deadlocks.

use chrono::Utc;
use rust_watcher::database::storage::multi_watch::MultiWatchDatabase;
use rust_watcher::database::types::{WatchMetadata, WatchPermissions};
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::task;
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
async fn test_concurrent_watch_registration_and_removal() {
	let temp_dir = tempdir().expect("Failed to create temp dir");
	let db_path = temp_dir.path().join(format!(
		"multi_watch_concurrency_test-{}.redb",
		uuid::Uuid::new_v4()
	));
	let db = redb::Database::create(&db_path).expect("Failed to create database");
	let db = Arc::new(db);
	let multi_watch = Arc::new(MultiWatchDatabase::new(db.clone()));

	let roots = vec!["/a/b", "/a/c", "/a/d", "/x/y", "/x/z"];
	let mut handles = Vec::new();

	// Spawn concurrent registration tasks
	for root in &roots {
		let mw = multi_watch.clone();
		let w = make_watch(root);
		handles.push(task::spawn(async move {
			mw.register_watch(&w).await.expect("register_watch");
			w.watch_id
		}));
	}

	// Wait for all registrations
	let watch_ids: Vec<_> = futures::future::join_all(handles)
		.await
		.into_iter()
		.map(|r| r.unwrap())
		.collect();

	// Spawn concurrent removal tasks
	let mut remove_handles = Vec::new();
	for watch_id in &watch_ids {
		let mw = multi_watch.clone();
		let id = *watch_id;
		remove_handles.push(task::spawn(async move {
			mw.remove_watch(&id).await.expect("remove_watch");
		}));
	}
	futures::future::join_all(remove_handles).await;

	// After all removals, the database should have no watches
	let remaining = multi_watch.list_watches().await.expect("list_watches");
	assert!(
		remaining.is_empty(),
		"Expected no watches remaining, found {}",
		remaining.len()
	);
}
