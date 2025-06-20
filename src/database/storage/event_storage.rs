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

/// Store an event record using the provided database
pub async fn store_event(database: &Arc<Database>, record: &EventRecord) -> DatabaseResult<()> {
	let write_txn = database.begin_write()?;
	{
		let mut events_log = write_txn.open_multimap_table(super::tables::EVENTS_LOG_TABLE)?;
		let mut stats_table = write_txn.open_table(super::tables::STATS_TABLE)?;
		let key = StorageKey::path_hash(&record.path);
		let key_bytes = key.to_bytes();

		// Assign sequence number
		let seq_bytes = stats_table.get(super::tables::EVENT_SEQUENCE_KEY)?;
		let mut sequence_number = seq_bytes
			.map(|v| u64::from_le_bytes(v.value().try_into().unwrap_or([0u8; 8])))
			.unwrap_or(0);

		let mut record = record.clone();
		record.sequence_number = sequence_number;
		sequence_number = sequence_number.saturating_add(1);
		stats_table.insert(
			super::tables::EVENT_SEQUENCE_KEY,
			&sequence_number.to_le_bytes()[..],
		)?;

		let record_bytes = bincode::serialize(&record)
			.map_err(|e| crate::database::error::DatabaseError::Serialization(e.to_string()))?;

		events_log.insert(key_bytes.as_slice(), record_bytes.as_slice())?;

		// Increment persistent event counter
		let count_bytes = stats_table.get(super::tables::EVENT_COUNT_KEY)?;
		let mut count = count_bytes
			.map(|v| u64::from_le_bytes(v.value().try_into().unwrap_or([0u8; 8])))
			.unwrap_or(0);
		count = count.saturating_add(1);
		stats_table.insert(super::tables::EVENT_COUNT_KEY, &count.to_le_bytes()[..])?;
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
	// Enforce append order: sort by sequence_number (ascending)
	events.sort_by_key(|e| e.sequence_number);
	Ok(events)
}
