//! Transaction management utilities
//!
//! This module provides common transaction patterns and utilities
//! for consistent error handling and resource management across all storage operations.

use crate::database::error::{DatabaseError, DatabaseResult};
use redb::{Database, ReadTransaction, WriteTransaction};
use std::sync::Arc;

/// Transaction helper utilities
pub struct TransactionUtils;

impl TransactionUtils {
	/// Execute a read operation with proper error handling
	pub async fn with_read_txn<F, R>(database: &Arc<Database>, operation: F) -> DatabaseResult<R>
	where
		F: FnOnce(&ReadTransaction) -> DatabaseResult<R>,
	{
		let read_txn = database.begin_read()?;
		operation(&read_txn)
	}

	/// Execute a write operation with proper error handling and commit
	pub async fn with_write_txn<F, R>(database: &Arc<Database>, operation: F) -> DatabaseResult<R>
	where
		F: FnOnce(&WriteTransaction) -> DatabaseResult<R>,
	{
		let write_txn = database.begin_write()?;
		let result = operation(&write_txn)?;
		write_txn.commit()?;
		Ok(result)
	}

	/// Serialize data with consistent error handling
	pub fn serialize<T>(data: &T) -> DatabaseResult<Vec<u8>>
	where
		T: serde::Serialize,
	{
		bincode::serialize(data).map_err(|e| DatabaseError::Serialization(e.to_string()))
	}

	/// Deserialize data with consistent error handling
	pub fn deserialize<T>(bytes: &[u8]) -> DatabaseResult<T>
	where
		T: serde::de::DeserializeOwned,
	{
		bincode::deserialize(bytes).map_err(|e| DatabaseError::Deserialization(e.to_string()))
	}

	/// Create a storage key from bytes
	pub fn create_key_bytes(key_data: &[u8]) -> Vec<u8> {
		key_data.to_vec()
	}

	/// Create a path hash key
	pub fn path_hash_key(path: &std::path::Path) -> Vec<u8> {
		let hash = crate::database::types::calculate_path_hash(path);
		hash.to_le_bytes().to_vec()
	}

	/// Create a UUID key
	pub fn uuid_key(uuid: &uuid::Uuid) -> Vec<u8> {
		uuid.as_bytes().to_vec()
	}

	/// Create a time bucket key for indexing
	pub fn time_bucket_key(
		timestamp: chrono::DateTime<chrono::Utc>,
		bucket_size_seconds: i64,
	) -> Vec<u8> {
		let timestamp_seconds = timestamp.timestamp();
		let bucket = (timestamp_seconds / bucket_size_seconds) * bucket_size_seconds;
		bucket.to_le_bytes().to_vec()
	}
}
