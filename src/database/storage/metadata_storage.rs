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
			let mut stats_table = write_txn.open_table(super::tables::STATS_TABLE)?;
			let path_hash = Self::path_hash(&record.path);
			let key_bytes = path_hash.to_le_bytes();
			let record_bytes = Self::serialize_record(record)?;

			let existed = metadata_table.get(key_bytes.as_slice())?.is_some();
			metadata_table.insert(key_bytes.as_slice(), record_bytes.as_slice())?;
			// Only increment if this is a new record
			if !existed {
				let count_bytes = stats_table.get(super::tables::METADATA_COUNT_KEY)?;
				let mut count = count_bytes
					.map(|v| u64::from_le_bytes(v.value().try_into().unwrap_or([0u8; 8])))
					.unwrap_or(0);
				count = count.saturating_add(1);
				stats_table.insert(super::tables::METADATA_COUNT_KEY, &count.to_le_bytes()[..])?;
			}
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
			let mut stats_table = write_txn.open_table(super::tables::STATS_TABLE)?;
			let path_hash = Self::path_hash(path);
			let key_bytes = path_hash.to_le_bytes();

			let existed = metadata_table.get(key_bytes.as_slice())?.is_some();
			if existed {
				metadata_table.remove(key_bytes.as_slice())?;
				let count_bytes = stats_table.get(super::tables::METADATA_COUNT_KEY)?;
				let mut count = count_bytes
					.map(|v| u64::from_le_bytes(v.value().try_into().unwrap_or([0u8; 8])))
					.unwrap_or(0);
				count = count.saturating_sub(1);
				stats_table.insert(super::tables::METADATA_COUNT_KEY, &count.to_le_bytes()[..])?;
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
		let iter = metadata_table.iter()?;

		for item in iter {
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
	let write_txn = database.begin_write()?;
	{
		let mut metadata_table = write_txn.open_table(super::tables::METADATA_TABLE)?;
		let path_hash = crate::database::types::calculate_path_hash(&record.path);
		let key_bytes = path_hash.to_le_bytes();
		let record_bytes = bincode::serialize(record)
			.map_err(|e| crate::database::error::DatabaseError::Serialization(e.to_string()))?;
		metadata_table.insert(key_bytes.as_slice(), record_bytes.as_slice())?;
	}
	write_txn.commit()?;
	Ok(())
}

/// Retrieve metadata by path using the provided database
pub async fn get_metadata(
	database: &Arc<Database>,
	path: &Path,
) -> DatabaseResult<Option<MetadataRecord>> {
	let read_txn = database.begin_read()?;
	let metadata_table = read_txn.open_table(super::tables::METADATA_TABLE)?;
	let path_hash = crate::database::types::calculate_path_hash(path);
	let key_bytes = path_hash.to_le_bytes();
	if let Some(record_bytes) = metadata_table.get(key_bytes.as_slice())? {
		let record = bincode::deserialize::<MetadataRecord>(record_bytes.value())
			.map_err(|e| crate::database::error::DatabaseError::Deserialization(e.to_string()))?;
		Ok(Some(record))
	} else {
		Ok(None)
	}
}
