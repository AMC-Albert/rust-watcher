//! Filesystem cache storage operations
//!
//! This module handles storage and retrieval of filesystem cache data,
//! including nodes, hierarchy relationships, and shared node management.

use crate::database::{
	error::{DatabaseError, DatabaseResult},
	types::{
		calculate_path_hash, FilesystemKey, FilesystemNode, SharedNodeInfo, UnifiedNode,
		WatchMetadata, WatchScopedKey,
	},
};
use redb::{Database, MultimapTableDefinition, ReadableTable, TableDefinition};
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

// Filesystem cache table definitions
const FS_CACHE_NODES: TableDefinition<&[u8], &[u8]> = TableDefinition::new("fs_cache_nodes");
const FS_HIERARCHY: MultimapTableDefinition<&[u8], &[u8]> =
	MultimapTableDefinition::new("fs_hierarchy");
const SHARED_NODES: TableDefinition<&[u8], &[u8]> = TableDefinition::new("shared_nodes");
const WATCH_REGISTRY: TableDefinition<&[u8], &[u8]> = TableDefinition::new("watch_registry");
const PATH_TO_WATCHES: MultimapTableDefinition<&[u8], &[u8]> =
	MultimapTableDefinition::new("path_to_watches");

/// Trait for filesystem cache storage operations
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

/// Implementation of filesystem cache storage using ReDB
pub struct RedbFilesystemCache {
	database: Arc<Database>,
}

impl RedbFilesystemCache {
	/// Create a new filesystem cache storage instance
	pub fn new(database: Arc<Database>) -> Self {
		Self { database }
	}

	/// Initialize the filesystem cache tables
	pub async fn initialize(&mut self) -> DatabaseResult<()> {
		let write_txn = self.database.begin_write()?;
		{
			// Create tables if they don't exist
			let _fs_cache_table = write_txn.open_table(FS_CACHE_NODES)?;
			let _hierarchy_table = write_txn.open_multimap_table(FS_HIERARCHY)?;
			let _shared_table = write_txn.open_table(SHARED_NODES)?;
			let _watch_registry = write_txn.open_table(WATCH_REGISTRY)?;
			let _path_watches = write_txn.open_multimap_table(PATH_TO_WATCHES)?;
		}
		write_txn.commit()?;
		Ok(())
	}

	/// Create a scoped key for a watch-specific node
	fn create_scoped_key(watch_id: &Uuid, path_hash: u64) -> WatchScopedKey {
		WatchScopedKey {
			watch_id: *watch_id,
			path_hash,
		}
	}

	/// Create a shared key for cross-watch nodes
	fn create_shared_key(path_hash: u64) -> u64 {
		path_hash
	}

	/// Serialize data to bytes
	fn serialize<T: serde::Serialize>(data: &T) -> DatabaseResult<Vec<u8>> {
		bincode::serialize(data).map_err(|e| DatabaseError::Serialization(e.to_string()))
	}

	/// Deserialize bytes to data
	fn deserialize<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> DatabaseResult<T> {
		bincode::deserialize(bytes).map_err(|e| DatabaseError::Deserialization(e.to_string()))
	}

	/// Convert key to bytes for storage
	fn key_to_bytes(key: &WatchScopedKey) -> Vec<u8> {
		Self::serialize(key).unwrap_or_default()
	}
}

#[async_trait::async_trait]
impl FilesystemCacheStorage for RedbFilesystemCache {
	async fn store_filesystem_node(
		&mut self,
		watch_id: &Uuid,
		node: &FilesystemNode,
	) -> DatabaseResult<()> {
		let path_hash = calculate_path_hash(&node.path);
		let scoped_key = Self::create_scoped_key(watch_id, path_hash);
		let key_bytes = Self::key_to_bytes(&scoped_key);
		let node_bytes = Self::serialize(node)?;

		let write_txn = self.database.begin_write()?;
		{
			let mut fs_cache_table = write_txn.open_table(FS_CACHE_NODES)?;
			let mut hierarchy_table = write_txn.open_multimap_table(FS_HIERARCHY)?;
			let mut path_watches_table = write_txn.open_multimap_table(PATH_TO_WATCHES)?;

			// Store the node
			fs_cache_table.insert(key_bytes.as_slice(), node_bytes.as_slice())?;

			// Update hierarchy relationships
			if let Some(parent_hash) = node.computed.parent_hash {
				let parent_key = Self::create_scoped_key(watch_id, parent_hash);
				let parent_key_bytes = Self::key_to_bytes(&parent_key);
				hierarchy_table.insert(parent_key_bytes.as_slice(), key_bytes.as_slice())?;
			}

			// Update path-to-watches mapping
			let path_key = path_hash.to_le_bytes();
			let watch_bytes = watch_id.as_bytes() as &[u8];
			path_watches_table.insert(path_key.as_slice(), watch_bytes)?;
		}
		write_txn.commit()?;
		Ok(())
	}

	async fn get_filesystem_node(
		&mut self,
		watch_id: &Uuid,
		path: &Path,
	) -> DatabaseResult<Option<FilesystemNode>> {
		let path_hash = calculate_path_hash(path);
		let scoped_key = Self::create_scoped_key(watch_id, path_hash);
		let key_bytes = Self::key_to_bytes(&scoped_key);

		let read_txn = self.database.begin_read()?;
		let fs_cache_table = read_txn.open_table(FS_CACHE_NODES)?;

		if let Some(node_bytes) = fs_cache_table.get(key_bytes.as_slice())? {
			let node: FilesystemNode = Self::deserialize(node_bytes.value())?;
			Ok(Some(node))
		} else {
			Ok(None)
		}
	}

	async fn list_directory_for_watch(
		&mut self,
		watch_id: &Uuid,
		parent_path: &Path,
	) -> DatabaseResult<Vec<FilesystemNode>> {
		let parent_hash = calculate_path_hash(parent_path);
		let parent_key = Self::create_scoped_key(watch_id, parent_hash);
		let parent_key_bytes = Self::key_to_bytes(&parent_key);

		let read_txn = self.database.begin_read()?;
		let hierarchy_table = read_txn.open_multimap_table(FS_HIERARCHY)?;
		let fs_cache_table = read_txn.open_table(FS_CACHE_NODES)?;

		let mut nodes = Vec::new();

		// Get all child keys for this parent
		let child_iter = hierarchy_table.get(parent_key_bytes.as_slice())?;
		for child_key_result in child_iter {
			let child_key = child_key_result?;
			if let Some(node_bytes) = fs_cache_table.get(child_key.value())? {
				let node: FilesystemNode = Self::deserialize(node_bytes.value())?;
				nodes.push(node);
			}
		}

		Ok(nodes)
	}

	async fn store_watch_metadata(&mut self, metadata: &WatchMetadata) -> DatabaseResult<()> {
		let metadata_bytes = Self::serialize(metadata)?;
		let watch_key = metadata.watch_id.as_bytes() as &[u8];

		let write_txn = self.database.begin_write()?;
		{
			let mut watch_table = write_txn.open_table(WATCH_REGISTRY)?;
			watch_table.insert(watch_key, metadata_bytes.as_slice())?;
		}
		write_txn.commit()?;
		Ok(())
	}

	async fn get_watch_metadata(
		&mut self,
		watch_id: &Uuid,
	) -> DatabaseResult<Option<WatchMetadata>> {
		let read_txn = self.database.begin_read()?;
		let watch_table = read_txn.open_table(WATCH_REGISTRY)?;

		let watch_key = watch_id.as_bytes() as &[u8];
		if let Some(metadata_bytes) = watch_table.get(watch_key)? {
			let metadata: WatchMetadata = Self::deserialize(metadata_bytes.value())?;
			Ok(Some(metadata))
		} else {
			Ok(None)
		}
	}

	async fn remove_watch(&mut self, watch_id: &Uuid) -> DatabaseResult<()> {
		let write_txn = self.database.begin_write()?;
		{
			let mut fs_cache_table = write_txn.open_table(FS_CACHE_NODES)?;
			let mut hierarchy_table = write_txn.open_multimap_table(FS_HIERARCHY)?;
			let mut watch_table = write_txn.open_table(WATCH_REGISTRY)?;
			let mut path_watches_table = write_txn.open_multimap_table(PATH_TO_WATCHES)?;

			// Remove watch metadata
			let watch_key = watch_id.as_bytes() as &[u8];
			watch_table.remove(watch_key)?;

			// Remove all nodes and hierarchy entries for this watch
			// This is a complex operation that requires iterating through all entries
			// In practice, this might be optimized with additional indexing

			// For now, we'll mark this as a TODO for a more efficient implementation
			// TODO: Implement efficient watch removal with proper indexing
		}
		write_txn.commit()?;
		Ok(())
	}

	async fn store_shared_node(&mut self, shared_info: &SharedNodeInfo) -> DatabaseResult<()> {
		let path_hash = calculate_path_hash(&shared_info.node.path);
		let shared_bytes = Self::serialize(shared_info)?;
		let shared_key = path_hash.to_le_bytes();

		let write_txn = self.database.begin_write()?;
		{
			let mut shared_table = write_txn.open_table(SHARED_NODES)?;
			shared_table.insert(shared_key.as_slice(), shared_bytes.as_slice())?;
		}
		write_txn.commit()?;
		Ok(())
	}

	async fn get_shared_node(&mut self, path_hash: u64) -> DatabaseResult<Option<SharedNodeInfo>> {
		let read_txn = self.database.begin_read()?;
		let shared_table = read_txn.open_table(SHARED_NODES)?;

		let shared_key = path_hash.to_le_bytes();
		if let Some(shared_bytes) = shared_table.get(shared_key.as_slice())? {
			let shared_info: SharedNodeInfo = Self::deserialize(shared_bytes.value())?;
			Ok(Some(shared_info))
		} else {
			Ok(None)
		}
	}

	async fn batch_store_filesystem_nodes(
		&mut self,
		watch_id: &Uuid,
		nodes: &[FilesystemNode],
	) -> DatabaseResult<()> {
		let write_txn = self.database.begin_write()?;
		{
			let mut fs_cache_table = write_txn.open_table(FS_CACHE_NODES)?;
			let mut hierarchy_table = write_txn.open_multimap_table(FS_HIERARCHY)?;
			let mut path_watches_table = write_txn.open_multimap_table(PATH_TO_WATCHES)?;

			for node in nodes {
				let path_hash = calculate_path_hash(&node.path);
				let scoped_key = Self::create_scoped_key(watch_id, path_hash);
				let key_bytes = Self::key_to_bytes(&scoped_key);
				let node_bytes = Self::serialize(node)?;

				// Store the node
				fs_cache_table.insert(key_bytes.as_slice(), node_bytes.as_slice())?;

				// Update hierarchy relationships
				if let Some(parent_hash) = node.computed.parent_hash {
					let parent_key = Self::create_scoped_key(watch_id, parent_hash);
					let parent_key_bytes = Self::key_to_bytes(&parent_key);
					hierarchy_table.insert(parent_key_bytes.as_slice(), key_bytes.as_slice())?;
				}

				// Update path-to-watches mapping
				let path_key = path_hash.to_le_bytes();
				let watch_bytes = watch_id.as_bytes() as &[u8];
				path_watches_table.insert(path_key.as_slice(), watch_bytes)?;
			}
		}
		write_txn.commit()?;
		Ok(())
	}

	async fn find_nodes_by_prefix(
		&mut self,
		watch_id: &Uuid,
		prefix: &Path,
	) -> DatabaseResult<Vec<FilesystemNode>> {
		// This is a complex operation that would benefit from prefix indexing
		// For now, we'll implement a basic version that scans all nodes
		// TODO: Implement efficient prefix indexing for better performance

		let read_txn = self.database.begin_read()?;
		let fs_cache_table = read_txn.open_table(FS_CACHE_NODES)?;

		let mut matching_nodes = Vec::new();
		let prefix_str = prefix.to_string_lossy();

		let iter = fs_cache_table.iter()?;
		for entry_result in iter {
			let (key, value) = entry_result?;
			let scoped_key: WatchScopedKey = Self::deserialize(key.value())?;

			// Check if this key belongs to our watch
			if scoped_key.watch_id == *watch_id {
				let node: FilesystemNode = Self::deserialize(value.value())?;
				if node.path.to_string_lossy().starts_with(&*prefix_str) {
					matching_nodes.push(node);
				}
			}
		}

		Ok(matching_nodes)
	}

	async fn get_cache_stats(&mut self, watch_id: &Uuid) -> DatabaseResult<CacheStats> {
		let read_txn = self.database.begin_read()?;
		let fs_cache_table = read_txn.open_table(FS_CACHE_NODES)?;

		let mut stats = CacheStats::default();
		let current_time = chrono::Utc::now();

		// Count nodes for this watch
		let iter = fs_cache_table.iter()?;
		for entry_result in iter {
			let (key, value) = entry_result?;
			let scoped_key: WatchScopedKey = Self::deserialize(key.value())?;

			if scoped_key.watch_id == *watch_id {
				let node: FilesystemNode = Self::deserialize(value.value())?;
				stats.total_nodes += 1;
				stats.cache_size_bytes += value.value().len() as u64;

				// Count by node type
				match &node.node_type {
					crate::database::types::NodeType::File { .. } => stats.files += 1,
					crate::database::types::NodeType::Directory { .. } => stats.directories += 1,
					crate::database::types::NodeType::Symlink { .. } => stats.symlinks += 1,
				}

				// Check if stale (older than 1 hour)
				let age = current_time - node.cache_info.last_verified;
				if age.num_seconds() > 3600 {
					stats.stale_entries += 1;
				}
			}
		}

		stats.last_updated = current_time;
		Ok(stats)
	}

	async fn cleanup_stale_cache(
		&mut self,
		watch_id: &Uuid,
		max_age_seconds: u64,
	) -> DatabaseResult<usize> {
		let read_txn = self.database.begin_read()?;
		let fs_cache_table = read_txn.open_table(FS_CACHE_NODES)?;

		let current_time = chrono::Utc::now();
		let mut stale_keys = Vec::new();

		// Find stale entries
		let iter = fs_cache_table.iter()?;
		for entry_result in iter {
			let (key, value) = entry_result?;
			let scoped_key: WatchScopedKey = Self::deserialize(key.value())?;

			if scoped_key.watch_id == *watch_id {
				let node: FilesystemNode = Self::deserialize(value.value())?;
				let age = current_time - node.cache_info.last_verified;

				if age.num_seconds() > max_age_seconds as i64 {
					stale_keys.push(key.value().to_vec());
				}
			}
		}
		// drop(iter); // Removed: iter is moved in the for loop and cannot be dropped explicitly
		drop(read_txn);

		// Remove stale entries
		if !stale_keys.is_empty() {
			let write_txn = self.database.begin_write()?;
			{
				let mut fs_cache_table = write_txn.open_table(FS_CACHE_NODES)?;
				for key in &stale_keys {
					fs_cache_table.remove(key.as_slice())?;
				}
			}
			write_txn.commit()?;
		}

		Ok(stale_keys.len())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use redb::Database;
	use std::path::PathBuf;
	use tempfile::tempdir;

	async fn create_test_cache() -> DatabaseResult<RedbFilesystemCache> {
		let temp_dir = tempdir().unwrap();
		let db_path = temp_dir.path().join("test_cache.db");
		let database = Database::create(&db_path)?;
		let database = Arc::new(database);

		let mut cache = RedbFilesystemCache::new(database);
		cache.initialize().await?;
		Ok(cache)
	}

	#[tokio::test]
	async fn test_store_and_retrieve_node() {
		let mut cache = create_test_cache().await.unwrap();
		let watch_id = Uuid::new_v4();

		// Create a test node
		let node = FilesystemNode::from_path(PathBuf::from("/test/file.txt")).unwrap();

		// Store the node
		cache.store_filesystem_node(&watch_id, &node).await.unwrap();

		// Retrieve the node
		let retrieved = cache
			.get_filesystem_node(&watch_id, &node.path)
			.await
			.unwrap();
		assert!(retrieved.is_some());
		assert_eq!(retrieved.unwrap().path, node.path);
	}

	#[tokio::test]
	async fn test_watch_metadata() {
		let mut cache = create_test_cache().await.unwrap();
		let watch_id = Uuid::new_v4();

		let metadata = WatchMetadata {
			watch_id,
			root_path: PathBuf::from("/test"),
			created_at: chrono::Utc::now(),
			last_scan: None,
			node_count: 0,
			is_active: true,
			config_hash: 12345,
		};

		// Store metadata
		cache.store_watch_metadata(&metadata).await.unwrap();

		// Retrieve metadata
		let retrieved = cache.get_watch_metadata(&watch_id).await.unwrap();
		assert!(retrieved.is_some());
		assert_eq!(retrieved.unwrap().watch_id, watch_id);
	}

	#[tokio::test]
	async fn test_batch_operations() {
		let mut cache = create_test_cache().await.unwrap();
		let watch_id = Uuid::new_v4();

		// Create test nodes
		let nodes = vec![
			FilesystemNode::from_path(PathBuf::from("/test/file1.txt")).unwrap(),
			FilesystemNode::from_path(PathBuf::from("/test/file2.txt")).unwrap(),
			FilesystemNode::from_path(PathBuf::from("/test/dir")).unwrap(),
		];

		// Batch store nodes
		cache
			.batch_store_filesystem_nodes(&watch_id, &nodes)
			.await
			.unwrap();

		// Verify all nodes were stored
		for node in &nodes {
			let retrieved = cache
				.get_filesystem_node(&watch_id, &node.path)
				.await
				.unwrap();
			assert!(retrieved.is_some());
		}
	}

	#[tokio::test]
	async fn test_cache_stats() {
		let mut cache = create_test_cache().await.unwrap();
		let watch_id = Uuid::new_v4();

		// Store some test nodes
		let nodes = vec![
			FilesystemNode::from_path(PathBuf::from("/test/file1.txt")).unwrap(),
			FilesystemNode::from_path(PathBuf::from("/test/dir")).unwrap(),
		];

		cache
			.batch_store_filesystem_nodes(&watch_id, &nodes)
			.await
			.unwrap();

		// Get stats
		let stats = cache.get_cache_stats(&watch_id).await.unwrap();
		assert_eq!(stats.total_nodes, 2);
		assert!(stats.cache_size_bytes > 0);
	}
}
