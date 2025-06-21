//! Overlap detection and shared cache optimization routines
//!
//! Moved from multi_watch.rs. This module handles overlap detection, statistics, and shared cache optimization.
use crate::database::error::DatabaseResult;
use crate::database::storage::multi_watch::implementation::MultiWatchDatabase;
use crate::database::storage::multi_watch::types::{WatchMetadata, WatchOverlap};
use redb::ReadableTable;
use std::path::PathBuf;

/// Detect overlap between two watches by root path
pub fn detect_overlap(watch_a: &WatchMetadata, watch_b: &WatchMetadata) -> WatchOverlap {
	let a = watch_a.root_path.components().collect::<Vec<_>>();
	let b = watch_b.root_path.components().collect::<Vec<_>>();
	if a == b {
		return WatchOverlap::Identical(watch_a.watch_id);
	}
	if a.starts_with(&b) {
		return WatchOverlap::Ancestor {
			ancestor: watch_b.watch_id,
			descendant: watch_a.watch_id,
		};
	}
	if b.starts_with(&a) {
		return WatchOverlap::Ancestor {
			ancestor: watch_a.watch_id,
			descendant: watch_b.watch_id,
		};
	}
	let min_len = a.len().min(b.len());
	let mut common = Vec::new();
	for i in 0..min_len {
		if a[i] == b[i] {
			common.push(a[i].as_os_str());
		} else {
			break;
		}
	}
	if !common.is_empty() {
		let prefix = common.iter().collect::<PathBuf>();
		let mut components = prefix.components();
		let is_only_root = match components.next() {
			Some(std::path::Component::RootDir) => components.next().is_none(),
			_ => false,
		};
		if !is_only_root {
			return WatchOverlap::Partial {
				watch_a: watch_a.watch_id,
				watch_b: watch_b.watch_id,
				common_prefix: prefix,
			};
		}
	}
	WatchOverlap::None
}

impl MultiWatchDatabase {
	/// Compute overlap statistics for all registered watches
	pub async fn compute_overlap_statistics(&self) -> DatabaseResult<Vec<WatchOverlap>> {
		let watches = self.list_watches().await?;
		let mut overlaps = Vec::new();
		for i in 0..watches.len() {
			for j in (i + 1)..watches.len() {
				let overlap = detect_overlap(&watches[i], &watches[j]);
				if overlap != WatchOverlap::None {
					overlaps.push(overlap);
				}
			}
		}
		Ok(overlaps)
	}

	/// Optimize the shared cache by analyzing overlap statistics and node reference counts.
	///
	/// This implementation is minimal and only logs detected overlaps. In production, this should
	/// trigger cache merges/splits and update shared node state as needed.
	pub async fn optimize_shared_cache(&self) {
		let overlaps = match self.compute_overlap_statistics().await {
			Ok(o) => o,
			Err(e) => {
				eprintln!("[MultiWatchDatabase] Failed to compute overlap statistics: {e}");
				return;
			}
		};
		for overlap in overlaps {
			match overlap {
				WatchOverlap::Partial {
					watch_a,
					watch_b,
					ref common_prefix,
				} => {
					if let Err(e) = self
						.merge_nodes_to_shared(common_prefix, &[watch_a, watch_b])
						.await
					{
						eprintln!(
							"[MultiWatchDatabase] Failed to merge nodes at {:?}: {e}",
							common_prefix
						);
					} else {
						println!("[MultiWatchDatabase] Merged nodes at {:?} into shared node for watches {:?}", common_prefix, [watch_a, watch_b]);
					}
				}
				WatchOverlap::Ancestor {
					ancestor: watch_a,
					descendant: watch_b,
				} => {
					if let Ok(Some(watch)) = self.get_watch_metadata(&watch_b).await {
						let path = &watch.root_path;
						if let Err(e) = self.merge_nodes_to_shared(path, &[watch_a, watch_b]).await
						{
							eprintln!(
								"[MultiWatchDatabase] Failed to merge nodes at {:?}: {e}",
								path
							);
						} else {
							println!("[MultiWatchDatabase] Merged nodes at {:?} into shared node for watches {:?}", path, [watch_a, watch_b]);
						}
					}
				}
				_ => {}
			}
		}
		if let Err(e) = self.cleanup_redundant_and_orphaned_nodes().await {
			eprintln!("[MultiWatchDatabase] Cleanup after optimization failed: {e}");
		}
	}

	/// Remove redundant watch-specific nodes and orphaned shared nodes after optimization.
	///
	/// This version attempts to be atomic by batching all removals in a single transaction.
	///
	/// Limitations:
	/// - If a crash occurs after commit but before all in-memory state is updated, some cleanup may be lost.
	/// - No distributed locking; races are possible if multiple optimizations run concurrently.
	/// - TODO: Add journaling or two-phase commit for true atomicity if needed.
	pub async fn cleanup_redundant_and_orphaned_nodes(&self) -> Result<(), String> {
		use crate::database::types::{UnifiedNode, WatchScopedKey};
		let db = self.database.begin_write().map_err(|e| e.to_string())?;
		{
			// Remove redundant watch-specific nodes and raw path_hash nodes FIRST
			{
				let fs_cache = db
					.open_table(crate::database::storage::tables::MULTI_WATCH_FS_CACHE)
					.map_err(|e| e.to_string())?;
				let shared_nodes = db
					.open_table(crate::database::storage::tables::SHARED_NODES)
					.map_err(|e| e.to_string())?;
				let mut to_remove = Vec::new();
				for entry in fs_cache.iter().map_err(|e| e.to_string())? {
					let (key, value) = entry.map_err(|e| e.to_string())?;
					if let Ok(node) = bincode::deserialize::<crate::database::types::FilesystemNode>(
						value.value(),
					) {
						let path_hash = node.computed.path_hash;
						if key.value().len() == 8 {
							let mut arr = [0u8; 8];
							arr.copy_from_slice(&key.value()[..8]);
							let raw_path_hash = u64::from_le_bytes(arr);
							let shared_lookup =
								shared_nodes.get(raw_path_hash.to_le_bytes().as_slice());
							if let Ok(Some(_)) = shared_lookup {
								to_remove.push(key.value().to_vec());
							}
						} else {
							// Try to parse the key as WatchScopedKey
							if let Ok(watch_scoped_key) =
								bincode::deserialize::<WatchScopedKey>(key.value())
							{
								if let Some(shared_value) = shared_nodes
									.get(path_hash.to_le_bytes().as_slice())
									.map_err(|e| e.to_string())?
								{
									if let Ok(UnifiedNode::Shared { shared_info }) =
										bincode::deserialize::<UnifiedNode>(shared_value.value())
									{
										if shared_info
											.watching_scopes
											.contains(&watch_scoped_key.watch_id)
										{
											to_remove.push(key.value().to_vec());
										}
									}
								}
							} else if key.value().len() == 8 {
								// Handle legacy/test keys: direct path_hash (u64 LE)
								let mut arr = [0u8; 8];
								arr.copy_from_slice(&key.value()[..8]);
								let raw_path_hash = u64::from_le_bytes(arr);
								let shared_lookup =
									shared_nodes.get(raw_path_hash.to_le_bytes().as_slice());
								if let Ok(Some(_)) = shared_lookup {
									to_remove.push(key.value().to_vec());
								}
							}
						}
					}
				}
				// Now remove orphaned shared nodes (reference_count == 0 or watching_scopes is empty)
				{
					let shared_nodes = db
						.open_table(crate::database::storage::tables::SHARED_NODES)
						.map_err(|e| e.to_string())?;
					let mut to_remove = Vec::new();
					// Collect debug info and keys to remove in a single pass
					for entry in shared_nodes.iter().map_err(|e| e.to_string())? {
						let (key, value) = entry.map_err(|e| e.to_string())?;
						// Try UnifiedNode first
						if let Ok(unode) = bincode::deserialize::<UnifiedNode>(value.value()) {
							if let UnifiedNode::Shared { shared_info: info } = unode {
								if info.reference_count == 0 || info.watching_scopes.is_empty() {
									to_remove.push(key.value().to_vec());
								}
							}
						} else if let Ok(info) = bincode::deserialize::<
							crate::database::types::SharedNodeInfo,
						>(value.value())
						{
							// Legacy/test: direct SharedNodeInfo
							if info.reference_count == 0 || info.watching_scopes.is_empty() {
								to_remove.push(key.value().to_vec());
							}
						}
					}
					// Drop the iterator before mutating the table
					drop(shared_nodes);
					let mut shared_nodes = db
						.open_table(crate::database::storage::tables::SHARED_NODES)
						.map_err(|e| e.to_string())?;
					for key in &to_remove {
						shared_nodes
							.remove(key.as_slice())
							.map_err(|e| e.to_string())?;
					}
				}
			}
		}
		db.commit().map_err(|e| e.to_string())?;
		Ok(())
	}
}

// The following functions require DB access and should be called from the main implementation module.
// Placeholders for now; move logic here as needed and pass DB handles as arguments.
//
// pub async fn compute_overlap_statistics(...) -> ...
// pub async fn optimize_shared_cache(...) -> ...
// pub async fn cleanup_redundant_and_orphaned_nodes(...) -> ...
