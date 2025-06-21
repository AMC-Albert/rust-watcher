//! Tests for watch overlap detection and statistics in MultiWatchDatabase

use chrono::Utc;
use rust_watcher::database::storage::multi_watch::implementation::MultiWatchDatabase;
use rust_watcher::database::storage::multi_watch::types::WatchOverlap;
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
async fn test_detect_overlap_cases() {
	let temp_dir = tempdir().expect("Failed to create temp dir");
	let db_path = temp_dir.path().join("overlap_test.db");
	let db = redb::Database::create(&db_path).expect("Failed to create database");
	let db = Arc::new(db);
	let multi_watch = MultiWatchDatabase::new(db.clone());

	let w1 = make_watch("/a/b");
	let w2 = make_watch("/a/b");
	let w3 = make_watch("/a");
	let w4 = make_watch("/a/b/c");
	let w5 = make_watch("/x/y");
	let w6 = make_watch("/a/bx");

	// Identical
	assert_eq!(
		multi_watch.detect_overlap(&w1, &w2).await,
		WatchOverlap::Identical(w1.watch_id)
	);
	// Ancestor/descendant
	assert_eq!(
		multi_watch.detect_overlap(&w1, &w3).await,
		WatchOverlap::Ancestor {
			ancestor: w3.watch_id,
			descendant: w1.watch_id
		}
	);
	assert_eq!(
		multi_watch.detect_overlap(&w4, &w1).await,
		WatchOverlap::Ancestor {
			ancestor: w1.watch_id,
			descendant: w4.watch_id
		}
	);
	// No overlap
	assert_eq!(
		multi_watch.detect_overlap(&w1, &w5).await,
		WatchOverlap::None
	);
	// Partial (common prefix /a)
	let overlap = multi_watch.detect_overlap(&w1, &w6).await;
	match overlap {
		WatchOverlap::Partial { common_prefix, .. } => {
			assert_eq!(common_prefix, PathBuf::from("/a"))
		}
		_ => panic!("Expected partial overlap"),
	}
}

#[tokio::test]
async fn test_compute_overlap_statistics() {
	let temp_dir = tempdir().expect("Failed to create temp dir");
	let db_path = temp_dir.path().join("overlap_stats_test.db");
	let db = redb::Database::create(&db_path).expect("Failed to create database");
	let db = Arc::new(db);
	let multi_watch = MultiWatchDatabase::new(db.clone());

	let watches = vec![
		make_watch("/a/b"),
		make_watch("/a"),
		make_watch("/a/b/c"),
		make_watch("/x/y"),
		make_watch("/a/bx"),
	];
	for w in &watches {
		multi_watch.register_watch(w).await.expect("register_watch");
	}
	let overlaps = multi_watch
		.compute_overlap_statistics()
		.await
		.expect("compute_overlap_statistics");
	// There should be at least one ancestor/descendant and one partial overlap
	assert!(overlaps
		.iter()
		.any(|o| matches!(o, WatchOverlap::Ancestor { .. })));
	assert!(overlaps
		.iter()
		.any(|o| matches!(o, WatchOverlap::Partial { .. })));
}
