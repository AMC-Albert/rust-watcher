//! Hierarchy/tree traversal logic for filesystem cache
//
// This module contains functions for ancestor/descendant traversal and unified directory listing.
//
// Limitations:
// - All traversal is O(N) and not suitable for very large datasets.
// - TODO: Replace with indexed or batched queries for production use.

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
				// Find parent node by hash (scan all nodes, not efficient, but safe for now)
				let parent_node = {
					let read_txn = self.database.begin_read()?;
					let fs_cache_table = read_txn
						.open_table(crate::database::storage::tables::MULTI_WATCH_FS_CACHE)?;
					let mut found = None;
					for entry in fs_cache_table.iter()? {
						let (_key, value) = entry?;
						let candidate: FilesystemNode =
							crate::database::storage::filesystem_cache::utils::deserialize(
								value.value(),
							)?;
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

	/// List all descendant nodes for a given path (subtree query).
	pub async fn list_descendants_modular(
		&mut self, path: &Path,
	) -> DatabaseResult<Vec<FilesystemNode>> {
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
}
