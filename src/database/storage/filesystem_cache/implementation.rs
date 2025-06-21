//! Implementation of filesystem cache storage using ReDB
//!
//! This file contains only the RedbFilesystemCache struct and its impls.
//! All helpers are imported from utils.rs.
//!
//! Limitations:
//! - Naive search and traversal (O(N)), not suitable for very large datasets.
//! - No alternative backends implemented yet.
//!
//! TODO: Refactor search to use indexed or batched queries for production use.

use super::utils::{deserialize, key_to_bytes, serialize};
use crate::database::error::DatabaseResult;
use crate::database::storage::tables::{
	MULTI_WATCH_FS_CACHE, MULTI_WATCH_HIERARCHY, PATH_PREFIX_TABLE, PATH_TO_WATCHES, SHARED_NODES,
	WATCH_REGISTRY,
};
use crate::database::types::{
	calculate_path_hash, FilesystemNode, SharedNodeInfo, WatchMetadata, WatchScopedKey,
};
use globset::Glob;
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

use super::trait_def::{CacheStats, FilesystemCacheStorage};
use redb::{ReadableMultimapTable, ReadableTable};
use tracing::{debug, info};

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
		WatchScopedKey { watch_id: *watch_id, path_hash }
	}

	#[allow(dead_code)]
	/// Create a shared key for cross-watch nodes
	fn create_shared_key(path_hash: u64) -> u64 {
		path_hash
	}

	/// Helper: insert all path prefixes for a node into PATH_PREFIX_TABLE
	fn index_path_prefixes(
		write_txn: &redb::WriteTransaction, node: &FilesystemNode, watch_id: &Uuid,
	) -> DatabaseResult<()> {
		let mut prefix_table = write_txn.open_multimap_table(PATH_PREFIX_TABLE)?;
		let path = &node.path;
		let path_hash = calculate_path_hash(path);
		let scoped_key = Self::create_scoped_key(watch_id, path_hash);
		let key_bytes = key_to_bytes(&scoped_key);
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
		&mut self, watch_id: &Uuid, node: &FilesystemNode,
	) -> DatabaseResult<()> {
		// Canonicalize the path for the key only; do not mutate the node's path
		let canonical = match node.path.canonicalize() {
			Ok(p) => p,
			Err(_) => node.path.clone(),
		};
		let path_hash = calculate_path_hash(&canonical);
		let scoped_key = Self::create_scoped_key(watch_id, path_hash);
		let key_bytes = serialize(&scoped_key)?;
		let node_bytes = serialize(node)?;

		let write_txn = self.database.begin_write()?;
		{
			// Store the node
			let mut fs_cache_table = write_txn.open_table(MULTI_WATCH_FS_CACHE)?;
			fs_cache_table.insert(key_bytes.as_slice(), node_bytes.as_slice())?;

			// Update hierarchy relationships
			if let Some(parent_hash) = node.computed.parent_hash {
				let parent_key = Self::create_scoped_key(watch_id, parent_hash);
				let parent_key_bytes = serialize(&parent_key)?;
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
		if cfg!(debug_assertions) {
			let stats = self.get_cache_stats(watch_id).await.unwrap_or_default();
			info!(
				"[DEBUG] After insert: nodes={}, dirs={}, files={}, symlinks={}, size={} bytes",
				stats.total_nodes,
				stats.directories,
				stats.files,
				stats.symlinks,
				stats.cache_size_bytes
			);
		}
		Ok(())
	}

	async fn get_filesystem_node(
		&mut self, watch_id: &Uuid, path: &Path,
	) -> DatabaseResult<Option<FilesystemNode>> {
		// Canonicalize the path for the key, matching store_filesystem_node
		let canonical = match path.canonicalize() {
			Ok(p) => p,
			Err(_) => path.to_path_buf(),
		};
		let read_txn = self.database.begin_read()?;
		let fs_cache_table = read_txn.open_table(MULTI_WATCH_FS_CACHE)?;
		let path_hash = calculate_path_hash(&canonical);
		let scoped_key = Self::create_scoped_key(watch_id, path_hash);
		let key_bytes = serialize(&scoped_key)?;
		let result = match fs_cache_table.get(key_bytes.as_slice())? {
			Some(bytes) => Some(deserialize(bytes.value())?),
			None => None,
		};
		Ok(result)
	}

	async fn list_directory_for_watch(
		&mut self, watch_id: &Uuid, parent_path: &Path,
	) -> DatabaseResult<Vec<FilesystemNode>> {
		let parent_hash = calculate_path_hash(parent_path);
		let parent_key = Self::create_scoped_key(watch_id, parent_hash);
		let parent_key_bytes = serialize(&parent_key)?;

		let read_txn = self.database.begin_read()?;
		let hierarchy_table = read_txn.open_multimap_table(MULTI_WATCH_HIERARCHY)?;
		let fs_cache_table = read_txn.open_table(MULTI_WATCH_FS_CACHE)?;

		let mut nodes = Vec::new();

		// Get all child keys for this parent
		let child_iter = hierarchy_table.get(parent_key_bytes.as_slice())?;
		for child_key_result in child_iter {
			let child_key = child_key_result?;
			if let Some(node_bytes) = fs_cache_table.get(child_key.value())? {
				let node: FilesystemNode = deserialize(node_bytes.value())?;
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
			watch_registry.insert(key.as_slice(), serialize(metadata)?.as_slice())?;
		}
		write_txn.commit()?;
		Ok(())
	}

	async fn get_watch_metadata(
		&mut self, watch_id: &Uuid,
	) -> DatabaseResult<Option<WatchMetadata>> {
		let read_txn = self.database.begin_read()?;
		let watch_registry = read_txn.open_table(WATCH_REGISTRY)?;
		let key = watch_id.as_bytes();
		let result = match watch_registry.get(key.as_slice())? {
			Some(bytes) => Some(deserialize(bytes.value())?),
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
			shared_table.insert(key.as_slice(), serialize(shared_info)?.as_slice())?;
		}
		write_txn.commit()?;
		Ok(())
	}

	async fn get_shared_node(&mut self, path_hash: u64) -> DatabaseResult<Option<SharedNodeInfo>> {
		let read_txn = self.database.begin_read()?;
		let shared_table = read_txn.open_table(SHARED_NODES)?;
		let key = path_hash.to_le_bytes();
		let result = match shared_table.get(key.as_slice())? {
			Some(bytes) => Some(deserialize(bytes.value())?),
			None => None,
		};
		Ok(result)
	}

	async fn batch_store_filesystem_nodes(
		&mut self, watch_id: &Uuid, nodes: &[FilesystemNode],
	) -> DatabaseResult<()> {
		let write_txn = self.database.begin_write()?;
		{
			let mut fs_cache_table = write_txn.open_table(MULTI_WATCH_FS_CACHE)?;
			for node in nodes {
				let path_hash = calculate_path_hash(&node.path);
				let scoped_key = Self::create_scoped_key(watch_id, path_hash);
				let key_bytes = serialize(&scoped_key)?;
				fs_cache_table.insert(key_bytes.as_slice(), serialize(node)?.as_slice())?;
				Self::index_path_prefixes(&write_txn, node, watch_id)?;
			}
		}
		write_txn.commit()?;
		Ok(())
	}

	async fn find_nodes_by_prefix(
		&mut self, _watch_id: &Uuid, prefix: &Path,
	) -> DatabaseResult<Vec<FilesystemNode>> {
		let read_txn = self.database.begin_read()?;
		let path_prefix_table = read_txn.open_multimap_table(PATH_PREFIX_TABLE)?;
		let prefix_str = prefix.to_string_lossy();
		let mut result = Vec::new();
		for entry in path_prefix_table.get(prefix_str.as_bytes())? {
			let entry = entry?;
			let value = entry.value();
			let node: FilesystemNode = deserialize(value)?;
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
		let mut total_bytes = 0u64;
		let start = std::time::Instant::now();
		for entry in fs_cache_table.iter()? {
			let (key, value) = entry?;
			total_bytes += key.value().len() as u64 + value.value().len() as u64;
			let node: FilesystemNode = deserialize(value.value())?;
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
		stats.cache_size_bytes = total_bytes;
		stats.last_updated = chrono::Utc::now();
		let elapsed = start.elapsed();
		debug!("Cache stats collected: nodes={}, dirs={}, files={}, symlinks={}, size={} bytes, elapsed={:?}",
			total_nodes, directories, files, symlinks, total_bytes, elapsed);
		Ok(stats)
	}

	async fn cleanup_stale_cache(
		&mut self, _watch_id: &Uuid, max_age_seconds: u64,
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
				let node: FilesystemNode = deserialize(value.value())?;
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

	async fn list_directory_unified(
		&mut self, parent_path: &Path,
	) -> DatabaseResult<Vec<FilesystemNode>> {
		// Aggregate all direct children of parent_path across all watches and shared nodes
		let read_txn = self.database.begin_read()?;
		let hierarchy_table = read_txn.open_multimap_table(MULTI_WATCH_HIERARCHY)?;
		let fs_cache_table = read_txn.open_table(MULTI_WATCH_FS_CACHE)?;
		let watch_registry = read_txn.open_table(WATCH_REGISTRY)?;
		let mut children = Vec::new();
		let mut seen_paths = std::collections::HashSet::new();

		let parent_hash = calculate_path_hash(parent_path);

		// 1. Scan all watches for direct children
		for entry in watch_registry.iter()? {
			let (watch_id_guard, _) = entry?;
			let watch_id_bytes = watch_id_guard.value();
			if watch_id_bytes.len() != 16 {
				continue;
			}
			let watch_id = match uuid::Uuid::from_slice(watch_id_bytes) {
				Ok(id) => id,
				Err(_) => continue, // skip corrupt entries
			};
			let parent_key = Self::create_scoped_key(&watch_id, parent_hash);
			let parent_key_bytes = serialize(&parent_key)?;
			if let Ok(mut child_iter) = hierarchy_table.get(parent_key_bytes.as_slice()) {
				for child_key_result in &mut child_iter {
					let child_key = match child_key_result {
						Ok(guard) => guard.value().to_vec(),
						Err(_) => continue,
					};
					if let Some(node_bytes) = fs_cache_table.get(child_key.as_slice())? {
						let node: FilesystemNode = match deserialize(node_bytes.value()) {
							Ok(n) => n,
							Err(_) => continue,
						};
						if seen_paths.insert(node.path.clone()) {
							children.push(node);
						}
					}
				}
			}
		}

		// 2. Also scan shared nodes for this parent (legacy support, if any)
		let shared_key_bytes = serialize(&parent_hash)?;
		if let Ok(mut values) = hierarchy_table.get(shared_key_bytes.as_slice()) {
			for result in &mut values {
				let access_guard = match result {
					Ok(guard) => guard,
					Err(_) => continue,
				};
				let child_key_bytes = access_guard.value();
				if let Some(child_node_bytes) = fs_cache_table.get(child_key_bytes)? {
					let child_node: FilesystemNode = match deserialize(child_node_bytes.value()) {
						Ok(node) => node,
						Err(_) => continue,
					};
					if seen_paths.insert(child_node.path.clone()) {
						children.push(child_node);
					}
				}
			}
		}
		Ok(children)
	}

	async fn get_unified_node(&mut self, path: &Path) -> DatabaseResult<Option<FilesystemNode>> {
		// Prefer shared node if present, else any watch-specific node
		let path_hash = calculate_path_hash(path);
		if let Some(shared) = self.get_shared_node(path_hash).await? {
			return Ok(Some(shared.node));
		}
		// Fallback: scan all watches for a node
		let read_txn = self.database.begin_read()?;
		let fs_cache_table = read_txn.open_table(MULTI_WATCH_FS_CACHE)?;
		for entry in fs_cache_table.iter()? {
			let (_key, value) = entry?;
			let node: FilesystemNode = deserialize(value.value())?;
			if node.path == path {
				return Ok(Some(node));
			}
		}
		Ok(None)
	}

	// List all ancestor nodes for a given path (up to root).
	async fn list_ancestors(&mut self, path: &Path) -> DatabaseResult<Vec<FilesystemNode>> {
		let mut ancestors = Vec::new();
		let mut current_path = path.to_path_buf();
		let mut seen_hashes = std::collections::HashSet::new();
		while let Some(node) = self.get_unified_node(&current_path).await? {
			let parent_hash = node.computed.parent_hash;
			if let Some(hash) = parent_hash {
				if !seen_hashes.insert(hash) {
					// Defensive: cycle detected
					break;
				}
				// Find parent node by hash (scan all nodes, not efficient, but safe for now)
				let parent_node = {
					let read_txn = self.database.begin_read()?;
					let fs_cache_table = read_txn.open_table(MULTI_WATCH_FS_CACHE)?;
					let mut found = None;
					for entry in fs_cache_table.iter()? {
						let (_key, value) = entry?;
						let candidate: FilesystemNode = deserialize(value.value())?;
						if candidate.computed.path_hash == hash {
							found = Some(candidate);
							break;
						}
					}
					found
				};
				if let Some(parent) = parent_node {
					ancestors.push(parent.clone());
					current_path = parent.path.clone();
				} else {
					break;
				}
			} else {
				break;
			}
		}
		Ok(ancestors)
	}

	// List all descendant nodes for a given path (subtree query).
	async fn list_descendants(&mut self, path: &Path) -> DatabaseResult<Vec<FilesystemNode>> {
		let mut descendants = Vec::new();
		let mut stack = vec![path.to_path_buf()];
		let mut seen = std::collections::HashSet::new();
		while let Some(current) = stack.pop() {
			let children = self.list_directory_unified(&current).await?;
			for child in children {
				if seen.insert(child.computed.path_hash) {
					descendants.push(child.clone());
					if matches!(
						child.node_type,
						crate::database::types::NodeType::Directory { .. }
					) {
						stack.push(child.path.clone());
					}
				}
			}
		}
		Ok(descendants)
	}

	/// Pattern-based search for nodes (e.g., glob, regex).
	///
	/// Returns all nodes matching the given pattern.
	/// This implementation is naive and scans all nodes. Performance will degrade with large caches.
	/// TODO: Replace with indexed or batched search for production use.
	async fn search_nodes(&mut self, pattern: &str) -> DatabaseResult<Vec<FilesystemNode>> {
		// WARNING: This implementation is a naive full scan. Performance will degrade with large caches.
		// TODO: Replace with indexed or batched search for production use.
		let glob = Glob::new(pattern)
			.map_err(|e| crate::database::error::DatabaseError::Other(e.to_string()))?;
		let matcher = glob.compile_matcher();
		let read_txn = self.database.begin_read()?;
		let fs_cache_table = read_txn.open_table(MULTI_WATCH_FS_CACHE)?;
		let mut results = Vec::new();
		for entry in fs_cache_table.iter()? {
			let (_key, value) = entry?;
			let node: FilesystemNode = deserialize(value.value())?;
			let file_name = node.path.file_name().unwrap_or_default().to_string_lossy();
			if matcher.is_match(file_name.as_ref()) {
				results.push(node);
			}
		}
		Ok(results)
	}

	async fn get_node(
		&mut self, watch_id: &Uuid, path: &Path,
	) -> DatabaseResult<Option<FilesystemNode>> {
		// Normalize the path to ensure consistent lookups across platforms
		let canonical = match path.canonicalize() {
			Ok(p) => p,
			Err(_) => path.to_path_buf(), // Fallback: use as-is if canonicalization fails
		};
		let node = self.get_filesystem_node(watch_id, &canonical).await?;
		// If the node is present, check if it is stale (needs refresh)
		if let Some(ref n) = node {
			// 1 hour is a placeholder; in production, make this configurable
			if n.needs_refresh(std::time::Duration::from_secs(3600)) {
				// Node is stale; treat as missing for now
				return Ok(None);
			}
		}
		Ok(node)
	}

	async fn remove_filesystem_node(&mut self, watch_id: &Uuid, path: &Path) -> DatabaseResult<()> {
		// Remove the node from the cache and update hierarchy/path indices.
		let canonical = match path.canonicalize() {
			Ok(p) => p,
			Err(_) => path.to_path_buf(),
		};
		let path_hash = calculate_path_hash(&canonical);
		let scoped_key = Self::create_scoped_key(watch_id, path_hash);
		let key_bytes = serialize(&scoped_key)?;
		let write_txn = self.database.begin_write()?;
		{
			let mut fs_cache_table = write_txn.open_table(MULTI_WATCH_FS_CACHE)?;
			fs_cache_table.remove(key_bytes.as_slice())?;
			// Remove from hierarchy table (as child)
			let mut hierarchy_table = write_txn.open_multimap_table(MULTI_WATCH_HIERARCHY)?;
			// Remove all parent->child links to this node
			// (Naive: scan all parents, remove child links)
			let mut to_remove: Vec<Vec<u8>> = Vec::new();
			for entry in hierarchy_table.iter()? {
				let (parent_key, child_key) = entry?;
				// child_key is a MultimapValue<'_, &[u8]>
				for child in child_key {
					let child = child?; // AccessGuard<'_, &[u8]>
					if child.value() == key_bytes.as_slice() {
						to_remove.push(parent_key.value().to_vec());
					}
				}
			}
			for parent in to_remove {
				hierarchy_table.remove(parent.as_slice(), key_bytes.as_slice())?;
			}
			// Remove from path prefix table
			let mut prefix_table = write_txn.open_multimap_table(PATH_PREFIX_TABLE)?;
			let prefix_str = canonical.to_string_lossy();
			prefix_table.remove(prefix_str.as_bytes(), key_bytes.as_slice())?;
		}
		write_txn.commit()?;
		Ok(())
	}

	async fn rename_filesystem_node(
		&mut self, watch_id: &Uuid, old_path: &Path, new_path: &Path,
	) -> DatabaseResult<()> {
		// Move the node, update indices, handle parent/child relationships.
		let old_canonical = match old_path.canonicalize() {
			Ok(p) => p,
			Err(_) => old_path.to_path_buf(),
		};
		let new_canonical = match new_path.canonicalize() {
			Ok(p) => p,
			Err(_) => new_path.to_path_buf(),
		};
		let old_hash = calculate_path_hash(&old_canonical);
		let new_hash = calculate_path_hash(&new_canonical);
		let old_key = Self::create_scoped_key(watch_id, old_hash);
		let new_key = Self::create_scoped_key(watch_id, new_hash);
		let old_key_bytes = serialize(&old_key)?;
		let new_key_bytes = serialize(&new_key)?;
		let write_txn = self.database.begin_write()?;
		{
			let mut fs_cache_table = write_txn.open_table(MULTI_WATCH_FS_CACHE)?;
			// Scope the immutable borrow to avoid borrow checker issues
			let node_opt = {
				if let Some(node_bytes) = fs_cache_table.get(old_key_bytes.as_slice())? {
					let mut node: FilesystemNode = deserialize(node_bytes.value())?;
					node.path = new_canonical.clone();
					node.computed.path_hash = new_hash;
					Some(node)
				} else {
					None
				}
			};
			if let Some(node) = node_opt {
				fs_cache_table.insert(new_key_bytes.as_slice(), serialize(&node)?.as_slice())?;
				fs_cache_table.remove(old_key_bytes.as_slice())?;
				// Update hierarchy: remove old parent->child, add new parent->child
				let mut hierarchy_table = write_txn.open_multimap_table(MULTI_WATCH_HIERARCHY)?;
				let mut to_remove = Vec::new();
				for entry in hierarchy_table.iter()? {
					let (parent_key, child_key) = entry?;
					for child in child_key {
						let child = child?;
						if child.value() == old_key_bytes.as_slice() {
							to_remove.push(parent_key.value().to_vec());
						}
					}
				}
				for parent in to_remove {
					hierarchy_table.remove(parent.as_slice(), old_key_bytes.as_slice())?;
					hierarchy_table.insert(parent.as_slice(), new_key_bytes.as_slice())?;
				}
				// Update path prefix table: remove old, add new
				let mut prefix_table = write_txn.open_multimap_table(PATH_PREFIX_TABLE)?;
				let old_prefix_str = old_canonical.to_string_lossy();
				let new_prefix_str = new_canonical.to_string_lossy();
				prefix_table.remove(old_prefix_str.as_bytes(), old_key_bytes.as_slice())?;
				prefix_table.insert(new_prefix_str.as_bytes(), new_key_bytes.as_slice())?;
			}
		}
		write_txn.commit()?;
		Ok(())
	}
}
