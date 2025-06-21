//! Implementation of filesystem cache storage using ReDB

use crate::database::error::DatabaseResult;
use crate::database::storage::tables::{
	MULTI_WATCH_FS_CACHE, MULTI_WATCH_HIERARCHY, PATH_PREFIX_TABLE, PATH_TO_WATCHES, SHARED_NODES,
	WATCH_REGISTRY,
};
use crate::database::types::{
	calculate_path_hash, FilesystemNode, SharedNodeInfo, WatchMetadata, WatchScopedKey,
};
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

use super::trait_def::{CacheStats, FilesystemCacheStorage};
use redb::ReadableTable;

pub struct RedbFilesystemCache {
	database: Arc<redb::Database>,
}

impl RedbFilesystemCache {
	pub fn new(database: Arc<redb::Database>) -> Self {
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
		let key_bytes = Self::serialize(&scoped_key)?;
		let node_bytes = Self::serialize(node)?;

		let write_txn = self.database.begin_write()?;
		{
			// Store the node
			let mut fs_cache_table = write_txn.open_table(MULTI_WATCH_FS_CACHE)?;
			fs_cache_table.insert(key_bytes.as_slice(), node_bytes.as_slice())?;

			// Update hierarchy relationships
			if let Some(parent_hash) = node.computed.parent_hash {
				let parent_key = Self::create_scoped_key(watch_id, parent_hash);
				let parent_key_bytes = Self::serialize(&parent_key)?;
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
		let read_txn = self.database.begin_read()?;
		let fs_cache_table = read_txn.open_table(MULTI_WATCH_FS_CACHE)?;
		let path_hash = calculate_path_hash(path);
		let scoped_key = Self::create_scoped_key(watch_id, path_hash);
		let key_bytes = Self::serialize(&scoped_key)?;
		let result = match fs_cache_table.get(key_bytes.as_slice())? {
			Some(bytes) => Some(Self::deserialize(bytes.value())?),
			None => None,
		};
		Ok(result)
	}

	async fn list_directory_for_watch(
		&mut self,
		watch_id: &Uuid,
		parent_path: &Path,
	) -> DatabaseResult<Vec<FilesystemNode>> {
		let parent_hash = calculate_path_hash(parent_path);
		let parent_key = Self::create_scoped_key(watch_id, parent_hash);
		let parent_key_bytes = Self::serialize(&parent_key)?;

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
		let write_txn = self.database.begin_write()?;
		{
			let mut watch_registry = write_txn.open_table(WATCH_REGISTRY)?;
			let key = metadata.watch_id.as_bytes();
			watch_registry.insert(key.as_slice(), Self::serialize(metadata)?.as_slice())?;
		}
		write_txn.commit()?;
		Ok(())
	}

	async fn get_watch_metadata(
		&mut self,
		watch_id: &Uuid,
	) -> DatabaseResult<Option<WatchMetadata>> {
		let read_txn = self.database.begin_read()?;
		let watch_registry = read_txn.open_table(WATCH_REGISTRY)?;
		let key = watch_id.as_bytes();
		let result = match watch_registry.get(key.as_slice())? {
			Some(bytes) => Some(Self::deserialize(bytes.value())?),
			None => None,
		};
		Ok(result)
	}

	async fn remove_watch(&mut self, watch_id: &Uuid) -> DatabaseResult<()> {
		let write_txn = self.database.begin_write()?;
		{
			let mut watch_registry = write_txn.open_table(WATCH_REGISTRY)?;
			let key = watch_id.as_bytes();
			watch_registry.remove(key.as_slice())?;
		}
		write_txn.commit()?;
		Ok(())
	}

	async fn store_shared_node(&mut self, shared_info: &SharedNodeInfo) -> DatabaseResult<()> {
		let write_txn = self.database.begin_write()?;
		{
			let mut shared_table = write_txn.open_table(SHARED_NODES)?;
			let key = shared_info.node.computed.path_hash.to_le_bytes();
			shared_table.insert(key.as_slice(), Self::serialize(shared_info)?.as_slice())?;
		}
		write_txn.commit()?;
		Ok(())
	}

	async fn get_shared_node(&mut self, path_hash: u64) -> DatabaseResult<Option<SharedNodeInfo>> {
		let read_txn = self.database.begin_read()?;
		let shared_table = read_txn.open_table(SHARED_NODES)?;
		let key = path_hash.to_le_bytes();
		let result = match shared_table.get(key.as_slice())? {
			Some(bytes) => Some(Self::deserialize(bytes.value())?),
			None => None,
		};
		Ok(result)
	}

	async fn batch_store_filesystem_nodes(
		&mut self,
		watch_id: &Uuid,
		nodes: &[FilesystemNode],
	) -> DatabaseResult<()> {
		let write_txn = self.database.begin_write()?;
		{
			let mut fs_cache_table = write_txn.open_table(MULTI_WATCH_FS_CACHE)?;
			for node in nodes {
				let path_hash = calculate_path_hash(&node.path);
				let scoped_key = Self::create_scoped_key(watch_id, path_hash);
				let key_bytes = Self::serialize(&scoped_key)?;
				fs_cache_table.insert(key_bytes.as_slice(), Self::serialize(node)?.as_slice())?;
				Self::index_path_prefixes(&write_txn, node, watch_id)?;
			}
		}
		write_txn.commit()?;
		Ok(())
	}

	async fn find_nodes_by_prefix(
		&mut self,
		_watch_id: &Uuid,
		prefix: &Path,
	) -> DatabaseResult<Vec<FilesystemNode>> {
		let read_txn = self.database.begin_read()?;
		let path_prefix_table = read_txn.open_multimap_table(PATH_PREFIX_TABLE)?;
		let prefix_str = prefix.to_string_lossy();
		let mut result = Vec::new();
		for entry in path_prefix_table.get(prefix_str.as_bytes())? {
			let entry = entry?;
			let value = entry.value();
			let node: FilesystemNode = Self::deserialize(value)?;
			result.push(node);
		}
		Ok(result)
	}

	async fn get_cache_stats(&mut self, _watch_id: &Uuid) -> DatabaseResult<CacheStats> {
		let read_txn = self.database.begin_read()?;
		let fs_cache_table = read_txn.open_table(MULTI_WATCH_FS_CACHE)?;
		let mut stats = CacheStats::default();
		let mut total_nodes = 0;
		let mut directories = 0;
		let mut files = 0;
		let mut symlinks = 0;
		for entry in fs_cache_table.iter()? {
			let (_key, value) = entry?;
			let node: FilesystemNode = Self::deserialize(value.value())?;
			total_nodes += 1;
			match node.node_type {
				crate::database::types::NodeType::Directory { .. } => directories += 1,
				crate::database::types::NodeType::File { .. } => files += 1,
				crate::database::types::NodeType::Symlink { .. } => symlinks += 1,
			}
		}
		stats.total_nodes = total_nodes;
		stats.directories = directories;
		stats.files = files;
		stats.symlinks = symlinks;
		stats.last_updated = chrono::Utc::now();
		Ok(stats)
	}

	async fn cleanup_stale_cache(
		&mut self,
		_watch_id: &Uuid,
		max_age_seconds: u64,
	) -> DatabaseResult<usize> {
		let write_txn = self.database.begin_write()?;
		let mut deleted_count = 0;
		{
			let mut fs_cache_table = write_txn.open_table(MULTI_WATCH_FS_CACHE)?;
			let now = chrono::Utc::now().timestamp() as u64;
			// Collect keys to delete to avoid borrowing issues
			let mut keys_to_delete = Vec::new();
			for entry in fs_cache_table.iter()? {
				let (key, value) = entry?;
				let node: FilesystemNode = Self::deserialize(value.value())?;
				let cached_at = node.cache_info.cached_at.timestamp() as u64;
				if now - cached_at > max_age_seconds {
					// Clone the key for removal after iteration
					keys_to_delete.push(key.value().to_vec());
				}
			}
			for key in keys_to_delete {
				fs_cache_table.remove(key.as_slice())?;
				deleted_count += 1;
			}
		}
		write_txn.commit()?;
		Ok(deleted_count)
	}
}

// ...move serialization notes and tests to a separate file or keep #[cfg(test)] here if small...
