//! Common helpers for database integration tests (filesystem cache)

use rust_watcher::database::types::FilesystemNode;
use rust_watcher::database::{DatabaseConfig, DatabaseStorage, RedbStorage};
use std::path::PathBuf;
use tempfile::TempDir;
use uuid::Uuid;

/// Create a temp database and RedbStorage for integration tests
pub async fn setup_test_storage(test_name: &str) -> (TempDir, PathBuf, RedbStorage, Uuid) {
	let temp_dir = TempDir::new().expect("Failed to create temp directory");
	let db_path = temp_dir.path().join(format!(
		"fs_cache_{test_name}-{}.redb",
		uuid::Uuid::new_v4()
	));
	let config = DatabaseConfig { database_path: db_path.clone(), ..Default::default() };
	let storage = RedbStorage::new(config).await.expect("Failed to create storage");
	let watch_id = Uuid::new_v4();
	(temp_dir, db_path, storage, watch_id)
}

/// Create and store a FilesystemNode for a given path
pub async fn create_and_store_node(
	storage: &mut RedbStorage, watch_id: &Uuid, path: &std::path::Path, event_type: &str,
) -> FilesystemNode {
	std::fs::write(path, b"test").unwrap();
	let metadata = std::fs::metadata(path).unwrap();
	let node = FilesystemNode::new_with_event_type(
		path.to_path_buf(),
		&metadata,
		Some(event_type.to_string()),
	);
	storage
		.store_filesystem_node(watch_id, &node, event_type)
		.await
		.expect("store node");
	node
}
