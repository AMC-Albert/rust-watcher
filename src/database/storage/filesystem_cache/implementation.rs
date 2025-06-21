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
use crate::database::storage::filesystem_cache::utils;
use crate::database::storage::tables::{
	MULTI_WATCH_FS_CACHE, MULTI_WATCH_HIERARCHY, PATH_PREFIX_TABLE, PATH_STATS, SHARED_NODES,
	STATS_TABLE, WATCH_REGISTRY, WATCH_STATS,
};
use crate::database::types::{
	calculate_path_hash, FilesystemNode, SharedNodeInfo, WatchMetadata, WatchScopedKey,
};
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

use super::trait_def::{CacheStats, FilesystemCacheStorage};
use crate::database::storage::filesystem_cache::watch_mapping::WatchMappingHelpers;
use redb::{ReadableMultimapTable, ReadableTable};
use tracing::{debug, info};

pub struct RedbFilesystemCache {
	pub(crate) database: Arc<redb::Database>,
}

impl RedbFilesystemCache {
	pub fn new(database: Arc<redb::Database>) -> Self {
		Self { database }
	}

	/// Initialize the filesystem cache tables
	pub async fn initialize(&mut self) -> DatabaseResult<()> {
		// This function is a stub for initializing tables if needed.
		// In this implementation, tables are created on first use by Redb.
		Ok(())
	}

	/// Create a scoped key for a watch-specific node
	pub fn create_scoped_key(watch_id: &Uuid, path_hash: u64) -> WatchScopedKey {
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
		&mut self, watch_id: &Uuid, node: &FilesystemNode, event_type: &str,
	) -> DatabaseResult<()> {
		// Canonicalize the path for the key only; do not mutate the node's path
		let canonical = match node.path.canonicalize() {
			Ok(p) => p,
			Err(_) => node.path.clone(),
		};
		let path_hash = calculate_path_hash(&canonical);
		let scoped_key = Self::create_scoped_key(watch_id, path_hash);
		let key_bytes = serialize(&scoped_key)?;

		// Always update last_event_type before storing
		let mut node = node.clone();
		node.last_event_type = Some(event_type.to_string());
		let node_bytes = serialize(&node)?;

		let mut write_txn = self.database.begin_write()?;
		{
			// Store the node
			{
				let mut fs_cache_table = write_txn.open_table(MULTI_WATCH_FS_CACHE)?;
				fs_cache_table.insert(key_bytes.as_slice(), node_bytes.as_slice())?;

				// Update hierarchy relationships
				if let Some(parent_hash) = node.computed.parent_hash {
					let parent_key = Self::create_scoped_key(watch_id, parent_hash);
					let parent_key_bytes = serialize(&parent_key)?;
					let mut hierarchy_table =
						write_txn.open_multimap_table(MULTI_WATCH_HIERARCHY)?;
					hierarchy_table.insert(parent_key_bytes.as_slice(), key_bytes.as_slice())?;
				}

				// Update path-to-watches mapping
				WatchMappingHelpers::insert_watch_mapping(&write_txn, path_hash, watch_id)?;

				// Update path prefix index
				Self::index_path_prefixes(&write_txn, &node, watch_id)?;

				// Update unified node index for O(1) cross-watch lookup
				{
					let mut unified_index = write_txn
						.open_table(crate::database::storage::tables::UNIFIED_NODE_INDEX)?;
					let path_hash = node.computed.path_hash;
					let node_bytes =
						crate::database::storage::filesystem_cache::utils::serialize(&node)?;
					unified_index
						.insert(path_hash.to_le_bytes().as_slice(), node_bytes.as_slice())?;
				}
			} // <-- All table borrows dropped here

			// --- Incremental stats update: per-watch and per-path ---
			let all_watches = WatchMappingHelpers::get_watches_for_path(&self.database, path_hash)?;
			for wid in all_watches.iter() {
				crate::database::storage::filesystem_cache::stats::increment_stats(
					&mut write_txn,
					wid,
					path_hash,
					event_type,
				)?;
			}
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
		&mut self, watch_id: &Uuid, nodes: &[FilesystemNode], _event_type: &str,
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
				// Optionally update stats here if needed, using _event_type
			}
		} // fs_cache_table dropped here
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
		// Prefer shared node if present, else use unified node index for O(1) lookup
		let path_hash = calculate_path_hash(path);
		if let Some(shared) = self.get_shared_node(path_hash).await? {
			return Ok(Some(shared.node));
		}
		// Use unified node index for O(1) lookup
		let read_txn = self.database.begin_read()?;
		let unified_index =
			read_txn.open_table(crate::database::storage::tables::UNIFIED_NODE_INDEX)?;
		if let Some(node_bytes) = unified_index.get(path_hash.to_le_bytes().as_slice())? {
			let node: FilesystemNode = utils::deserialize(node_bytes.value())?;
			return Ok(Some(node));
		}
		Ok(None)
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

	async fn remove_filesystem_node(
		&mut self, watch_id: &Uuid, path: &Path, event_type: &str,
	) -> DatabaseResult<()> {
		// Remove the node from the cache and update hierarchy/path indices.
		let canonical = match path.canonicalize() {
			Ok(p) => p,
			Err(_) => path.to_path_buf(),
		};
		let path_hash = calculate_path_hash(&canonical);
		let scoped_key = Self::create_scoped_key(watch_id, path_hash);
		let key_bytes = serialize(&scoped_key)?;
		let mut write_txn = self.database.begin_write()?;
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
			// All table borrows dropped here
		}
		// --- Incremental stats update: per-watch and per-path (removal) ---
		let all_watches = WatchMappingHelpers::get_watches_for_path(&self.database, path_hash)?;
		for wid in all_watches.iter() {
			crate::database::storage::filesystem_cache::stats::decrement_stats(
				&mut write_txn,
				wid,
				path_hash,
				event_type,
			)?;
		}
		write_txn.commit()?;
		Ok(())
	}

	async fn rename_filesystem_node(
		&mut self, watch_id: &Uuid, old_path: &Path, new_path: &Path, event_type: &str,
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
		let old_scoped_key = Self::create_scoped_key(watch_id, old_hash);
		let new_scoped_key = Self::create_scoped_key(watch_id, new_hash);
		let old_key_bytes = serialize(&old_scoped_key)?;
		let new_key_bytes = serialize(&new_scoped_key)?;
		let mut write_txn = self.database.begin_write()?;
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
		// --- Incremental stats update: per-watch and per-path (move/rename) ---
		let old_all_watches = WatchMappingHelpers::get_watches_for_path(&self.database, old_hash)?;
		for wid in old_all_watches.iter() {
			crate::database::storage::filesystem_cache::stats::decrement_stats(
				&mut write_txn,
				wid,
				old_hash,
				event_type,
			)?;
		}
		let new_all_watches = WatchMappingHelpers::get_watches_for_path(&self.database, new_hash)?;
		for wid in new_all_watches.iter() {
			crate::database::storage::filesystem_cache::stats::increment_stats(
				&mut write_txn,
				wid,
				new_hash,
				event_type,
			)?;
		}
		write_txn.commit()?;
		Ok(())
	}

	async fn search_nodes(&mut self, pattern: &str) -> DatabaseResult<Vec<FilesystemNode>> {
		use globset::Glob;
		use std::ffi::OsStr;
		let read_txn = self.database.begin_read()?;
		let glob = Glob::new(pattern)
			.map_err(|e| crate::database::error::DatabaseError::Other(e.to_string()))?;
		let matcher = glob.compile_matcher();

		// If the pattern is a simple prefix (e.g., "foo*"), use PATH_PREFIX_TABLE for efficiency.
		// Otherwise, fall back to O(N) scan. This is a pragmatic compromise until a secondary index is implemented.
		let is_prefix = !pattern.contains('?')
			&& !pattern.contains('[')
			&& !pattern.contains(']')
			&& !pattern.contains('{')
			&& !pattern.contains('}')
			&& pattern.ends_with('*')
			&& !pattern[..pattern.len() - 1].contains('*');
		let mut results = Vec::new();
		if is_prefix {
			// Remove trailing '*' for prefix
			let prefix = &pattern[..pattern.len() - 1];
			let path_prefix_table = read_txn.open_multimap_table(PATH_PREFIX_TABLE)?;
			for entry in path_prefix_table.get(prefix.as_bytes())? {
				let entry = entry?;
				let value = entry.value();
				let node: FilesystemNode = utils::deserialize(value)?;
				// Defensive: still check the pattern in case of false positives
				if let Some(fname) = node.path.file_name().and_then(OsStr::to_str) {
					if matcher.is_match(fname) {
						results.push(node);
					}
				}
			}
		} else {
			// Fallback: O(N) scan over all nodes
			let fs_cache_table = read_txn.open_table(MULTI_WATCH_FS_CACHE)?;
			for entry in fs_cache_table.iter()? {
				let (_key, value) = entry?;
				let node: FilesystemNode = utils::deserialize(value.value())?;
				if let Some(fname) = node.path.file_name().and_then(OsStr::to_str) {
					if matcher.is_match(fname) {
						results.push(node);
					}
				}
			}
		}
		// NOTE: This does not support efficient suffix/infix search. For large datasets, a secondary index on canonical_name or extension is required.
		// TODO: Implement a file name or extension index if suffix/infix search is a production requirement.
		Ok(results)
	}
}

// End of FilesystemCacheStorage trait impl

// Inherent methods for RedbFilesystemCache
impl RedbFilesystemCache {
	/// Fallback: scan all watches for a node by path (O(N)). Used by shared node helpers.
	pub async fn scan_all_watches_for_node(
		&self, path: &std::path::Path,
	) -> DatabaseResult<Option<FilesystemNode>> {
		let read_txn = self.database.begin_read()?;
		let fs_cache_table = read_txn.open_table(MULTI_WATCH_FS_CACHE)?;
		for entry in fs_cache_table.iter()? {
			let (_key, value) = entry?;
			let node: FilesystemNode = utils::deserialize(value.value())?;
			if node.path == path {
				return Ok(Some(node));
			}
		}
		Ok(None)
	}

	/// Repairs or recomputes all stats counters for a given watch or path.
	///
	/// This implementation scans all nodes in the cache, recomputes per-watch, per-path, and per-type stats,
	/// and updates the stats tables in a single transaction. For large datasets, this may be slow and memory-intensive.
	/// If both watch_id and path are None, repairs the entire database. If either is Some, restricts to that scope.
	pub async fn repair_stats_counters(
		&mut self, watch_id: Option<&Uuid>, path: Option<&Path>,
	) -> DatabaseResult<usize> {
		use crate::database::storage::filesystem_cache::stats::{PathStats, WatchStats};
		use std::collections::HashMap;
		let write_txn = self.database.begin_write()?;
		let mut total = 0;
		{
			let fs_cache_table = write_txn.open_table(MULTI_WATCH_FS_CACHE)?;
			let mut watch_stats: HashMap<Uuid, WatchStats> = HashMap::new();
			let mut path_stats: HashMap<u64, PathStats> = HashMap::new();
			let mut per_type_counts: HashMap<String, u64> = HashMap::new();
			for entry in fs_cache_table.iter()? {
				let (key_bytes, value) = entry?;
				let node: FilesystemNode = utils::deserialize(value.value())?;
				let scoped_key: WatchScopedKey = utils::deserialize(key_bytes.value())?;
				// Filter by watch_id/path if requested
				if let Some(wid) = watch_id {
					if &scoped_key.watch_id != wid {
						continue;
					}
				}
				if let Some(filter_path) = path {
					if node.path != *filter_path {
						continue;
					}
				}
				// Use the event type stored on the node if available, else fallback to "create" for legacy nodes
				let event_type = node.last_event_type.as_deref().unwrap_or("create");
				// Update per-watch
				let ws = watch_stats.entry(scoped_key.watch_id).or_default();
				ws.event_count += 1;
				*ws.per_type_counts.entry(event_type.to_string()).or_insert(0) += 1;
				// Update per-path
				let ps = path_stats.entry(scoped_key.path_hash).or_default();
				ps.event_count += 1;
				*ps.per_type_counts.entry(event_type.to_string()).or_insert(0) += 1;
				// Update global per-type stats
				*per_type_counts.entry(event_type.to_string()).or_insert(0) += 1;
				total += 1;
			}
			// Write watch stats
			let mut watch_stats_table = write_txn.open_table(WATCH_STATS)?;
			for (wid, stats) in watch_stats {
				watch_stats_table
					.insert(wid.as_bytes().as_slice(), serialize(&stats)?.as_slice())?;
			}
			// Write path stats
			let mut path_stats_table = write_txn.open_table(PATH_STATS)?;
			for (phash, stats) in path_stats {
				path_stats_table.insert(
					(&phash.to_le_bytes()) as &[u8],
					serialize(&stats)?.as_slice(),
				)?;
			}
			// Write global per-type stats
			let mut stats_table = write_txn.open_table(STATS_TABLE)?;
			for (etype, count) in per_type_counts {
				let stat_key = crate::database::types::event_type_stat_key(&etype);
				stats_table.insert(stat_key.as_slice(), count.to_le_bytes().as_slice())?;
			}
			// All table borrows dropped here
		}
		write_txn.commit()?;
		Ok(total)
	}
} // End of impl RedbFilesystemCache
