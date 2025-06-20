//! Event storage operations
//!
//! This module handles storage and retrieval of filesystem events.
//! Focused on basic CRUD operations for EventRecord instances.

use crate::database::{
	error::DatabaseResult,
	types::{EventRecord, StorageKey},
};
use redb::{Database, ReadableTable};
use std::sync::Arc;

/// Trait for event storage operations
#[async_trait::async_trait]
pub trait EventStorage: Send + Sync {
	/// Store an event record
	async fn store_event(&mut self, record: &EventRecord) -> DatabaseResult<()>;

	/// Retrieve events by storage key
	async fn get_events(&mut self, key: &StorageKey) -> DatabaseResult<Vec<EventRecord>>;

	/// Remove an event by storage key
	async fn remove_event(&mut self, key: &StorageKey) -> DatabaseResult<bool>;

	/// List all events (optionally limited)
	async fn list_events(&mut self, limit: Option<usize>) -> DatabaseResult<Vec<EventRecord>>;
}

/// Implementation of event storage using ReDB
pub struct EventStorageImpl {
	database: Arc<Database>,
}

impl EventStorageImpl {
	pub fn new(database: Arc<Database>) -> Self {
		Self { database }
	}

	/// Initialize event storage tables
	pub async fn initialize(&mut self, _database: &Arc<Database>) -> DatabaseResult<()> {
		let write_txn = self.database.begin_write()?;
		{
			// Create events table if it doesn't exist
			let _events_table = write_txn.open_table(super::tables::EVENTS_TABLE)?;
			let _indexes_table = write_txn.open_multimap_table(super::tables::INDEXES_TABLE)?;
		}
		write_txn.commit()?;
		Ok(())
	}

	/// Serialize record to bytes
	fn serialize_record(record: &EventRecord) -> DatabaseResult<Vec<u8>> {
		bincode::serialize(record)
			.map_err(|e| crate::database::error::DatabaseError::Serialization(e.to_string()))
	}

	/// Deserialize bytes to record
	fn deserialize_record(bytes: &[u8]) -> DatabaseResult<EventRecord> {
		bincode::deserialize(bytes)
			.map_err(|e| crate::database::error::DatabaseError::Deserialization(e.to_string()))
	}
}

#[async_trait::async_trait]
impl EventStorage for EventStorageImpl {
	async fn store_event(&mut self, record: &EventRecord) -> DatabaseResult<()> {
		let write_txn = self.database.begin_write()?;
		{
			let mut events_log = write_txn.open_multimap_table(super::tables::EVENTS_LOG_TABLE)?;
			let key = StorageKey::path_hash(&record.path);
			let key_bytes = key.to_bytes();
			let record_bytes = Self::serialize_record(record)?;

			events_log.insert(key_bytes.as_slice(), record_bytes.as_slice())?;
		}
		write_txn.commit()?;
		Ok(())
	}

	async fn get_events(&mut self, key: &StorageKey) -> DatabaseResult<Vec<EventRecord>> {
		let read_txn = self.database.begin_read()?;
		let events_log = read_txn.open_multimap_table(super::tables::EVENTS_LOG_TABLE)?;
		let key_bytes = key.to_bytes();
		let mut events = Vec::new();
		let multimap = events_log.get(key_bytes.as_slice())?;
		for item in multimap {
			let value = item?;
			let record = bincode::deserialize::<EventRecord>(value.value()).map_err(|e| {
				crate::database::error::DatabaseError::Deserialization(e.to_string())
			})?;
			events.push(record);
		}
		// TODO: Consider sorting by timestamp if order is not guaranteed by storage
		Ok(events)
	}

	async fn remove_event(&mut self, key: &StorageKey) -> DatabaseResult<bool> {
		let write_txn = self.database.begin_write()?;
		let removed = {
			let mut events_table = write_txn.open_table(super::tables::EVENTS_TABLE)?;
			let key_bytes = key.to_bytes();

			let existed = events_table.get(key_bytes.as_slice())?.is_some();
			if existed {
				events_table.remove(key_bytes.as_slice())?;
			}
			existed
		};
		write_txn.commit()?;
		Ok(removed)
	}

	async fn list_events(&mut self, limit: Option<usize>) -> DatabaseResult<Vec<EventRecord>> {
		let read_txn = self.database.begin_read()?;
		let events_table = read_txn.open_table(super::tables::EVENTS_TABLE)?;

		let mut events = Vec::new();
		let iter = events_table.iter()?;

		for (count, item) in iter.enumerate() {
			if let Some(max_limit) = limit {
				if count >= max_limit {
					break;
				}
			}

			let (_, value) = item?;
			let record = Self::deserialize_record(value.value())?;
			events.push(record);
		}

		Ok(events)
	}
}

/// Store an event record using the provided database
pub async fn store_event(_database: &Arc<Database>, _record: &EventRecord) -> DatabaseResult<()> {
	// TODO: Implement event storage
	// For now, return success - this would be implemented properly in Phase 1.2
	Ok(())
}

/// Retrieve events by storage key using the provided database
pub async fn get_events(
	database: &Arc<Database>,
	key: &StorageKey,
) -> DatabaseResult<Vec<EventRecord>> {
	let read_txn = database.begin_read()?;
	let events_log = read_txn.open_multimap_table(super::tables::EVENTS_LOG_TABLE)?;
	let key_bytes = key.to_bytes();
	let multimap = events_log.get(key_bytes.as_slice())?;
	let mut events = Vec::new();
	for item in multimap {
		let value = item?;
		let record = bincode::deserialize::<EventRecord>(value.value())
			.map_err(|e| crate::database::error::DatabaseError::Deserialization(e.to_string()))?;
		events.push(record);
	}
	// TODO: Consider sorting by timestamp if order is not guaranteed by storage
	Ok(events)
}
