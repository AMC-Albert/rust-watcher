//! Database storage implementation using redb

use crate::database::{
	config::DatabaseConfig,
	error::{DatabaseError, DatabaseResult},
	types::{DatabaseStats, EventRecord, MetadataRecord, StorageKey},
};
use chrono::{DateTime, Utc};
use redb::{Database, MultimapTableDefinition, ReadableTable, TableDefinition};
#[allow(unused_imports)]
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use tracing::{debug, info, warn};

// Table definitions for redb
const EVENTS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("events");
const METADATA_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("metadata");
const INDEXES_TABLE: MultimapTableDefinition<&[u8], &[u8]> =
	MultimapTableDefinition::new("indexes");

/// Trait for database storage operations
#[async_trait::async_trait]
pub trait DatabaseStorage: Send + Sync {
	/// Initialize the database
	async fn initialize(&mut self) -> DatabaseResult<()>;

	/// Store an event record
	async fn store_event(&mut self, record: &EventRecord) -> DatabaseResult<()>;
	/// Retrieve events by key
	async fn get_events(&mut self, key: &StorageKey) -> DatabaseResult<Vec<EventRecord>>;

	/// Store metadata record
	async fn store_metadata(&mut self, record: &MetadataRecord) -> DatabaseResult<()>;
	/// Retrieve metadata by path
	async fn get_metadata(&mut self, path: &Path) -> DatabaseResult<Option<MetadataRecord>>;

	/// Find events by size range
	async fn find_events_by_size(
		&mut self,
		min_size: u64,
		max_size: u64,
	) -> DatabaseResult<Vec<EventRecord>>;

	/// Find events by time range
	async fn find_events_by_time_range(
		&mut self,
		start: DateTime<Utc>,
		end: DateTime<Utc>,
	) -> DatabaseResult<Vec<EventRecord>>;

	/// Clean up expired events
	async fn cleanup_expired_events(&mut self, before: SystemTime) -> DatabaseResult<usize>;

	/// Get database statistics
	async fn get_stats(&self) -> DatabaseResult<DatabaseStats>;

	/// Compact the database to reclaim space
	async fn compact(&mut self) -> DatabaseResult<()>;

	/// Close the database connection
	async fn close(self) -> DatabaseResult<()>;
}

/// redb-based database storage implementation
pub struct RedbStorage {
	database: Arc<Database>,
	config: DatabaseConfig,
	stats: DatabaseStats,
}

impl RedbStorage {
	/// Create a new redb storage instance
	pub async fn new(config: DatabaseConfig) -> DatabaseResult<Self> {
		config
			.validate()
			.map_err(DatabaseError::InvalidConfiguration)?;

		// Create database directory if it doesn't exist
		if let Some(parent) = config.database_path.parent() {
			std::fs::create_dir_all(parent)?;
		}

		// Open or create the database
		let database = Database::create(&config.database_path).map_err(|e| {
			DatabaseError::InitializationFailed(format!("Failed to create database: {}", e))
		})?;

		debug!("Opened redb database at: {:?}", config.database_path);

		let mut storage = Self {
			database: Arc::new(database),
			config,
			stats: DatabaseStats::default(),
		};

		storage.initialize().await?;

		Ok(storage)
	}
	/// Serialize an event record
	#[allow(clippy::result_large_err)]
	fn serialize_event(record: &EventRecord) -> DatabaseResult<Vec<u8>> {
		bincode::serialize(record).map_err(DatabaseError::SerializationError)
	}

	/// Deserialize an event record
	#[allow(clippy::result_large_err)]
	fn deserialize_event(data: &[u8]) -> DatabaseResult<EventRecord> {
		bincode::deserialize(data).map_err(DatabaseError::SerializationError)
	}

	/// Serialize a metadata record
	#[allow(clippy::result_large_err)]
	fn serialize_metadata(record: &MetadataRecord) -> DatabaseResult<Vec<u8>> {
		bincode::serialize(record).map_err(DatabaseError::SerializationError)
	}

	/// Deserialize a metadata record
	#[allow(clippy::result_large_err)]
	fn deserialize_metadata(data: &[u8]) -> DatabaseResult<MetadataRecord> {
		bincode::deserialize(data).map_err(DatabaseError::SerializationError)
	}
	/// Generate index entries for an event
	fn generate_indexes(&self, record: &EventRecord) -> Vec<(StorageKey, Vec<u8>)> {
		let mut indexes = Vec::new();
		let event_key = StorageKey::EventId(record.event_id);
		let event_id_bytes = event_key.to_bytes();

		// Path hash index
		indexes.push((StorageKey::path_hash(&record.path), event_id_bytes.clone()));

		// Size bucket index
		if let Some(size) = record.size {
			indexes.push((StorageKey::size_bucket(size), event_id_bytes.clone()));
		}

		// Inode index
		if let Some(inode) = record.inode {
			indexes.push((StorageKey::Inode(inode), event_id_bytes.clone()));
		}

		// Windows ID index
		if let Some(windows_id) = record.windows_id {
			indexes.push((StorageKey::WindowsId(windows_id), event_id_bytes.clone()));
		}

		// Content hash index
		if let Some(ref content_hash) = record.content_hash {
			indexes.push((
				StorageKey::ContentHash(content_hash.clone()),
				event_id_bytes.clone(),
			));
		}

		// Time bucket index (hourly buckets)
		indexes.push((
			StorageKey::time_bucket(record.timestamp, 3600), // 1 hour buckets
			event_id_bytes,
		));

		indexes
	}
}

#[async_trait::async_trait]
impl DatabaseStorage for RedbStorage {
	async fn initialize(&mut self) -> DatabaseResult<()> {
		let write_txn = self.database.begin_write()?;
		{
			// Create tables if they don't exist
			let _events_table = write_txn.open_table(EVENTS_TABLE)?;
			let _metadata_table = write_txn.open_table(METADATA_TABLE)?;
			let _indexes_table = write_txn.open_multimap_table(INDEXES_TABLE)?;
		}
		write_txn.commit()?;

		info!("Initialized redb database with all required tables");
		Ok(())
	}

	async fn store_event(&mut self, record: &EventRecord) -> DatabaseResult<()> {
		let event_data = Self::serialize_event(record)?;
		let event_key = StorageKey::EventId(record.event_id).to_bytes();
		let indexes = self.generate_indexes(record);

		let write_txn = self.database.begin_write()?;
		{
			// Store the event
			let mut events_table = write_txn.open_table(EVENTS_TABLE)?;
			events_table.insert(event_key.as_slice(), event_data.as_slice())?; // Store indexes
			let mut indexes_table = write_txn.open_multimap_table(INDEXES_TABLE)?;
			for (index_key, index_value) in indexes {
				let index_key_bytes = index_key.to_bytes();
				indexes_table.insert(index_key_bytes.as_slice(), index_value.as_slice())?;
			}
		}
		write_txn.commit()?;

		self.stats.write_operations += 1;
		self.stats.total_events += 1;

		debug!(
			"Stored event: {} for path: {:?}",
			record.event_id, record.path
		);
		Ok(())
	}
	async fn get_events(&mut self, key: &StorageKey) -> DatabaseResult<Vec<EventRecord>> {
		let read_txn = self.database.begin_read()?;
		let mut events = Vec::new();

		match key {
			StorageKey::EventId(_event_id) => {
				let events_table = read_txn.open_table(EVENTS_TABLE)?;
				let event_key = key.to_bytes();

				if let Some(data) = events_table.get(event_key.as_slice())? {
					let event = Self::deserialize_event(data.value())?;
					events.push(event);
				}
			}
			_ => {
				// Use index to find event IDs, then fetch events
				let indexes_table = read_txn.open_multimap_table(INDEXES_TABLE)?;
				let events_table = read_txn.open_table(EVENTS_TABLE)?;
				let index_key = key.to_bytes();

				// Get all event IDs for this index key
				let event_ids = indexes_table.get(index_key.as_slice())?;

				for event_id_entry in event_ids {
					let event_id_bytes = event_id_entry?;
					if let Some(event_data) = events_table.get(event_id_bytes.value())? {
						let event = Self::deserialize_event(event_data.value())?;
						events.push(event);
					}
				}
			}
		}

		self.stats.read_operations += 1;
		Ok(events)
	}

	async fn store_metadata(&mut self, record: &MetadataRecord) -> DatabaseResult<()> {
		let metadata_data = Self::serialize_metadata(record)?;
		let metadata_key = StorageKey::path_hash(&record.path).to_bytes();

		let write_txn = self.database.begin_write()?;
		{
			let mut metadata_table = write_txn.open_table(METADATA_TABLE)?;
			metadata_table.insert(metadata_key.as_slice(), metadata_data.as_slice())?;
		}
		write_txn.commit()?;

		self.stats.write_operations += 1;
		self.stats.total_metadata += 1;

		debug!("Stored metadata for path: {:?}", record.path);
		Ok(())
	}

	async fn get_metadata(&mut self, path: &Path) -> DatabaseResult<Option<MetadataRecord>> {
		let read_txn = self.database.begin_read()?;
		let metadata_table = read_txn.open_table(METADATA_TABLE)?;
		let metadata_key = StorageKey::path_hash(path).to_bytes();
		let result = if let Some(data) = metadata_table.get(metadata_key.as_slice())? {
			Some(Self::deserialize_metadata(data.value())?)
		} else {
			None
		};

		self.stats.read_operations += 1;
		Ok(result)
	}

	async fn find_events_by_size(
		&mut self,
		min_size: u64,
		max_size: u64,
	) -> DatabaseResult<Vec<EventRecord>> {
		let read_txn = self.database.begin_read()?;
		let events_table = read_txn.open_table(EVENTS_TABLE)?;
		let mut matching_events = Vec::new();

		// Iterate through all events and filter by size
		// In a production implementation, we would use size bucket indexes for efficiency
		for item in events_table.iter()? {
			let (_, data) = item?;
			let event = Self::deserialize_event(data.value())?;

			if let Some(size) = event.size {
				if size >= min_size && size <= max_size {
					matching_events.push(event);
				}
			}
		}

		self.stats.read_operations += 1;
		Ok(matching_events)
	}
	async fn find_events_by_time_range(
		&mut self,
		start: DateTime<Utc>,
		end: DateTime<Utc>,
	) -> DatabaseResult<Vec<EventRecord>> {
		let read_txn = self.database.begin_read()?;
		let events_table = read_txn.open_table(EVENTS_TABLE)?;
		let mut matching_events = Vec::new();

		// Iterate through all events and filter by timestamp
		// In a production implementation, we would use time bucket indexes for efficiency
		for item in events_table.iter()? {
			let (_, data) = item?;
			let event = Self::deserialize_event(data.value())?;

			if event.timestamp >= start && event.timestamp <= end {
				matching_events.push(event);
			}
		}

		self.stats.read_operations += 1;
		Ok(matching_events)
	}

	async fn cleanup_expired_events(&mut self, before: SystemTime) -> DatabaseResult<usize> {
		let before_datetime = DateTime::<Utc>::from(before);
		let write_txn = self.database.begin_write()?;
		let mut removed_count = 0;

		{
			let mut events_table = write_txn.open_table(EVENTS_TABLE)?;
			let mut to_remove = Vec::new();

			// Find expired events
			for item in events_table.iter()? {
				let (key, data) = item?;
				let event = Self::deserialize_event(data.value())?;

				if event.expires_at <= before_datetime {
					to_remove.push(key.value().to_vec());
				}
			}
			// Remove expired events
			for key in to_remove {
				events_table.remove(key.as_slice())?;
				removed_count += 1;
			}
		}

		write_txn.commit()?;

		self.stats.delete_operations += removed_count as u64;
		self.stats.cleaned_up_events += removed_count as u64;
		self.stats.total_events = self.stats.total_events.saturating_sub(removed_count as u64);

		if removed_count > 0 {
			info!("Cleaned up {} expired events", removed_count);
		}

		Ok(removed_count)
	}

	async fn get_stats(&self) -> DatabaseResult<DatabaseStats> {
		// Update database size
		let mut stats = self.stats.clone();
		if let Ok(metadata) = std::fs::metadata(&self.config.database_path) {
			stats.database_size = metadata.len();
		}

		Ok(stats)
	}

	async fn compact(&mut self) -> DatabaseResult<()> {
		// redb handles compaction automatically, but we can trigger a manual compaction
		// by creating a new database and copying data over
		warn!("Manual compaction not implemented for redb - database auto-compacts");
		Ok(())
	}

	async fn close(self) -> DatabaseResult<()> {
		// redb automatically closes when dropped
		info!("Closed redb database");
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::TempDir;

	async fn create_test_storage() -> (RedbStorage, TempDir) {
		let temp_dir = TempDir::new().unwrap();
		let config = DatabaseConfig::with_path(temp_dir.path().join("test.redb"));
		let storage = RedbStorage::new(config).await.unwrap();
		(storage, temp_dir)
	}

	#[tokio::test]
	async fn test_storage_creation() {
		let (storage, _temp_dir) = create_test_storage().await;
		assert!(storage.database.begin_read().is_ok());
	}

	#[tokio::test]
	async fn test_event_storage_and_retrieval() {
		let (mut storage, _temp_dir) = create_test_storage().await;

		let event = EventRecord::new(
			"Create".to_string(),
			PathBuf::from("/test/file.txt"),
			false,
			chrono::Duration::hours(1),
		);

		let event_id = event.event_id;
		storage.store_event(&event).await.unwrap();

		let retrieved = storage
			.get_events(&StorageKey::EventId(event_id))
			.await
			.unwrap();
		assert_eq!(retrieved.len(), 1);
		assert_eq!(retrieved[0].event_id, event_id);
	}

	#[tokio::test]
	async fn test_metadata_storage() {
		let (mut storage, _temp_dir) = create_test_storage().await;

		let metadata = MetadataRecord::new(PathBuf::from("/test/file.txt"), false);
		let path = metadata.path.clone();

		storage.store_metadata(&metadata).await.unwrap();

		let retrieved = storage.get_metadata(&path).await.unwrap();
		assert!(retrieved.is_some());
		assert_eq!(retrieved.unwrap().path, path);
	}
}
