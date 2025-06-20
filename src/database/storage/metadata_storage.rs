//! Metadata storage operations
//!
//! This module handles storage and retrieval of file metadata records.
//! Provides efficient path-based lookups and prefix-based queries.

use crate::database::{error::DatabaseResult, types::MetadataRecord};
use redb::{Database, ReadableTable};
use std::{path::Path, sync::Arc};

/// Trait for metadata storage operations
#[async_trait::async_trait]
pub trait MetadataStorage: Send + Sync {
	/// Store a metadata record
	async fn store_metadata(&mut self, record: &MetadataRecord) -> DatabaseResult<()>;

	/// Retrieve metadata by path
	async fn get_metadata(&mut self, path: &Path) -> DatabaseResult<Option<MetadataRecord>>;

	/// Remove metadata by path
	async fn remove_metadata(&mut self, path: &Path) -> DatabaseResult<bool>;

	/// List metadata records by path prefix
	async fn list_metadata(&mut self, prefix: Option<&str>) -> DatabaseResult<Vec<MetadataRecord>>;
}

/// Implementation of metadata storage using ReDB
pub struct MetadataStorageImpl {
	database: Arc<Database>,
}

impl MetadataStorageImpl {
	pub fn new(database: Arc<Database>) -> Self {
		Self { database }
	}

	/// Initialize metadata storage tables
	pub async fn initialize(&mut self, _database: &Arc<Database>) -> DatabaseResult<()> {
		let write_txn = self.database.begin_write()?;
		{
			// Create metadata table if it doesn't exist
			let _metadata_table = write_txn.open_table(super::tables::METADATA_TABLE)?;
		}
		write_txn.commit()?;
		Ok(())
	}

	/// Calculate path hash for consistent indexing
	fn path_hash(path: &Path) -> u64 {
		crate::database::types::calculate_path_hash(path)
	}

	/// Serialize record to bytes
	fn serialize_record(record: &MetadataRecord) -> DatabaseResult<Vec<u8>> {
		bincode::serialize(record)
			.map_err(|e| crate::database::error::DatabaseError::Serialization(e.to_string()))
	}

	/// Deserialize bytes to record
	fn deserialize_record(bytes: &[u8]) -> DatabaseResult<MetadataRecord> {
		bincode::deserialize(bytes)
			.map_err(|e| crate::database::error::DatabaseError::Deserialization(e.to_string()))
	}
}

#[async_trait::async_trait]
impl MetadataStorage for MetadataStorageImpl {
	async fn store_metadata(&mut self, record: &MetadataRecord) -> DatabaseResult<()> {
		let write_txn = self.database.begin_write()?;
		{
			let mut metadata_table = write_txn.open_table(super::tables::METADATA_TABLE)?;
			let path_hash = Self::path_hash(&record.path);
			let key_bytes = path_hash.to_le_bytes();
			let record_bytes = Self::serialize_record(record)?;

			metadata_table.insert(key_bytes.as_slice(), record_bytes.as_slice())?;
		}
		write_txn.commit()?;
		Ok(())
	}

	async fn get_metadata(&mut self, path: &Path) -> DatabaseResult<Option<MetadataRecord>> {
		let read_txn = self.database.begin_read()?;
		let metadata_table = read_txn.open_table(super::tables::METADATA_TABLE)?;

		let path_hash = Self::path_hash(path);
		let key_bytes = path_hash.to_le_bytes();

		if let Some(record_bytes) = metadata_table.get(key_bytes.as_slice())? {
			let record = Self::deserialize_record(record_bytes.value())?;
			Ok(Some(record))
		} else {
			Ok(None)
		}
	}

	async fn remove_metadata(&mut self, path: &Path) -> DatabaseResult<bool> {
		let write_txn = self.database.begin_write()?;
		let removed = {
			let mut metadata_table = write_txn.open_table(super::tables::METADATA_TABLE)?;
			let path_hash = Self::path_hash(path);
			let key_bytes = path_hash.to_le_bytes();

			let existed = metadata_table.get(key_bytes.as_slice())?.is_some();
			if existed {
				metadata_table.remove(key_bytes.as_slice())?;
			}
			existed
		};
		write_txn.commit()?;
		Ok(removed)
	}

	async fn list_metadata(&mut self, prefix: Option<&str>) -> DatabaseResult<Vec<MetadataRecord>> {
		let read_txn = self.database.begin_read()?;
		let metadata_table = read_txn.open_table(super::tables::METADATA_TABLE)?;

		let mut metadata_records = Vec::new();
		let mut iter = metadata_table.iter()?;

		while let Some(item) = iter.next() {
			let (_, value) = item?;
			let record = Self::deserialize_record(value.value())?;

			// Apply prefix filter if specified
			if let Some(prefix_str) = prefix {
				if !record.path.to_string_lossy().starts_with(prefix_str) {
					continue;
				}
			}

			metadata_records.push(record);
		}

		Ok(metadata_records)
	}
}

/// Store metadata record using the provided database
pub async fn store_metadata(
	database: &Arc<Database>,
	record: &MetadataRecord,
) -> DatabaseResult<()> {
	// TODO: Implement metadata storage
	// For now, return success - this would be implemented properly in Phase 1.2
	Ok(())
}

/// Retrieve metadata by path using the provided database
pub async fn get_metadata(
	database: &Arc<Database>,
	path: &Path,
) -> DatabaseResult<Option<MetadataRecord>> {
	// TODO: Implement metadata retrieval
	// For now, return None - this would be implemented properly in Phase 1.2
	Ok(None)
}
