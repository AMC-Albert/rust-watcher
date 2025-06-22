//! Stress and edge-case tests for multi-watch scenarios: overlapping roots, shared node invalidation, and rapid add/remove.

use chrono::Utc;
use rust_watcher::database::storage::multi_watch::MultiWatchDatabase;
use rust_watcher::database::types::{FilesystemNode, NodeType, SharedNodeInfo, WatchMetadata};
use std::sync::Arc;
use tempfile::tempdir;
use uuid::Uuid;

fn make_watch(root: &str) -> WatchMetadata {
	WatchMetadata {
		watch_id: Uuid::new_v4(),
		root_path: root.into(),
		created_at: Utc::now(),
		last_scan: None,
		node_count: 0,
		is_active: true,
		config_hash: 0,
		permissions: None,
	}
}

fn make_shared_node(path: &str, scopes: Vec<Uuid>, ref_count: u32) -> SharedNodeInfo {
	use rust_watcher::database::types::{calculate_path_hash, ComputedProperties};
	let path_buf = std::path::PathBuf::from(path);
	let path_hash = calculate_path_hash(&path_buf);
	SharedNodeInfo {
		node: FilesystemNode {
			path: path_buf.clone(),
			node_type: NodeType::Directory { child_count: 0, total_size: 0, max_depth: 0 },
			metadata: Default::default(),
			cache_info: Default::default(),
			computed: ComputedProperties {
				depth_from_root: path_buf.components().count() as u16,
				path_hash,
				parent_hash: path_buf.parent().map(calculate_path_hash),
				canonical_name: path_buf
					.file_name()
					.unwrap_or_default()
					.to_string_lossy()
					.to_string(),
			},
			last_event_type: Some("test".to_string()),
		},
		watching_scopes: scopes,
		reference_count: ref_count,
		last_shared_update: Utc::now(),
	}
}

#[tokio::test]
async fn stress_overlapping_watch_registration_and_invalidation() {
	let temp_dir = tempdir().expect("Failed to create temp dir");
	let db_path = temp_dir.path().join(format!("multi_watch_stress-{}.redb", Uuid::new_v4()));
	let db = redb::Database::create(&db_path).expect("Failed to create database");
	let db = Arc::new(db);
	let multi_watch = Arc::new(MultiWatchDatabase::new(db.clone()));

	// Register overlapping watches
	let w1 = make_watch("/a");
	let w2 = make_watch("/a/b");
	let w3 = make_watch("/a/b/c");
	multi_watch.register_watch(&w1).await.expect("register w1");
	multi_watch.register_watch(&w2).await.expect("register w2");
	multi_watch.register_watch(&w3).await.expect("register w3");

	// Add shared node referenced by all
	let shared = make_shared_node(
		"/a/b/c/shared",
		vec![w1.watch_id, w2.watch_id, w3.watch_id],
		3,
	);
	multi_watch.store_shared_node(&shared).await.expect("store shared");

	// Remove watches in various orders and check shared node
	multi_watch.remove_watch(&w2.watch_id).await.expect("remove w2");
	let hash =
		rust_watcher::database::types::calculate_path_hash(std::path::Path::new("/a/b/c/shared"));
	let node = multi_watch.get_shared_node(hash).await.expect("get shared");
	let node = node.unwrap();
	assert_eq!(node.reference_count, 2);
	assert!(node.watching_scopes.contains(&w1.watch_id));
	assert!(node.watching_scopes.contains(&w3.watch_id));
	assert!(!node.watching_scopes.contains(&w2.watch_id));

	multi_watch.remove_watch(&w1.watch_id).await.expect("remove w1");
	let node = multi_watch.get_shared_node(hash).await.expect("get shared");
	let node = node.unwrap();
	assert_eq!(node.reference_count, 1);
	assert!(node.watching_scopes.contains(&w3.watch_id));
	assert!(!node.watching_scopes.contains(&w1.watch_id));

	multi_watch.remove_watch(&w3.watch_id).await.expect("remove w3");
	let node = multi_watch.get_shared_node(hash).await.expect("get shared");
	assert!(node.is_none() || node.as_ref().unwrap().reference_count == 0);
}
