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

// TODO: In a real implementation, transaction metadata should be persisted in a dedicated table (e.g., WATCH_TRANSACTIONS),
// and all watch-scoped mutations should be coordinated through this mechanism. This stub is for API completeness and future expansion.
