//! Internal helpers for filesystem cache storage
//!
//! Contains serialization, deserialization, and key conversion utilities.
//!
//! These are not part of the public API.

use crate::database::error::DatabaseResult;
use crate::database::types::WatchScopedKey;

/// Serialize data to bytes
pub fn serialize<T: serde::Serialize>(data: &T) -> DatabaseResult<Vec<u8>> {
	use crate::database::error::DatabaseError;
	bincode::serialize(data).map_err(|e| DatabaseError::Serialization(e.to_string()))
}

/// Deserialize bytes to data
pub fn deserialize<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> DatabaseResult<T> {
	use crate::database::error::DatabaseError;
	bincode::deserialize(bytes).map_err(|e| DatabaseError::Deserialization(e.to_string()))
}

/// Convert key to bytes for storage
pub fn key_to_bytes(key: &WatchScopedKey) -> Vec<u8> {
	serialize(key).unwrap_or_default()
}
