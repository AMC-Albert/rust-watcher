//! Trait definitions for filesystem cache storage operations

use crate::database::error::DatabaseResult;
use crate::database::types::{FilesystemNode, SharedNodeInfo, WatchMetadata};
use std::path::Path;
use uuid::Uuid;

#[async_trait::async_trait]
pub trait FilesystemCacheStorage: Send + Sync {
	/// Store a filesystem node for a specific watch
	async fn store_filesystem_node(
		&mut self, watch_id: &Uuid, node: &FilesystemNode, event_type: &str,
	) -> DatabaseResult<()>;

	/// Retrieve a filesystem node for a specific watch
	async fn get_filesystem_node(
		&mut self, watch_id: &Uuid, path: &Path,
	) -> DatabaseResult<Option<FilesystemNode>>;

	/// List all nodes in a directory for a specific watch
	async fn list_directory_for_watch(
		&mut self, watch_id: &Uuid, parent_path: &Path,
	) -> DatabaseResult<Vec<FilesystemNode>>;

	/// Store watch metadata
	async fn store_watch_metadata(&mut self, metadata: &WatchMetadata) -> DatabaseResult<()>;

	/// Retrieve watch metadata
	async fn get_watch_metadata(
		&mut self, watch_id: &Uuid,
	) -> DatabaseResult<Option<WatchMetadata>>;

	/// Remove all data for a watch
	async fn remove_watch(&mut self, watch_id: &Uuid) -> DatabaseResult<()>;

	/// Store shared node information
	async fn store_shared_node(&mut self, shared_info: &SharedNodeInfo) -> DatabaseResult<()>;

	/// Retrieve shared node information
	async fn get_shared_node(&mut self, path_hash: u64) -> DatabaseResult<Option<SharedNodeInfo>>;

	/// Batch store multiple filesystem nodes
	async fn batch_store_filesystem_nodes(
		&mut self, watch_id: &Uuid, nodes: &[FilesystemNode], event_type: &str,
	) -> DatabaseResult<()>;

	/// Find nodes by path prefix (for efficient subtree operations)
	async fn find_nodes_by_prefix(
		&mut self, watch_id: &Uuid, prefix: &Path,
	) -> DatabaseResult<Vec<FilesystemNode>>;

	/// Get cache statistics for a watch
	async fn get_cache_stats(&mut self, watch_id: &Uuid) -> DatabaseResult<CacheStats>;

	/// Clean up stale cache entries
	async fn cleanup_stale_cache(
		&mut self, watch_id: &Uuid, max_age_seconds: u64,
	) -> DatabaseResult<usize>;

	// === Phase 3: Unified and Hierarchical Queries ===

	/// List directory contents across all watches (unified view).
	///
	/// Returns all nodes that are direct children of the given path, regardless of watch.
	/// See RedbFilesystemCache for the actual implementation. This default is a stub.
	async fn list_directory_unified(
		&mut self, _parent_path: &std::path::Path,
	) -> DatabaseResult<Vec<FilesystemNode>> {
		unimplemented!("list_directory_unified not yet implemented; see RedbFilesystemCache for implementation");
	}

	/// Get a unified node view for a given path (across all watches).
	///
	/// Returns the most relevant node (shared or watch-specific) for the path.
	/// TODO: Implement cross-watch node resolution and conflict handling.
	async fn get_unified_node(
		&mut self, _path: &std::path::Path,
	) -> DatabaseResult<Option<FilesystemNode>> {
		// Not yet implemented. This will require merging shared and watch-specific nodes.
		// Edge cases: conflicting metadata, permissions, and node types.
		unimplemented!("get_unified_node not yet implemented");
	}

	/// List all ancestor nodes for a given path (up to root).
	///
	/// Returns the chain of parent nodes, for hierarchical queries.
	/// TODO: Implement efficient ancestor traversal using prefix or hierarchy tables.
	async fn list_ancestors(
		&mut self, _path: &std::path::Path,
	) -> DatabaseResult<Vec<FilesystemNode>> {
		// Not yet implemented. This will require walking up the hierarchy and resolving nodes.
		// Edge cases: missing parents, cross-watch ancestry, symlink loops.
		unimplemented!("list_ancestors not yet implemented");
	}

	/// List all descendant nodes for a given path (subtree query).
	///
	/// Returns all nodes under the given path, recursively.
	/// TODO: Implement efficient subtree traversal using prefix or hierarchy tables.
	async fn list_descendants(
		&mut self, _path: &std::path::Path,
	) -> DatabaseResult<Vec<FilesystemNode>> {
		// Not yet implemented. This will require recursive or batched traversal.
		// Edge cases: large subtrees, cycles, permission filtering.
		unimplemented!("list_descendants not yet implemented");
	}

	/// Pattern-based search for nodes (e.g., glob, regex).
	///
	/// Returns all nodes matching the given pattern. WARNING: Current implementation is a naive full scan and will not scale for large caches. Use only for testing or small datasets.
	/// TODO: Implement efficient indexed search for production use. Edge cases: escaping, cross-platform path semantics, and performance bottlenecks.
	async fn search_nodes(&mut self, _pattern: &str) -> DatabaseResult<Vec<FilesystemNode>> {
		// Not yet implemented. This will require pattern parsing and index scans.
		// Edge cases: performance, escaping, cross-platform path semantics.
		unimplemented!("search_nodes not yet implemented");
	}

	/// Retrieve a single filesystem node for a specific watch (single-node query).
	///
	/// Returns the node if present, or None if not found. This is a fundamental API for cache lookups.
	/// TODO: Implement in all backends. Edge cases: path normalization, cross-platform semantics, and stale cache entries.
	async fn get_node(
		&mut self, _watch_id: &Uuid, _path: &Path,
	) -> DatabaseResult<Option<FilesystemNode>> {
		unimplemented!("get_node not yet implemented");
	}

	/// Remove a single filesystem node for a specific watch
	async fn remove_filesystem_node(
		&mut self, watch_id: &Uuid, path: &Path, event_type: &str,
	) -> DatabaseResult<()>;

	/// Rename (move) a filesystem node for a specific watch
	async fn rename_filesystem_node(
		&mut self, watch_id: &Uuid, old_path: &Path, new_path: &Path, event_type: &str,
	) -> DatabaseResult<()>;
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
