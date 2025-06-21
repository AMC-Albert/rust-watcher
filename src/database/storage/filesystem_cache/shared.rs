//! Shared node and unified node lookup logic for filesystem cache
//
// This module contains helpers for shared node handling and unified node lookup.
//
// Limitations:
// - O(N) scan for fallback; not suitable for very large datasets.
// - TODO: Replace with indexed or batched queries for production use.

use crate::database::error::DatabaseResult;
use crate::database::types::{FilesystemNode, SharedNodeInfo};
use std::path::Path;

pub struct SharedNodeHelpers;

impl SharedNodeHelpers {
	/// Prefer shared node if present, else fallback to any watch-specific node (naive scan).
	pub async fn get_unified_node<F, G>(
		get_shared_node: F, scan_all_nodes: G, path: &Path,
	) -> DatabaseResult<Option<FilesystemNode>>
	where
		F: Fn(
			u64,
		) -> std::pin::Pin<
			Box<dyn std::future::Future<Output = DatabaseResult<Option<SharedNodeInfo>>> + Send>,
		>,
		G: Fn(
			&Path,
		) -> std::pin::Pin<
			Box<dyn std::future::Future<Output = DatabaseResult<Option<FilesystemNode>>> + Send>,
		>,
	{
		let path_hash = crate::database::types::calculate_path_hash(path);
		if let Some(shared) = get_shared_node(path_hash).await? {
			return Ok(Some(shared.node));
		}
		// Fallback: scan all watches for a node
		scan_all_nodes(path).await
	}
	// TODO: Add more helpers as needed (shared node insert, etc.)
}
