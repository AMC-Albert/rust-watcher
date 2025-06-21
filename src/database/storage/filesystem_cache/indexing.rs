//! Path prefix and indexing logic for filesystem cache
//
// This module contains helpers for path prefix indexing and prefix-based node search.
//
// Limitations:
// - All prefix search is O(N) and not suitable for very large datasets.
// - TODO: Replace with indexed or batched queries for production use.

use crate::database::error::DatabaseResult;
use crate::database::storage::filesystem_cache::trait_def::FilesystemCacheStorage;
use crate::database::storage::filesystem_cache::utils;
use crate::database::storage::filesystem_cache::RedbFilesystemCache;
use crate::database::types::FilesystemNode;
use redb::WriteTransaction;
use std::path::Path;
use uuid::Uuid;

pub struct IndexingHelpers;

impl IndexingHelpers {
	/// Insert all path prefixes for a node into the prefix table.
	pub fn index_path_prefixes<F>(
		node: &FilesystemNode, _watch_id: &Uuid, mut insert_fn: F,
	) -> DatabaseResult<()>
	where
		F: FnMut(&str, &[u8]) -> DatabaseResult<()>,
	{
		let path = &node.path;
		let mut prefix = Path::new("").to_path_buf();
		for component in path.components() {
			prefix.push(component);
			let prefix_str = prefix.to_string_lossy();
			// The caller provides the key bytes (e.g., scoped key)
			insert_fn(&prefix_str, &[])?; // TODO: Pass correct key bytes
		}
		Ok(())
	}

	/// Find nodes by prefix (naive scan).
	pub async fn find_nodes_by_prefix<F>(
		get_nodes_by_prefix: F, prefix: &Path,
	) -> DatabaseResult<Vec<FilesystemNode>>
	where
		F: Fn(
			&Path,
		) -> std::pin::Pin<
			Box<dyn std::future::Future<Output = DatabaseResult<Vec<FilesystemNode>>> + Send>,
		>,
	{
		get_nodes_by_prefix(prefix).await
	}
}

impl RedbFilesystemCache {
	/// Insert all path prefixes for a node into the prefix table (delegated to IndexingHelpers).
	pub fn index_path_prefixes_modular(
		write_txn: &WriteTransaction, node: &FilesystemNode, watch_id: &Uuid,
	) -> DatabaseResult<()> {
		IndexingHelpers::index_path_prefixes(node, watch_id, |prefix_str, _| {
			let mut prefix_table = write_txn
				.open_multimap_table(crate::database::storage::tables::PATH_PREFIX_TABLE)?;
			let path_hash = crate::database::types::calculate_path_hash(&node.path);
			let scoped_key = RedbFilesystemCache::create_scoped_key(watch_id, path_hash);
			let key_bytes = utils::key_to_bytes(&scoped_key);
			prefix_table.insert(prefix_str.as_bytes(), key_bytes.as_slice())?;
			Ok(())
		})
	}

	/// Find nodes by prefix using the trait method directly (no closure indirection).
	pub async fn find_nodes_by_prefix_modular(
		&mut self, watch_id: &Uuid, prefix: &Path,
	) -> DatabaseResult<Vec<FilesystemNode>> {
		self.find_nodes_by_prefix(watch_id, prefix).await
	}
}
