//! Unit and integration tests for the filesystem cache module
//!
//! These tests cover the ReDB implementation and helpers.
//!
//! Known issues:
//! - Tests may be slow due to naive search and traversal.
//! - Some edge cases (e.g., concurrent access) are not covered.

#[cfg(test)]
mod tests {
	use super::super::synchronizer::{DefaultFilesystemCacheSynchronizer, FilesystemCacheSynchronizer};
	use crate::database::types::{FilesystemNode, NodeType};
	use crate::events::{FileSystemEvent, EventType};
	use tempfile::TempDir;
	use uuid::Uuid;

	#[tokio::test]
	async fn test_synchronizer_create_and_remove() {
		let temp_dir = TempDir::new().unwrap();
		let db_path = temp_dir.path().join(format!("sync_test-{}.redb", Uuid::new_v4()));
		let db = redb::Database::create(&db_path).unwrap();
		let mut cache = super::super::implementation::RedbFilesystemCache::new(Arc::new(db));
		let cache = Arc::new(tokio::sync::Mutex::new(cache));
		let mut synchronizer = DefaultFilesystemCacheSynchronizer { cache: cache.clone() };
		let watch_id = Uuid::new_v4();
		// Simulate a create event
		let test_path = temp_dir.path().join("file.txt");
		std::fs::write(&test_path, b"test").unwrap();
		let event = FileSystemEvent {
			id: Uuid::new_v4(),
			event_type: EventType::Create,
			path: test_path.clone(),
			timestamp: chrono::Utc::now(),
			is_directory: false,
			size: Some(4),
			move_data: None,
		};
		synchronizer.handle_event(&watch_id, &event).await;
		// Node should exist in cache
		let node = cache.lock().await.get_filesystem_node(&watch_id, &test_path).await.unwrap();
		assert!(node.is_some(), "Node should exist after create event");
		// Simulate a remove event (removal not implemented, should log but not remove)
		let remove_event = FileSystemEvent {
			event_type: EventType::Remove,
			..event.clone()
		};
		synchronizer.handle_event(&watch_id, &remove_event).await;
		let node = cache.lock().await.get_filesystem_node(&watch_id, &test_path).await.unwrap();
		assert!(node.is_some(), "Node should still exist (removal not implemented)");
	}
}
