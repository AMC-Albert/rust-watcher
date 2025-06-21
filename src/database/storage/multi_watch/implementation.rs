//! Implementation of MultiWatchDatabase and related traits
//
// Contains the main struct, trait, and core DB logic for multi-watch management.

use crate::database::error::DatabaseResult;
use crate::database::types::{SharedNodeInfo, WatchMetadata};
use async_trait::async_trait;
use redb::Database;
use redb::ReadableTable;
use std::sync::Arc;
use uuid::Uuid;

/// Trait for multi-watch storage operations
#[async_trait]
pub trait MultiWatchStorage: Send + Sync {
	async fn store_watch_metadata(&mut self, metadata: &WatchMetadata) -> DatabaseResult<()>;
	async fn get_watch_metadata(
		&mut self,
		watch_id: &Uuid,
	) -> DatabaseResult<Option<WatchMetadata>>;
	async fn remove_watch(&mut self, watch_id: &Uuid) -> DatabaseResult<()>;
	async fn store_shared_node(&mut self, shared_info: &SharedNodeInfo) -> DatabaseResult<()>;
	async fn get_shared_node(&mut self, path_hash: u64) -> DatabaseResult<Option<SharedNodeInfo>>;
	async fn list_watches(&mut self) -> DatabaseResult<Vec<WatchMetadata>>;
}

/// Main implementation struct for multi-watch database management
pub struct MultiWatchDatabase {
	pub(crate) database: Arc<Database>,
}

impl MultiWatchDatabase {
	pub fn new(database: Arc<Database>) -> Self {
		Self { database }
	}

	// Core DB logic for watch registration, listing, metadata, shared node storage, etc.
	// (See old multi_watch.rs for full details; only core methods are included here.)

	pub async fn register_watch(&self, metadata: &WatchMetadata) -> DatabaseResult<()> {
		let write_txn = self.database.begin_write()?;
		{
			let mut table =
				write_txn.open_table(crate::database::storage::tables::WATCH_REGISTRY)?;
			table.insert(
				&metadata.watch_id.as_bytes()[..],
				bincode::serialize(metadata).unwrap().as_slice(),
			)?;
		}
		write_txn.commit()?;
		Ok(())
	}

	pub async fn list_watches(&self) -> DatabaseResult<Vec<WatchMetadata>> {
		let read_txn = self.database.begin_read()?;
		let table = read_txn.open_table(crate::database::storage::tables::WATCH_REGISTRY)?;
		let mut result = Vec::new();
		for entry in table.range::<&[u8]>(..)? {
			let (_key, value) = entry?;
			if let Ok(meta) = bincode::deserialize::<WatchMetadata>(value.value()) {
				result.push(meta);
			}
		}
		Ok(result)
	}

	pub async fn get_watch_metadata(
		&self,
		watch_id: &Uuid,
	) -> DatabaseResult<Option<WatchMetadata>> {
		let read_txn = self.database.begin_read()?;
		let table = read_txn.open_table(crate::database::storage::tables::WATCH_REGISTRY)?;
		if let Some(value) = table.get(&watch_id.as_bytes()[..])? {
			let meta = bincode::deserialize::<WatchMetadata>(value.value()).ok();
			Ok(meta)
		} else {
			Ok(None)
		}
	}

	pub async fn store_shared_node(&self, shared_info: &SharedNodeInfo) -> DatabaseResult<()> {
		let write_txn = self.database.begin_write()?;
		{
			let mut table = write_txn.open_table(crate::database::storage::tables::SHARED_NODES)?;
			let key = shared_info.node.computed.path_hash.to_le_bytes();
			let value = bincode::serialize(shared_info)
				.map_err(|e| crate::database::error::DatabaseError::Serialization(e.to_string()))?;
			table.insert(key.as_slice(), value.as_slice())?;
		}
		write_txn.commit()?;
		Ok(())
	}

	pub async fn get_shared_node(&self, path_hash: u64) -> DatabaseResult<Option<SharedNodeInfo>> {
		let read_txn = self.database.begin_read()?;
		let table = read_txn.open_table(crate::database::storage::tables::SHARED_NODES)?;
		let key = path_hash.to_le_bytes();
		if let Some(value) = table.get(key.as_slice())? {
			let node = bincode::deserialize::<SharedNodeInfo>(value.value()).ok();
			Ok(node)
		} else {
			Ok(None)
		}
	}

	pub async fn remove_watch(&self, watch_id: &Uuid) -> DatabaseResult<()> {
		let write_txn = self.database.begin_write()?;
		{
			// Remove from registry
			let mut registry =
				write_txn.open_table(crate::database::storage::tables::WATCH_REGISTRY)?;
			registry.remove(&watch_id.as_bytes()[..])?;
			// Clean up shared nodes (see old logic for details)
			let mut shared_nodes =
				write_txn.open_table(crate::database::storage::tables::SHARED_NODES)?;
			let mut to_remove = Vec::new();
			let mut to_update = Vec::new();
			for entry in shared_nodes.iter()? {
				let (key, value) = entry?;
				if let Ok(mut info) = bincode::deserialize::<SharedNodeInfo>(value.value()) {
					if let Some(pos) = info.watching_scopes.iter().position(|id| id == watch_id) {
						info.watching_scopes.remove(pos);
						if info.reference_count > 0 {
							info.reference_count -= 1;
						}
						if info.reference_count == 0 {
							to_remove.push(key.value().to_vec());
						} else {
							let updated = bincode::serialize(&info).map_err(|e| {
								crate::database::error::DatabaseError::Serialization(e.to_string())
							})?;
							to_update.push((key.value().to_vec(), updated));
						}
					}
				}
			}
			for (key, updated) in to_update {
				shared_nodes.insert(key.as_slice(), updated.as_slice())?;
			}
			for key in to_remove {
				shared_nodes.remove(key.as_slice())?;
			}
		}
		write_txn.commit()?;
		Ok(())
	}

	/// Merge nodes at the given path into a shared node for the specified watches.
	/// This version attempts to be atomic by performing all DB mutations in a single transaction.
	///
	/// Limitations:
	/// - No rollback/undo if crash occurs after partial commit (ReDB limitation).
	/// - No distributed locking; concurrent optimizations may race.
	/// - TODO: Add journaling or two-phase commit for true atomicity if needed.
	/// - TODO: Validate all input nodes before merge to avoid data loss.
	pub async fn merge_nodes_to_shared(
		&self,
		path: &std::path::Path,
		watch_ids: &[uuid::Uuid],
	) -> Result<(), String> {
		use crate::database::types::{FilesystemNode, SharedNodeInfo, UnifiedNode};
		use chrono::Utc;
		let path_hash = crate::database::types::calculate_path_hash(path);
		let node = FilesystemNode {
			path: path.to_path_buf(),
			node_type: crate::database::types::NodeType::Directory {
				child_count: 0,
				total_size: 0,
				max_depth: 0,
			},
			metadata: crate::database::types::NodeMetadata {
				modified_time: std::time::SystemTime::now(),
				created_time: None,
				accessed_time: None,
				permissions: 0,
				inode: None,
				windows_id: None,
			},
			cache_info: crate::database::types::CacheInfo {
				cached_at: Utc::now(),
				last_verified: Utc::now(),
				cache_version: 1,
				needs_refresh: false,
			},
			computed: crate::database::types::ComputedProperties {
				depth_from_root: 0,
				path_hash,
				parent_hash: None,
				canonical_name: path.to_string_lossy().to_string(),
			},
		};
		let shared_info = SharedNodeInfo {
			node,
			watching_scopes: watch_ids.to_vec(),
			reference_count: watch_ids.len() as u32,
			last_shared_update: Utc::now(),
		};
		let key = path_hash.to_le_bytes();
		let value =
			bincode::serialize(&UnifiedNode::Shared { shared_info }).map_err(|e| e.to_string())?;
		let db = self.database.begin_write().map_err(|e| e.to_string())?;
		{
			let mut table = db
				.open_table(crate::database::storage::tables::SHARED_NODES)
				.map_err(|e| e.to_string())?;
			// TODO: Validate no conflicting shared node exists before insert
			table
				.insert(&key[..], value.as_slice())
				.map_err(|e| e.to_string())?;
		}
		// TODO: Remove all watch-specific nodes for this path in the same transaction
		// TODO: Update reference counts for all affected nodes atomically
		db.commit().map_err(|e| e.to_string())?;
		Ok(())
	}

	/// Detect overlap between two watches by root path (compatibility shim for legacy API)
	pub async fn detect_overlap(
		&self,
		watch_a: &crate::database::storage::multi_watch::types::WatchMetadata,
		watch_b: &crate::database::storage::multi_watch::types::WatchMetadata,
	) -> crate::database::storage::multi_watch::types::WatchOverlap {
		crate::database::storage::multi_watch::optimization::detect_overlap(watch_a, watch_b)
	}

	// Additional methods (transaction coordination, etc.) can be added here as needed.
}
