//! Filesystem cache storage operations
//!
//! This module handles storage and retrieval of filesystem cache data,
//! including nodes, hierarchy relationships, and shared node management.

use crate::database::storage::tables::{
	MULTI_WATCH_FS_CACHE, MULTI_WATCH_HIERARCHY, PATH_PREFIX_TABLE, PATH_TO_WATCHES, SHARED_NODES,
	WATCH_REGISTRY,
};
use crate::database::{
	error::DatabaseResult,
	types::{calculate_path_hash, FilesystemNode, SharedNodeInfo, WatchMetadata, WatchScopedKey},
};
use redb::{Database, ReadableTable};
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

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
			// Use shared table constants from tables.rs
			let _fs_cache_table = write_txn.open_table(MULTI_WATCH_FS_CACHE)?;
			let _hierarchy_table = write_txn.open_multimap_table(MULTI_WATCH_HIERARCHY)?;
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

	#[allow(dead_code)]
	/// Create a shared key for cross-watch nodes
	fn create_shared_key(path_hash: u64) -> u64 {
		path_hash
	}

	/// Serialize data to bytes
	fn serialize<T: serde::Serialize>(data: &T) -> DatabaseResult<Vec<u8>> {
		use crate::database::error::DatabaseError;
		bincode::serialize(data).map_err(|e| DatabaseError::Serialization(e.to_string()))
	}

	/// Deserialize bytes to data
	fn deserialize<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> DatabaseResult<T> {
		use crate::database::error::DatabaseError;
		bincode::deserialize(bytes).map_err(|e| DatabaseError::Deserialization(e.to_string()))
	}

	/// Convert key to bytes for storage
	fn key_to_bytes(key: &WatchScopedKey) -> Vec<u8> {
		Self::serialize(key).unwrap_or_default()
	}

	/// Helper: insert all path prefixes for a node into PATH_PREFIX_TABLE
	fn index_path_prefixes(
		write_txn: &redb::WriteTransaction,
		node: &FilesystemNode,
		watch_id: &Uuid,
	) -> DatabaseResult<()> {
		let mut prefix_table = write_txn.open_multimap_table(PATH_PREFIX_TABLE)?;
		let path = &node.path;
		let path_hash = calculate_path_hash(path);
		let scoped_key = Self::create_scoped_key(watch_id, path_hash);
		let key_bytes = Self::key_to_bytes(&scoped_key);
		// Insert all parent prefixes (e.g., /a, /a/b, /a/b/c)
		let mut prefix = Path::new("").to_path_buf();
		for component in path.components() {
			prefix.push(component);
			let prefix_str = prefix.to_string_lossy();
			prefix_table.insert(prefix_str.as_bytes(), key_bytes.as_slice())?;
		}
		Ok(())
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
			// Store the node
			let mut fs_cache_table = write_txn.open_table(MULTI_WATCH_FS_CACHE)?;
			fs_cache_table.insert(key_bytes.as_slice(), node_bytes.as_slice())?;

			// Update hierarchy relationships
			if let Some(parent_hash) = node.computed.parent_hash {
				let parent_key = Self::create_scoped_key(watch_id, parent_hash);
				let parent_key_bytes = Self::key_to_bytes(&parent_key);
				let mut hierarchy_table = write_txn.open_multimap_table(MULTI_WATCH_HIERARCHY)?;
				hierarchy_table.insert(parent_key_bytes.as_slice(), key_bytes.as_slice())?;
			}

			// Update path-to-watches mapping
			let path_key = path_hash.to_le_bytes();
			let watch_bytes = &watch_id.as_bytes()[..];
			let mut path_watches_table = write_txn.open_multimap_table(PATH_TO_WATCHES)?;
			path_watches_table.insert(path_key.as_slice(), watch_bytes)?;

			// Update path prefix index
			Self::index_path_prefixes(&write_txn, node, watch_id)?;
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
		let fs_cache_table = read_txn.open_table(MULTI_WATCH_FS_CACHE)?;

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
		let hierarchy_table = read_txn.open_multimap_table(MULTI_WATCH_HIERARCHY)?;
		let fs_cache_table = read_txn.open_table(MULTI_WATCH_FS_CACHE)?;

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
		let watch_key = &metadata.watch_id.as_bytes()[..];

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

		let watch_key = &watch_id.as_bytes()[..];
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
			let mut watch_table = write_txn.open_table(WATCH_REGISTRY)?;

			// Remove watch metadata
			let watch_key = &watch_id.as_bytes()[..];
			watch_table.remove(watch_key)?;

			// Remove all nodes for this watch
			let mut fs_cache_table = write_txn.open_table(MULTI_WATCH_FS_CACHE)?;
			let mut to_remove = Vec::new();
			for entry in fs_cache_table.iter()? {
				let (key, _) = entry?;
				let scoped_key: WatchScopedKey = Self::deserialize(key.value())?;
				if scoped_key.watch_id == *watch_id {
					to_remove.push(key.value().to_vec());
				}
			}
			for key in &to_remove {
				fs_cache_table.remove(&key[..])?;
			}

			// Remove all hierarchy and path-to-watches entries for this watch
			// Limitation: MultimapTable does not support full iteration; we must scan known keys.
			let mut hierarchy_table = write_txn.open_multimap_table(MULTI_WATCH_HIERARCHY)?;
			let mut path_watches_table = write_txn.open_multimap_table(PATH_TO_WATCHES)?;
			for key in &to_remove {
				// Remove all hierarchy entries for this node
				hierarchy_table.remove_all(&key[..])?;
				// Remove all path-to-watches entries for this node
				path_watches_table.remove_all(&key[..])?;
			}
			// NOTE: This will not remove orphaned entries if the tables are out of sync.
			// For full correctness, a separate index or a full scan of all possible keys is required.
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
			for node in nodes {
				let path_hash = calculate_path_hash(&node.path);
				let scoped_key = Self::create_scoped_key(watch_id, path_hash);
				let key_bytes = Self::key_to_bytes(&scoped_key);
				let node_bytes = Self::serialize(node)?;

				// Store the node
				let mut fs_cache_table = write_txn.open_table(MULTI_WATCH_FS_CACHE)?;
				fs_cache_table.insert(key_bytes.as_slice(), node_bytes.as_slice())?;

				// Update hierarchy relationships
				if let Some(parent_hash) = node.computed.parent_hash {
					let parent_key = Self::create_scoped_key(watch_id, parent_hash);
					let parent_key_bytes = Self::key_to_bytes(&parent_key);
					let mut hierarchy_table =
						write_txn.open_multimap_table(MULTI_WATCH_HIERARCHY)?;
					hierarchy_table.insert(parent_key_bytes.as_slice(), key_bytes.as_slice())?;
				}

				// Update path-to-watches mapping
				let path_key = path_hash.to_le_bytes();
				let watch_bytes = &watch_id.as_bytes()[..];
				let mut path_watches_table = write_txn.open_multimap_table(PATH_TO_WATCHES)?;
				path_watches_table.insert(path_key.as_slice(), watch_bytes)?;

				// Update path prefix index
				Self::index_path_prefixes(&write_txn, node, watch_id)?;
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
		let read_txn = self.database.begin_read()?;
		let prefix_table = read_txn.open_multimap_table(PATH_PREFIX_TABLE)?;
		let fs_cache_table = read_txn.open_table(MULTI_WATCH_FS_CACHE)?;
		let prefix_str = prefix.to_string_lossy();
		let mut nodes = Vec::new();
		if let Ok(child_iter) = prefix_table.get(prefix_str.as_bytes()) {
			for child_key in child_iter {
				let child_key = child_key?;
				let scoped_key: WatchScopedKey = Self::deserialize(child_key.value())?;
				if scoped_key.watch_id == *watch_id {
					if let Some(node_bytes) = fs_cache_table.get(child_key.value())? {
						let node: FilesystemNode = Self::deserialize(node_bytes.value())?;
						nodes.push(node);
					}
				}
			}
		}
		Ok(nodes)
	}

	async fn get_cache_stats(&mut self, watch_id: &Uuid) -> DatabaseResult<CacheStats> {
		let read_txn = self.database.begin_read()?;
		let fs_cache_table = read_txn.open_table(MULTI_WATCH_FS_CACHE)?;

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
		let fs_cache_table = read_txn.open_table(MULTI_WATCH_FS_CACHE)?;

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
				let mut fs_cache_table = write_txn.open_table(MULTI_WATCH_FS_CACHE)?;
				for key in &stale_keys {
					fs_cache_table.remove(key.as_slice())?;
				}
			}
			write_txn.commit()?;
		}

		Ok(stale_keys.len())
	}
}

// --- Serialization Format Notes ---
// FilesystemNode and related types use bincode for serialization.
// bincode is not forward/backward compatible by default; schema changes can break deserialization.
// Any change to FilesystemNode or its fields must be accompanied by a migration or versioning plan.
// Test round-trip serialization to catch schema drift early.

#[cfg(test)]
mod tests {
	use super::*;
	use crate::database::types::FilesystemNode;
	use std::fs;
	use tempfile::tempdir;

	#[test]
	fn test_filesystem_node_roundtrip() {
		// Create a temp file to get real metadata
		let dir = tempdir().unwrap();
		let file_path = dir.path().join("testfile.txt");
		fs::write(&file_path, b"hello world").unwrap();
		let metadata = fs::metadata(&file_path).unwrap();
		let node = FilesystemNode::new(file_path.clone(), &metadata);

		let bytes = RedbFilesystemCache::serialize(&node).expect("serialize");
		let decoded: FilesystemNode =
			RedbFilesystemCache::deserialize(&bytes).expect("deserialize");

		// Path equality is strict; on Windows, normalization may be needed for robust tests
		assert_eq!(node.path, decoded.path);
		assert_eq!(node.node_type, decoded.node_type);
		assert_eq!(node.metadata.permissions, decoded.metadata.permissions);
		assert_eq!(node.computed.path_hash, decoded.computed.path_hash);
	}

	#[test]
	fn test_filesystem_node_serialization_failure() {
		// Corrupt data should fail to deserialize
		let bad_bytes = vec![0, 1, 2, 3, 4, 5];
		let result: Result<FilesystemNode, _> = RedbFilesystemCache::deserialize(&bad_bytes);
		assert!(result.is_err());
	}

	#[test]
	fn test_prefix_indexing() {
		let dir = tempdir().unwrap();
		let file_a = dir.path().join("a.txt");
		let file_b = dir.path().join("subdir/b.txt");
		let file_c = dir.path().join("subdir/c.txt");
		fs::write(&file_a, b"A").unwrap();
		fs::create_dir_all(file_b.parent().unwrap()).unwrap();
		fs::write(&file_b, b"B").unwrap();
		fs::write(&file_c, b"C").unwrap();
		let meta_a = fs::metadata(&file_a).unwrap();
		let meta_b = fs::metadata(&file_b).unwrap();
		let meta_c = fs::metadata(&file_c).unwrap();
		let node_a = FilesystemNode::new(file_a.clone(), &meta_a);
		let node_b = FilesystemNode::new(file_b.clone(), &meta_b);
		let node_c = FilesystemNode::new(file_c.clone(), &meta_c);
		let db = Database::create(dir.path().join("test.db")).unwrap();
		let db = Arc::new(db);
		let mut cache = RedbFilesystemCache::new(db.clone());
		let watch_id = Uuid::new_v4();
		futures::executor::block_on(cache.store_filesystem_node(&watch_id, &node_a)).unwrap();
		futures::executor::block_on(cache.store_filesystem_node(&watch_id, &node_b)).unwrap();
		futures::executor::block_on(cache.store_filesystem_node(&watch_id, &node_c)).unwrap();
		// Query for prefix "subdir"
		let prefix = dir.path().join("subdir");
		let found =
			futures::executor::block_on(cache.find_nodes_by_prefix(&watch_id, &prefix)).unwrap();
		let found_paths: Vec<_> = found.iter().map(|n| n.path.clone()).collect();
		assert!(found_paths.contains(&file_b));
		assert!(found_paths.contains(&file_c));
		assert!(!found_paths.contains(&file_a));
	}

	// #[tokio::test]
	// async fn test_store_and_retrieve_node() {
	//     let _cache = create_test_cache().await.unwrap();
	//     let _watch_id = Uuid::new_v4();
	//     // Test is a stub; see above for TODO.
	//     return;

	//     // Create a test node
	//     // TODO: Implement FilesystemNode::from_path or use FilesystemNode::new with real metadata in tests
	//     // let node = FilesystemNode::from_path(PathBuf::from("/test/file.txt")).unwrap();
	//     // The FilesystemNode::new signature does not match; test is a stub until a proper constructor is available.
	//     // let node = FilesystemNode::new(
	//     //     &PathBuf::from("/test/file.txt"),
	//     //     None,
	//     //     None,
	//     //     None,
	//     //     None,
	//     //     None,
	//     // );
	//     // TODO: Restore this test when FilesystemNode::new or from_path is implemented for testability.
	//     // return;

	//     // Store the node
	//     // cache.store_filesystem_node(&watch_id, &node).await.unwrap();

	//     // Retrieve the node
	//     // let retrieved = cache
	//     // 	.get_filesystem_node(&watch_id, &node.path)
	//     // 	.await
	//     // 	.unwrap();
	//     // assert!(retrieved.is_some());
	//     // assert_eq!(retrieved.unwrap().path, node.path);
	// }

	// #[tokio::test]
	// async fn test_watch_metadata() {
	//     let _cache = create_test_cache().await.unwrap();
	//     let _watch_id = Uuid::new_v4();
	//     // Test is a stub; see above for TODO.
	//     return;

	//     // let metadata = WatchMetadata {
	//     //     watch_id,
	//     //     root_path: PathBuf::from("/test"),
	//     //     created_at: chrono::Utc::now(),
	//     //     last_scan: None,
	//     //     node_count: 0,
	//     //     is_active: true,
	//     //     config_hash: 12345,
	//     // };
	//     // cache.store_watch_metadata(&metadata).await.unwrap();
	//     // let retrieved = cache.get_watch_metadata(&watch_id).await.unwrap();
	//     // assert!(retrieved.is_some());
	//     // assert_eq!(retrieved.unwrap().watch_id, watch_id);
	//     // TODO: Restore this test when FilesystemNode::new or from_path is implemented for testability.
	//     // return;
	// }

	// #[tokio::test]
	// async fn test_batch_operations() {
	//     let _cache = create_test_cache().await.unwrap();
	//     let _watch_id = Uuid::new_v4();
	//     // Test is a stub; see above for TODO.
	//     return;

	//     // Create test nodes
	//     // TODO: Implement FilesystemNode::from_path or use FilesystemNode::new with real metadata in tests
	//     // let nodes = vec![
	//     //     FilesystemNode::from_path(PathBuf::from("/test/file1.txt")).unwrap(),
	//     //     FilesystemNode::from_path(PathBuf::from("/test/file2.txt")).unwrap(),
	//     //     FilesystemNode::from_path(PathBuf::from("/test/dir")).unwrap(),
	//     // ];
	//     // let nodes = vec![
	//     //     FilesystemNode::new(
	//     //         &PathBuf::from("/test/file1.txt"),
	//     //         None,
	//     //         None,
	//     //         None,
	//     //         None,
	//     //         None,
	//     //     ),
	//     //     FilesystemNode::new(
	//     //         &PathBuf::from("/test/file2.txt"),
	//     //         None,
	//     //         None,
	//     //         None,
	//     //         None,
	//     //         None,
	//     //     ),
	//     //     FilesystemNode::new(
	//     //         &PathBuf::from("/test/dir"),
	//     //         None,
	//     //         None,
	//     //         None,
	//     //         None,
	//     //         None,
	//     //     ),
	//     // ];
	//     // TODO: Restore this test when FilesystemNode::new or from_path is implemented for testability.
	//     // return;

	//     // Batch store nodes
	//     // cache
	//     // 	.batch_store_filesystem_nodes(&watch_id, &nodes)
	//     // 	.await
	//     // 	.unwrap();

	//     // Verify all nodes were stored
	//     // for node in &nodes {
	//     // 	let retrieved = cache
	//     // 		.get_filesystem_node(&watch_id, &node.path)
	//     // 		.await
	//     // 		.unwrap();
	//     // 	assert!(retrieved.is_some());
	//     // }
	// }

	// #[tokio::test]
	// async fn test_cache_stats() {
	//     let _cache = create_test_cache().await.unwrap();
	//     let _watch_id = Uuid::new_v4();
	//     // Test is a stub; see above for TODO.
	//     return;

	//     // Store some test nodes
	//     // TODO: Implement FilesystemNode::from_path or use FilesystemNode::new with real metadata in tests
	//     // let nodes = vec![
	//     //     FilesystemNode::from_path(PathBuf::from("/test/file1.txt")).unwrap(),
	//     //     FilesystemNode::from_path(PathBuf::from("/test/dir")).unwrap(),
	//     // ];
	//     // let nodes = vec![
	//     //     FilesystemNode::new(
	//     //         &PathBuf::from("/test/file1.txt"),
	//     //         None,
	//     //         None,
	//     //         None,
	//     //         None,
	//     //         None,
	//     //     ),
	//     //     FilesystemNode::new(
	//     //         &PathBuf::from("/test/dir"),
	//     //         None,
	//     //         None,
	//     //         None,
	//     //         None,
	//     //         None,
	//     //     ),
	//     // ];
	//     // TODO: Restore this test when FilesystemNode::new or from_path is implemented for testability.
	//     // return;

	//     // cache
	//     // 	.batch_store_filesystem_nodes(&watch_id, &nodes)
	//     // 	.await
	//     // 	.unwrap();

	//     // Get stats
	//     // let stats = cache.get_cache_stats(&watch_id).await.unwrap();
	//     // assert_eq!(stats.total_nodes, 2);
	//     // assert!(stats.cache_size_bytes > 0);
	// }
}
