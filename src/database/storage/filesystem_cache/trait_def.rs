//! Trait definitions for filesystem cache storage operations

use crate::database::error::DatabaseResult;
use crate::database::types::{FilesystemNode, SharedNodeInfo, WatchMetadata};
use std::path::Path;
use uuid::Uuid;

#[async_trait::async_trait]
pub trait FilesystemCacheStorage: Send + Sync {
	/// Store a filesystem node for a specific watch
	async fn store_filesystem_node(
		&mut self,
		watch_id: &Uuid,
		node: &FilesystemNode,
	) -> DatabaseResult<()>;

	/// Retrieve a filesystem node for a specific watch
	async fn get_filesystem_node(
		&mut self,
		watch_id: &Uuid,
		path: &Path,
	) -> DatabaseResult<Option<FilesystemNode>>;

	/// List all nodes in a directory for a specific watch
	async fn list_directory_for_watch(
		&mut self,
		watch_id: &Uuid,
		parent_path: &Path,
	) -> DatabaseResult<Vec<FilesystemNode>>;

	/// Store watch metadata
	async fn store_watch_metadata(&mut self, metadata: &WatchMetadata) -> DatabaseResult<()>;

	/// Retrieve watch metadata
	async fn get_watch_metadata(
		&mut self,
		watch_id: &Uuid,
	) -> DatabaseResult<Option<WatchMetadata>>;

	/// Remove all data for a watch
	async fn remove_watch(&mut self, watch_id: &Uuid) -> DatabaseResult<()>;

	/// Store shared node information
	async fn store_shared_node(&mut self, shared_info: &SharedNodeInfo) -> DatabaseResult<()>;

	/// Retrieve shared node information
	async fn get_shared_node(&mut self, path_hash: u64) -> DatabaseResult<Option<SharedNodeInfo>>;

	/// Batch store multiple filesystem nodes
	async fn batch_store_filesystem_nodes(
		&mut self,
		watch_id: &Uuid,
		nodes: &[FilesystemNode],
	) -> DatabaseResult<()>;

	/// Find nodes by path prefix (for efficient subtree operations)
	async fn find_nodes_by_prefix(
		&mut self,
		watch_id: &Uuid,
		prefix: &Path,
	) -> DatabaseResult<Vec<FilesystemNode>>;

	/// Get cache statistics for a watch
	async fn get_cache_stats(&mut self, watch_id: &Uuid) -> DatabaseResult<CacheStats>;

	/// Clean up stale cache entries
	async fn cleanup_stale_cache(
		&mut self,
		watch_id: &Uuid,
		max_age_seconds: u64,
	) -> DatabaseResult<usize>;
}

/// Cache statistics for monitoring
#[derive(Debug, Clone)]
pub struct CacheStats {
	pub total_nodes: u64,
	pub directories: u64,
	pub files: u64,
	pub symlinks: u64,
	pub shared_nodes: u64,
	pub stale_entries: u64,
	pub cache_size_bytes: u64,
	pub last_updated: chrono::DateTime<chrono::Utc>,
}

impl Default for CacheStats {
	fn default() -> Self {
		Self {
			total_nodes: 0,
			directories: 0,
			files: 0,
			symlinks: 0,
			shared_nodes: 0,
			stale_entries: 0,
			cache_size_bytes: 0,
			last_updated: chrono::Utc::now(),
		}
	}
}
