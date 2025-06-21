//! Multi-watch database management
//!
//! This module handles coordination between multiple filesystem watches,
//! shared resource management, and cross-watch operations.

use crate::database::types::WatchPermissions;
use crate::database::{
	error::DatabaseResult,
	types::{SharedNodeInfo, WatchMetadata},
};
use chrono::{DateTime, Utc};
use redb::{Database, ReadableTable};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;
use walkdir::WalkDir;

/// Transaction status for coordination
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionStatus {
	InProgress,
	Committed,
	Aborted,
}

/// Metadata for a watch-scoped transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchTransaction {
	pub transaction_id: Uuid,
	pub watch_id: Uuid,
	pub started_at: DateTime<Utc>,
	pub status: TransactionStatus,
}

/// Trait for multi-watch storage operations
#[async_trait::async_trait]
pub trait MultiWatchStorage: Send + Sync {
	/// Register a new watch with metadata
	async fn store_watch_metadata(&mut self, metadata: &WatchMetadata) -> DatabaseResult<()>;

	/// Retrieve watch metadata
	async fn get_watch_metadata(
		&mut self,
		watch_id: &Uuid,
	) -> DatabaseResult<Option<WatchMetadata>>;

	/// Remove a watch and clean up its resources
	async fn remove_watch(&mut self, watch_id: &Uuid) -> DatabaseResult<()>;

	/// Store shared node information
	async fn store_shared_node(&mut self, shared_info: &SharedNodeInfo) -> DatabaseResult<()>;

	/// Retrieve shared node information
	async fn get_shared_node(&mut self, path_hash: u64) -> DatabaseResult<Option<SharedNodeInfo>>;

	/// List all active watches
	async fn list_watches(&mut self) -> DatabaseResult<Vec<WatchMetadata>>;
}

/// Implementation of multi-watch storage using ReDB
pub struct MultiWatchImpl {
	database: Arc<Database>,
}

impl MultiWatchImpl {
	pub fn new(database: Arc<Database>) -> Self {
		Self { database }
	}

	/// Initialize multi-watch tables
	pub async fn initialize(&mut self, _database: &Arc<Database>) -> DatabaseResult<()> {
		let write_txn = self.database.begin_write()?;
		{
			// Create multi-watch tables if they don't exist
			let _multi_fs_cache = write_txn.open_table(super::tables::MULTI_WATCH_FS_CACHE)?;
			let _multi_hierarchy =
				write_txn.open_multimap_table(super::tables::MULTI_WATCH_HIERARCHY)?;
			let _shared_nodes = write_txn.open_table(super::tables::SHARED_NODES)?;
			let _watch_registry = write_txn.open_table(super::tables::WATCH_REGISTRY)?;
			let _path_to_watches = write_txn.open_multimap_table(super::tables::PATH_TO_WATCHES)?;
		}
		write_txn.commit()?;
		Ok(())
	}
}

#[async_trait::async_trait]
impl MultiWatchStorage for MultiWatchImpl {
	async fn store_watch_metadata(&mut self, _metadata: &WatchMetadata) -> DatabaseResult<()> {
		// TODO: Implement watch metadata storage
		// This is a placeholder for Phase 2 implementation
		Ok(())
	}

	async fn get_watch_metadata(
		&mut self,
		_watch_id: &Uuid,
	) -> DatabaseResult<Option<WatchMetadata>> {
		// TODO: Implement watch metadata retrieval
		// This is a placeholder for Phase 2 implementation
		Ok(None)
	}

	async fn remove_watch(&mut self, _watch_id: &Uuid) -> DatabaseResult<()> {
		// TODO: Implement watch removal and cleanup
		// This is a placeholder for Phase 2 implementation
		Ok(())
	}

	async fn store_shared_node(&mut self, _shared_info: &SharedNodeInfo) -> DatabaseResult<()> {
		// TODO: Implement shared node storage
		// This is a placeholder for Phase 2 implementation
		Ok(())
	}

	async fn get_shared_node(&mut self, _path_hash: u64) -> DatabaseResult<Option<SharedNodeInfo>> {
		// TODO: Implement shared node retrieval
		// This is a placeholder for Phase 2 implementation
		Ok(None)
	}

	async fn list_watches(&mut self) -> DatabaseResult<Vec<WatchMetadata>> {
		// TODO: Implement watch listing
		// This is a placeholder for Phase 2 implementation
		Ok(Vec::new())
	}
}

// --- MultiWatchDatabase: initial design and partial implementation ---
// This struct will coordinate multiple watches, their metadata, and shared node management.
// Only watch registration, listing, and metadata retrieval are implemented for now.

pub struct MultiWatchDatabase {
	database: Arc<Database>,
}

impl MultiWatchDatabase {
	pub fn new(database: Arc<Database>) -> Self {
		Self { database }
	}

	/// Register a new watch with metadata
	pub async fn register_watch(&self, metadata: &WatchMetadata) -> DatabaseResult<()> {
		let write_txn = self.database.begin_write()?;
		{
			let mut table = write_txn.open_table(super::tables::WATCH_REGISTRY)?;
			table.insert(
				&metadata.watch_id.as_bytes()[..],
				bincode::serialize(metadata).unwrap().as_slice(),
			)?;
		}
		write_txn.commit()?;
		Ok(())
	}

	/// List all registered watches
	pub async fn list_watches(&self) -> DatabaseResult<Vec<WatchMetadata>> {
		let read_txn = self.database.begin_read()?;
		let table = read_txn.open_table(super::tables::WATCH_REGISTRY)?;
		let mut result = Vec::new();
		for entry in table.range::<&[u8]>(..)? {
			let (_key, value) = entry?;
			if let Ok(meta) = bincode::deserialize::<WatchMetadata>(value.value()) {
				result.push(meta);
			}
		}
		Ok(result)
	}

	/// Get metadata for a specific watch
	pub async fn get_watch_metadata(
		&self,
		watch_id: &Uuid,
	) -> DatabaseResult<Option<WatchMetadata>> {
		let read_txn = self.database.begin_read()?;
		let table = read_txn.open_table(super::tables::WATCH_REGISTRY)?;
		if let Some(value) = table.get(&watch_id.as_bytes()[..])? {
			let meta = bincode::deserialize::<WatchMetadata>(value.value()).ok();
			Ok(meta)
		} else {
			Ok(None)
		}
	}

	/// Store or update a shared node in the SHARED_NODES table.
	///
	/// This will overwrite any existing entry for the same path_hash.
	/// If reference_count is zero, the node should be removed by the caller.
	///
	/// Failure modes: serialization errors, DB write errors, schema drift.
	pub async fn store_shared_node(&self, shared_info: &SharedNodeInfo) -> DatabaseResult<()> {
		let write_txn = self.database.begin_write()?;
		{
			let mut table = write_txn.open_table(super::tables::SHARED_NODES)?;
			let key = shared_info.node.computed.path_hash.to_le_bytes();
			let value = bincode::serialize(shared_info)
				.map_err(|e| crate::database::error::DatabaseError::Serialization(e.to_string()))?;
			table.insert(key.as_slice(), value.as_slice())?;
		}
		write_txn.commit()?;
		Ok(())
	}

	/// Retrieve a shared node by path_hash from the SHARED_NODES table.
	///
	/// Returns None if not found or deserialization fails.
	pub async fn get_shared_node(&self, path_hash: u64) -> DatabaseResult<Option<SharedNodeInfo>> {
		let read_txn = self.database.begin_read()?;
		let table = read_txn.open_table(super::tables::SHARED_NODES)?;
		let key = path_hash.to_le_bytes();
		if let Some(value) = table.get(key.as_slice())? {
			let node = bincode::deserialize::<SharedNodeInfo>(value.value()).ok();
			Ok(node)
		} else {
			Ok(None)
		}
	}

	/// Remove a watch and clean up all references in the registry and shared nodes.
	///
	/// This will:
	/// - Remove the watch from WATCH_REGISTRY.
	/// - Scan SHARED_NODES and decrement reference_count for any node referencing this watch.
	/// - Remove any shared node whose reference_count drops to zero.
	///
	/// Failure modes: DB errors, partial cleanup if interrupted, schema drift.
	pub async fn remove_watch(&self, watch_id: &Uuid) -> DatabaseResult<()> {
		let write_txn = self.database.begin_write()?;
		{
			// Remove from registry
			let mut registry = write_txn.open_table(super::tables::WATCH_REGISTRY)?;
			registry.remove(&watch_id.as_bytes()[..])?;

			// Clean up shared nodes
			let mut shared_nodes = write_txn.open_table(super::tables::SHARED_NODES)?;
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
							// Defer update until after iteration
							let updated = bincode::serialize(&info).map_err(|e| {
								crate::database::error::DatabaseError::Serialization(e.to_string())
							})?;
							to_update.push((key.value().to_vec(), updated));
						}
					}
				}
			}
			// Perform updates
			for (key, updated) in to_update {
				shared_nodes.insert(key.as_slice(), updated.as_slice())?;
			}
			// Remove orphaned shared nodes
			for key in to_remove {
				shared_nodes.remove(key.as_slice())?;
			}
		}
		write_txn.commit()?;
		Ok(())
	}

	/// Begin a new transaction for a watch and persist it
	pub async fn begin_transaction(&self, watch_id: &Uuid) -> DatabaseResult<WatchTransaction> {
		let txn = WatchTransaction {
			transaction_id: Uuid::new_v4(),
			watch_id: *watch_id,
			started_at: Utc::now(),
			status: TransactionStatus::InProgress,
		};
		self.persist_transaction(&txn).await?;
		Ok(txn)
	}

	/// Commit a transaction and update its status
	pub async fn commit_transaction(&self, txn: &WatchTransaction) -> DatabaseResult<()> {
		self.update_transaction_status(&txn.transaction_id, TransactionStatus::Committed)
			.await
	}

	/// Abort a transaction and update its status
	pub async fn abort_transaction(&self, txn: &WatchTransaction) -> DatabaseResult<()> {
		self.update_transaction_status(&txn.transaction_id, TransactionStatus::Aborted)
			.await
	}

	/// Persist a new transaction for a watch
	pub async fn persist_transaction(&self, txn: &WatchTransaction) -> DatabaseResult<()> {
		let write_txn = self.database.begin_write()?;
		{
			let mut table = write_txn.open_table(super::tables::WATCH_TRANSACTIONS)?;
			let key = txn.transaction_id.as_bytes();
			let value = bincode::serialize(txn)
				.map_err(|e| crate::database::error::DatabaseError::Serialization(e.to_string()))?;
			table.insert(key.as_slice(), value.as_slice())?;
		}
		write_txn.commit()?;
		Ok(())
	}

	pub async fn get_transaction(
		&self,
		transaction_id: &Uuid,
	) -> DatabaseResult<Option<WatchTransaction>> {
		let read_txn = self.database.begin_read()?;
		let table = read_txn.open_table(super::tables::WATCH_TRANSACTIONS)?;
		if let Some(value) = table.get(transaction_id.as_bytes().as_slice())? {
			let txn = bincode::deserialize::<WatchTransaction>(value.value()).ok();
			Ok(txn)
		} else {
			Ok(None)
		}
	}

	pub async fn update_transaction_status(
		&self,
		transaction_id: &Uuid,
		status: TransactionStatus,
	) -> DatabaseResult<()> {
		let write_txn = self.database.begin_write()?;
		// First, read and update the transaction in a separate scope
		let updated = {
			let table = write_txn.open_table(super::tables::WATCH_TRANSACTIONS)?;
			let value_bytes = table
				.get(transaction_id.as_bytes().as_slice())?
				.map(|v| v.value().to_vec()); // Copy bytes to break borrow
			if let Some(value_bytes) = value_bytes {
				if let Ok(mut txn) = bincode::deserialize::<WatchTransaction>(&value_bytes) {
					txn.status = status;
					Some(bincode::serialize(&txn).map_err(|e| {
						crate::database::error::DatabaseError::Serialization(e.to_string())
					})?)
				} else {
					None
				}
			} else {
				None
			}
		};
		// Now, perform the write if needed
		if let Some(updated) = updated {
			let mut table = write_txn.open_table(super::tables::WATCH_TRANSACTIONS)?;
			table.insert(transaction_id.as_bytes().as_slice(), updated.as_slice())?;
		}
		write_txn.commit()?;
		Ok(())
	}

	/// Create a new watch, scan the filesystem tree, and initialize metadata and permissions.
	/// Returns the created WatchMetadata.
	pub async fn create_watch_with_scan(
		&self,
		root_path: PathBuf,
		permissions: Option<WatchPermissions>,
	) -> DatabaseResult<WatchMetadata> {
		let watch_id = Uuid::new_v4();
		let created_at = Utc::now();
		let mut node_count = 0u64;
		// Scan the filesystem tree and count nodes (could be extended to cache nodes)
		for entry in WalkDir::new(&root_path).into_iter().filter_map(Result::ok) {
			let path = entry.path();
			if let Ok(metadata) = fs::metadata(path) {
				let _node =
					crate::database::types::FilesystemNode::new(path.to_path_buf(), &metadata);
				node_count += 1;
				// TODO: Optionally insert node into cache here
			}
		}
		let meta = WatchMetadata {
			watch_id,
			root_path: root_path.clone(),
			created_at,
			last_scan: Some(created_at),
			node_count,
			is_active: true,
			config_hash: 0, // TODO: Compute from config if needed
			permissions,
		};
		self.register_watch(&meta).await?;
		Ok(meta)
	}
}

/// Represents the overlap relationship between two watches
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchOverlap {
	/// No overlap between the two watches
	None,
	/// One watch is a strict ancestor of the other
	Ancestor { ancestor: Uuid, descendant: Uuid },
	/// The two watches have a common subtree (partial overlap)
	Partial {
		watch_a: Uuid,
		watch_b: Uuid,
		common_prefix: std::path::PathBuf,
	},
	/// The two watches are identical (same root)
	Identical(Uuid),
}

impl MultiWatchDatabase {
	/// Detect overlap between two watches by root path
	pub async fn detect_overlap(
		&self,
		watch_a: &WatchMetadata,
		watch_b: &WatchMetadata,
	) -> WatchOverlap {
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
		// Find common prefix
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
			let prefix = common.iter().collect::<std::path::PathBuf>();
			// On Windows and Unix, a common prefix of only the root (e.g., "\\" or "/") is not a meaningful overlap.
			// Only treat as partial overlap if the common prefix is longer than just the root.
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

	/// Compute overlap statistics for all registered watches
	pub async fn compute_overlap_statistics(&self) -> DatabaseResult<Vec<WatchOverlap>> {
		let watches = self.list_watches().await?;
		let mut overlaps = Vec::new();
		for i in 0..watches.len() {
			for j in (i + 1)..watches.len() {
				let overlap = self.detect_overlap(&watches[i], &watches[j]).await;
				if overlap != WatchOverlap::None {
					overlaps.push(overlap);
				}
			}
		}
		Ok(overlaps)
	}

	/// Background optimization scheduler for shared cache optimization.
	///
	/// This is a stub. In a production system, this would spawn a background task (tokio or std thread)
	/// that periodically scans for suboptimal shared node/cache state and triggers optimization routines.
	///
	/// Limitations:
	/// - Not implemented: no actual background task or optimization logic yet.
	/// - TODO: Integrate with watch registration/removal, transaction coordination, and error handling.
	/// - TODO: Expose control via API/config (start/stop, interval, diagnostics).
	pub fn start_optimization_scheduler(&self) {
		// TODO: Implement background task using tokio::spawn or std::thread::spawn
		// This should periodically call self.optimize_shared_cache().
		// For now, this is a no-op.
	}

	/// Optimize the shared cache by analyzing overlap statistics and node reference counts.
	///
	/// This implementation is minimal and only logs detected overlaps. In production, this should
	/// trigger cache merges/splits and update shared node state as needed.
	pub async fn optimize_shared_cache(&self) {
		// Gather overlap statistics
		let overlaps = match self.compute_overlap_statistics().await {
			Ok(o) => o,
			Err(e) => {
				eprintln!("[MultiWatchDatabase] Failed to compute overlap statistics: {e}");
				return;
			}
		};
		for overlap in overlaps {
			match overlap {
				crate::database::storage::multi_watch::WatchOverlap::Partial {
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
				crate::database::storage::multi_watch::WatchOverlap::Ancestor {
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
		// TODO: Remove redundant watch-specific nodes and clean up orphaned shared nodes
	}

	/// Merge nodes at the given path into a shared node for the specified watches.
	/// This is a minimal implementation: it creates/updates a SharedNodeInfo entry in SHARED_NODES.
	async fn merge_nodes_to_shared(
		&self,
		path: &std::path::Path,
		watch_ids: &[uuid::Uuid],
	) -> Result<(), String> {
		use crate::database::types::{FilesystemNode, SharedNodeInfo, UnifiedNode};
		use chrono::Utc;
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
				path_hash: 0,
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
		// Store in SHARED_NODES table (synchronously for now)
		let key = crate::database::types::StorageKey::PathHash(0).to_bytes(); // TODO: use real hash
		let value =
			bincode::serialize(&UnifiedNode::Shared { shared_info }).map_err(|e| e.to_string())?;
		let db = self.database.begin_write().map_err(|e| e.to_string())?;
		{
			let mut table = db
				.open_table(crate::database::storage::tables::SHARED_NODES)
				.map_err(|e| e.to_string())?;
			table
				.insert(key.as_slice(), value.as_slice())
				.map_err(|e| e.to_string())?;
		}
		db.commit().map_err(|e| e.to_string())?;
		Ok(())
	}
}

// TODO: In a real implementation, transaction metadata should be persisted in a dedicated table (e.g., WATCH_TRANSACTIONS),
// and all watch-scoped mutations should be coordinated through this mechanism. This stub is for API completeness and future expansion.
