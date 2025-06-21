//! Hierarchy/tree traversal logic for filesystem cache
//
// This module contains functions for ancestor/descendant traversal and unified directory listing.
//
// Limitations:
// - Ancestor traversal is O(log N) with hierarchy index, fallback to O(N) for legacy/corrupt data.
// - Descendant (subtree) traversal is O(M) using PATH_PREFIX_TABLE prefix scan, where M is the number of descendants.
// - Edge cases: path normalization, cross-platform semantics, and index corruption are not fully handled.
//
// TODO: Comprehensive health checks and index repair for production use.

use crate::database::error::DatabaseResult;
use crate::database::storage::filesystem_cache::trait_def::FilesystemCacheStorage;
use crate::database::types::FilesystemNode;
use redb::ReadableTable;
use std::path::Path;

impl crate::database::storage::filesystem_cache::RedbFilesystemCache {
	/// List all ancestor nodes for a given path (up to root).
	pub async fn list_ancestors_modular(
		&mut self, path: &Path,
	) -> DatabaseResult<Vec<FilesystemNode>> {
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
				// Try to find parent node using MULTI_WATCH_HIERARCHY index
				let parent_node = {
					let read_txn = self.database.begin_read()?;
					let hierarchy_table = read_txn.open_multimap_table(
						crate::database::storage::tables::MULTI_WATCH_HIERARCHY,
					)?;
					let mut found = None;
					let parent_key = hash.to_le_bytes();
					if let Ok(iter) = hierarchy_table.get(parent_key.as_slice()) {
						for entry in iter {
							let entry = entry?;
							let child_hash = u64::from_le_bytes(
								entry.value()[..8].try_into().unwrap_or_default(),
							);
							// Defensive: check for matching child
							if let Some(candidate) = self.get_node_by_hash(child_hash).await? {
								found = Some(candidate);
								break;
							}
						}
					}
					found
				};
				if let Some(parent) = parent_node {
					ancestors.push(parent.clone());
					current_path = parent.path.clone();
				} else {
					// Fallback: scan all nodes (legacy/corrupt index)
					let parent_node = self.find_node_by_hash_fallback(hash).await?;
					if let Some(parent) = parent_node {
						ancestors.push(parent.clone());
						current_path = parent.path.clone();
					} else {
						break;
					}
				}
			} else {
				break;
			}
		}
		Ok(ancestors)
	}

	/// List all descendant nodes for a given path (subtree query, efficient).
	pub async fn list_descendants_modular(
		&mut self, path: &Path,
	) -> DatabaseResult<Vec<FilesystemNode>> {
		let mut descendants = Vec::new();
		let prefix_str = path.to_string_lossy();
		let read_txn = self.database.begin_read()?;
		let prefix_table =
			read_txn.open_multimap_table(crate::database::storage::tables::PATH_PREFIX_TABLE)?;
		// Use prefix scan to find all descendant path hashes
		if let Ok(iter) = prefix_table.get(prefix_str.as_bytes()) {
			for entry in iter {
				let entry = entry?;
				// Deserialize WatchScopedKey from value
				let scoped_key: crate::database::types::WatchScopedKey =
					crate::database::storage::filesystem_cache::utils::deserialize(entry.value())?;
				let hash = scoped_key.path_hash;
				if let Some(node) = self.get_node_by_hash(hash).await? {
					// Exclude the root node itself from descendants
					if node.path != path {
						descendants.push(node);
					}
				}
			}
		}
		Ok(descendants)
	}

	/// Helper: get node by path hash (single watch or shared)
	async fn get_node_by_hash(&mut self, hash: u64) -> DatabaseResult<Option<FilesystemNode>> {
		// Try shared node first
		if let Some(shared) = self.get_shared_node(hash).await? {
			return Ok(Some(shared.node));
		}
		// Fallback: scan all watches (inefficient, but avoids missing data)
		self.find_node_by_hash_fallback(hash).await
	}

	/// Fallback: scan all nodes for a given hash (legacy/corrupt index)
	async fn find_node_by_hash_fallback(
		&mut self, hash: u64,
	) -> DatabaseResult<Option<FilesystemNode>> {
		let read_txn = self.database.begin_read()?;
		let fs_cache_table =
			read_txn.open_table(crate::database::storage::tables::MULTI_WATCH_FS_CACHE)?;
		for entry in fs_cache_table.iter()? {
			let (_key, value) = entry?;
			let candidate: FilesystemNode =
				crate::database::storage::filesystem_cache::utils::deserialize(value.value())?;
			if candidate.computed.path_hash == hash {
				return Ok(Some(candidate));
			}
		}
		Ok(None)
	}
}
