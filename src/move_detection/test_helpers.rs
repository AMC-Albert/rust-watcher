//! Test helpers and mocks for move_detection module

use crate::database::error::DatabaseResult;
use crate::database::storage::filesystem_cache::trait_def::{CacheStats, FilesystemCacheStorage};
use crate::database::types::{FilesystemNode, SharedNodeInfo, WatchMetadata};
use std::path::Path;
use uuid::Uuid;

/// DummyCache: a stub FilesystemCacheStorage for unit tests
pub struct DummyCache;

#[async_trait::async_trait]
impl FilesystemCacheStorage for DummyCache {
	async fn store_filesystem_node(
		&mut self, _watch_id: &Uuid, _node: &FilesystemNode, _event_type: &str,
	) -> DatabaseResult<()> {
		Ok(())
	}
	async fn get_filesystem_node(
		&mut self, _watch_id: &Uuid, _path: &Path,
	) -> DatabaseResult<Option<FilesystemNode>> {
		Ok(None)
	}
	async fn list_directory_for_watch(
		&mut self, _watch_id: &Uuid, _parent_path: &Path,
	) -> DatabaseResult<Vec<FilesystemNode>> {
		Ok(vec![])
	}
	async fn store_watch_metadata(&mut self, _metadata: &WatchMetadata) -> DatabaseResult<()> {
		Ok(())
	}
	async fn get_watch_metadata(
		&mut self, _watch_id: &Uuid,
	) -> DatabaseResult<Option<WatchMetadata>> {
		Ok(None)
	}
	async fn remove_watch(&mut self, _watch_id: &Uuid) -> DatabaseResult<()> {
		Ok(())
	}
	async fn store_shared_node(&mut self, _shared_info: &SharedNodeInfo) -> DatabaseResult<()> {
		Ok(())
	}
	async fn get_shared_node(&mut self, _path_hash: u64) -> DatabaseResult<Option<SharedNodeInfo>> {
		Ok(None)
	}
	async fn batch_store_filesystem_nodes(
		&mut self, _watch_id: &Uuid, _nodes: &[FilesystemNode], _event_type: &str,
	) -> DatabaseResult<()> {
		Ok(())
	}
	async fn find_nodes_by_prefix(
		&mut self, _watch_id: &Uuid, _prefix: &Path,
	) -> DatabaseResult<Vec<FilesystemNode>> {
		Ok(vec![])
	}
	async fn get_cache_stats(&mut self, _watch_id: &Uuid) -> DatabaseResult<CacheStats> {
		Ok(Default::default())
	}
	async fn cleanup_stale_cache(
		&mut self, _watch_id: &Uuid, _max_age_seconds: u64,
	) -> DatabaseResult<usize> {
		Ok(0)
	}
	async fn list_directory_unified(
		&mut self, _parent_path: &Path,
	) -> DatabaseResult<Vec<FilesystemNode>> {
		Ok(vec![])
	}
	async fn get_unified_node(&mut self, _path: &Path) -> DatabaseResult<Option<FilesystemNode>> {
		Ok(None)
	}
	async fn list_ancestors(&mut self, _path: &Path) -> DatabaseResult<Vec<FilesystemNode>> {
		Ok(vec![])
	}
	async fn list_descendants(&mut self, _path: &Path) -> DatabaseResult<Vec<FilesystemNode>> {
		Ok(vec![])
	}
	async fn search_nodes(&mut self, _pattern: &str) -> DatabaseResult<Vec<FilesystemNode>> {
		Ok(vec![])
	}
	async fn get_node(
		&mut self, _watch_id: &Uuid, _path: &Path,
	) -> DatabaseResult<Option<FilesystemNode>> {
		Ok(None)
	}
	async fn remove_filesystem_node(
		&mut self, _watch_id: &Uuid, _path: &Path, _event_type: &str,
	) -> DatabaseResult<()> {
		Ok(())
	}
	async fn rename_filesystem_node(
		&mut self, _watch_id: &Uuid, _old_path: &Path, _new_path: &Path, _event_type: &str,
	) -> DatabaseResult<()> {
		Ok(())
	}
}
