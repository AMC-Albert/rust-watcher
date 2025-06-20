//! Event storage operations
//!
//! This module handles storage and retrieval of filesystem events.
//! Focused on basic CRUD operations for EventRecord instances.

use crate::database::{
	error::DatabaseResult,
	types::{EventRecord, StorageKey},
};
use redb::Database;
use std::sync::Arc;

/// Store an event record using the provided database
pub async fn store_event(database: &Arc<Database>, record: &EventRecord) -> DatabaseResult<()> {
	let write_txn = database.begin_write()?;
	{
		let mut events_log = write_txn.open_multimap_table(super::tables::EVENTS_LOG_TABLE)?;
		let key = StorageKey::path_hash(&record.path);
		let key_bytes = key.to_bytes();
		let record_bytes = bincode::serialize(record)
			.map_err(|e| crate::database::error::DatabaseError::Serialization(e.to_string()))?;

		events_log.insert(key_bytes.as_slice(), record_bytes.as_slice())?;
	}
	write_txn.commit()?;
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
