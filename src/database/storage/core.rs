//! Core database storage traits and implementation
//!
//! This module defines the main storage traits and provides the primary
//! RedbStorage implementation that coordinates all storage operations.

use super::filesystem_cache::trait_def::FilesystemCacheStorage;
use super::filesystem_cache::RedbFilesystemCache;
use crate::database::{
	config::DatabaseConfig,
	error::DatabaseResult,
	types::{DatabaseStats, EventRecord, MetadataRecord, StorageKey},
};
use chrono::{DateTime, Utc};
use redb::{Database, ReadableMultimapTable, ReadableTable};
use std::any::Any;
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

/// Main trait for database storage operations
#[async_trait::async_trait]
pub trait DatabaseStorage: Send + Sync {
	fn as_any(&self) -> &dyn Any;

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

	/// Find events by time range
	async fn find_events_by_time_range(
		&mut self, start: DateTime<Utc>, end: DateTime<Utc>,
	) -> DatabaseResult<Vec<EventRecord>>;

	/// Clean up expired events
	async fn cleanup_expired_events(&mut self, before: SystemTime) -> DatabaseResult<usize>;

	/// Clean up events using a configurable retention policy
	async fn cleanup_events_with_policy(
		&mut self, config: &crate::database::storage::event_retention::EventRetentionConfig,
	) -> DatabaseResult<usize>;

	/// Get database statistics
	async fn get_stats(&self) -> DatabaseResult<DatabaseStats>;

	/// Compact database
	async fn compact(&mut self) -> DatabaseResult<()>;

	/// Close the database
	async fn close(self) -> DatabaseResult<()>;

	/// --- Filesystem cache methods ---
	async fn store_filesystem_node(
		&mut self, watch_id: &uuid::Uuid, node: &crate::database::types::FilesystemNode,
		event_type: &str,
	) -> crate::database::error::DatabaseResult<()>;

	async fn get_filesystem_node(
		&mut self, watch_id: &uuid::Uuid, path: &std::path::Path,
	) -> crate::database::error::DatabaseResult<Option<crate::database::types::FilesystemNode>>;

	async fn list_directory_for_watch(
		&mut self, watch_id: &uuid::Uuid, parent_path: &std::path::Path,
	) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>;

	async fn batch_store_filesystem_nodes(
		&mut self, watch_id: &uuid::Uuid, nodes: &[crate::database::types::FilesystemNode],
		event_type: &str,
	) -> crate::database::error::DatabaseResult<()>;

	async fn store_watch_metadata(
		&mut self, metadata: &crate::database::types::WatchMetadata,
	) -> crate::database::error::DatabaseResult<()>;

	async fn get_watch_metadata(
		&mut self, watch_id: &uuid::Uuid,
	) -> crate::database::error::DatabaseResult<Option<crate::database::types::WatchMetadata>>;

	/// Delete all events older than the given cutoff timestamp
	async fn delete_events_older_than(
		&mut self, cutoff: std::time::SystemTime,
	) -> DatabaseResult<usize>;

	/// Count total number of events in the log
	async fn count_events(&self) -> DatabaseResult<usize>;

	/// Delete the N oldest events from the log
	async fn delete_oldest_events(&mut self, n: usize) -> DatabaseResult<usize>;

	/// Retrieve a single filesystem node for a specific watch (single-node query).
	/// Returns the node if present, or None if not found. This is a fundamental API for cache lookups.
	/// TODO: Edge cases: path normalization, cross-platform semantics, and stale cache entries.
	async fn get_node(
		&mut self, watch_id: &uuid::Uuid, path: &std::path::Path,
	) -> crate::database::error::DatabaseResult<Option<crate::database::types::FilesystemNode>>;

	/// Pattern-based search for nodes (e.g., glob, regex).
	async fn search_nodes(
		&mut self, pattern: &str,
	) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>;
}

/// Primary ReDB implementation that coordinates all storage modules
pub struct RedbStorage {
	database: Arc<Database>,
	config: DatabaseConfig,
}

impl RedbStorage {
	/// Create a new RedbStorage instance
	pub async fn new(config: DatabaseConfig) -> DatabaseResult<Self> {
		let database = Database::create(&config.database_path)?;
		let database = Arc::new(database);

		let mut storage = Self { database, config };
		storage.initialize().await?;
		Ok(storage)
	}

	/// Get reference to the underlying database
	pub fn database(&self) -> &Arc<Database> {
		&self.database
	}

	/// Get reference to the configuration
	pub fn config(&self) -> &DatabaseConfig {
		&self.config
	}

	fn cache(&self) -> RedbFilesystemCache {
		RedbFilesystemCache::new(self.database.clone())
	}
}

#[async_trait::async_trait]
impl DatabaseStorage for RedbStorage {
	fn as_any(&self) -> &dyn Any {
		self
	}

	async fn initialize(&mut self) -> DatabaseResult<()> {
		// Initialize all required tables through specialized modules
		super::tables::initialize_tables(&self.database).await?;
		Ok(())
	}

	async fn store_event(&mut self, record: &EventRecord) -> DatabaseResult<()> {
		super::event_storage::store_event(&self.database, record).await
	}

	async fn get_events(&mut self, key: &StorageKey) -> DatabaseResult<Vec<EventRecord>> {
		super::event_storage::get_events(&self.database, key).await
	}

	async fn store_metadata(&mut self, record: &MetadataRecord) -> DatabaseResult<()> {
		super::metadata_storage::store_metadata(&self.database, record).await
	}

	async fn get_metadata(&mut self, path: &Path) -> DatabaseResult<Option<MetadataRecord>> {
		super::metadata_storage::get_metadata(&self.database, path).await
	}

	async fn find_events_by_time_range(
		&mut self, start: DateTime<Utc>, end: DateTime<Utc>,
	) -> DatabaseResult<Vec<EventRecord>> {
		super::indexing::find_events_by_time_range(&self.database, start, end).await
	}

	async fn cleanup_expired_events(&mut self, before: SystemTime) -> DatabaseResult<usize> {
		super::maintenance::cleanup_expired_events(&self.database, before).await
	}

	async fn cleanup_events_with_policy(
		&mut self, config: &crate::database::storage::event_retention::EventRetentionConfig,
	) -> DatabaseResult<usize> {
		// Use the new event_retention logic for cleanup
		crate::database::storage::event_retention::cleanup_old_events(self, config).await
	}

	async fn get_stats(&self) -> DatabaseResult<DatabaseStats> {
		super::maintenance::get_database_stats(&self.database).await
	}

	async fn compact(&mut self) -> DatabaseResult<()> {
		super::maintenance::compact_database(&self.database).await
	}

	async fn close(self) -> DatabaseResult<()> {
		// ReDB handles closing automatically when dropped
		Ok(())
	}

	async fn store_filesystem_node(
		&mut self, watch_id: &uuid::Uuid, node: &crate::database::types::FilesystemNode,
		event_type: &str,
	) -> crate::database::error::DatabaseResult<()> {
		let mut cache = self.cache();
		cache.store_filesystem_node(watch_id, node, event_type).await
	}

	async fn get_filesystem_node(
		&mut self, watch_id: &uuid::Uuid, path: &std::path::Path,
	) -> crate::database::error::DatabaseResult<Option<crate::database::types::FilesystemNode>> {
		let mut cache = self.cache();
		cache.get_filesystem_node(watch_id, path).await
	}

	async fn list_directory_for_watch(
		&mut self, watch_id: &uuid::Uuid, parent_path: &std::path::Path,
	) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>> {
		let mut cache = self.cache();
		cache.list_directory_for_watch(watch_id, parent_path).await
	}

	async fn batch_store_filesystem_nodes(
		&mut self, watch_id: &uuid::Uuid, nodes: &[crate::database::types::FilesystemNode],
		event_type: &str,
	) -> crate::database::error::DatabaseResult<()> {
		let mut cache = self.cache();
		cache.batch_store_filesystem_nodes(watch_id, nodes, event_type).await
	}

	async fn store_watch_metadata(
		&mut self, metadata: &crate::database::types::WatchMetadata,
	) -> crate::database::error::DatabaseResult<()> {
		let mut cache = self.cache();
		cache.store_watch_metadata(metadata).await
	}

	async fn get_watch_metadata(
		&mut self, watch_id: &uuid::Uuid,
	) -> crate::database::error::DatabaseResult<Option<crate::database::types::WatchMetadata>> {
		let mut cache = self.cache();
		cache.get_watch_metadata(watch_id).await
	}

	async fn delete_events_older_than(
		&mut self, cutoff: std::time::SystemTime,
	) -> DatabaseResult<usize> {
		// WARNING: This implementation iterates all events. Performance will degrade with large logs.
		// For production, use an indexed timestamp or batch delete if supported by backend.
		use crate::database::types::EventRecord;
		let write_txn = self.database.begin_write()?;
		let mut events_log =
			write_txn.open_multimap_table(crate::database::storage::tables::EVENTS_LOG_TABLE)?;
		let mut stats_table =
			write_txn.open_table(crate::database::storage::tables::STATS_TABLE)?;
		let mut removed = 0;
		let mut to_remove = Vec::new();
		for entry in events_log.iter()? {
			let (key_guard, multimap_value) = entry?;
			let key_bytes = key_guard.value();
			for value_result in multimap_value {
				let value_guard = match value_result {
					Ok(v) => v,
					Err(_) => continue, // Skip corrupt
				};
				let value_bytes = value_guard.value();
				let record: EventRecord = match bincode::deserialize(value_bytes) {
					Ok(r) => r,
					Err(_) => continue, // Skip corrupt
				};
				if record.timestamp < chrono::DateTime::<chrono::Utc>::from(cutoff) {
					to_remove.push((key_bytes.to_vec(), value_bytes.to_vec()));
				}
			}
		}
		// Decrement persistent event counter for each event removed
		let count_bytes = stats_table.get(crate::database::storage::tables::EVENT_COUNT_KEY)?;
		let mut count = count_bytes
			.map(|v| u64::from_le_bytes(v.value().try_into().unwrap_or([0u8; 8])))
			.unwrap_or(0);
		for (key, value) in to_remove {
			events_log.remove(key.as_slice(), value.as_slice())?;
			removed += 1;
			count = count.saturating_sub(1);
		}
		stats_table.insert(
			crate::database::storage::tables::EVENT_COUNT_KEY,
			&count.to_le_bytes()[..],
		)?;
		drop(stats_table);
		drop(events_log); // Ensure tables are dropped before committing
		write_txn.commit()?;
		Ok(removed)
	}

	async fn count_events(&self) -> DatabaseResult<usize> {
		let read_txn = self.database.begin_read()?;
		let events_log =
			read_txn.open_multimap_table(crate::database::storage::tables::EVENTS_LOG_TABLE)?;
		let mut count = 0;
		for entry in events_log.iter()? {
			let (_key_guard, multimap_value) = entry?;
			for value_result in multimap_value {
				if value_result.is_ok() {
					count += 1;
				}
			}
		}
		Ok(count)
	}

	async fn delete_oldest_events(&mut self, n: usize) -> DatabaseResult<usize> {
		// WARNING: This implementation loads all events into memory to sort by timestamp.
		// This is not scalable for very large logs.
		use crate::database::types::EventRecord;
		let write_txn = self.database.begin_write()?;
		let mut events_log =
			write_txn.open_multimap_table(crate::database::storage::tables::EVENTS_LOG_TABLE)?;
		let mut stats_table =
			write_txn.open_table(crate::database::storage::tables::STATS_TABLE)?;
		let mut all_events = Vec::new();
		for entry in events_log.iter()? {
			let (key_guard, multimap_value) = entry?;
			let key_bytes = key_guard.value();
			for value_result in multimap_value {
				let value_guard = match value_result {
					Ok(v) => v,
					Err(_) => continue,
				};
				let value_bytes = value_guard.value();
				let record: EventRecord = match bincode::deserialize(value_bytes) {
					Ok(r) => r,
					Err(_) => continue,
				};
				all_events.push((record.timestamp, key_bytes.to_vec(), value_bytes.to_vec()));
			}
		}
		all_events.sort_by_key(|(ts, _, _)| *ts);
		let mut removed = 0;
		// Decrement persistent event counter for each event removed
		let count_bytes = stats_table.get(crate::database::storage::tables::EVENT_COUNT_KEY)?;
		let mut count = count_bytes
			.map(|v| u64::from_le_bytes(v.value().try_into().unwrap_or([0u8; 8])))
			.unwrap_or(0);
		for (_ts, key, value) in all_events.into_iter().take(n) {
			events_log.remove(key.as_slice(), value.as_slice())?;
			removed += 1;
			count = count.saturating_sub(1);
		}
		stats_table.insert(
			crate::database::storage::tables::EVENT_COUNT_KEY,
			&count.to_le_bytes()[..],
		)?;
		drop(stats_table);
		drop(events_log); // Ensure table is dropped before committing
		write_txn.commit()?;
		Ok(removed)
	}

	async fn get_node(
		&mut self, watch_id: &uuid::Uuid, path: &std::path::Path,
	) -> crate::database::error::DatabaseResult<Option<crate::database::types::FilesystemNode>> {
		let mut cache = self.cache();
		cache.get_node(watch_id, path).await
	}

	async fn search_nodes(
		&mut self, pattern: &str,
	) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>> {
		self.cache().search_nodes(pattern).await
	}
}

impl RedbStorage {
	/// Remove a filesystem node for a specific watch (test passthrough)
	pub async fn remove_filesystem_node(
		&mut self, watch_id: &uuid::Uuid, path: &std::path::Path, event_type: &str,
	) -> crate::database::error::DatabaseResult<()> {
		let mut cache = self.cache();
		cache.remove_filesystem_node(watch_id, path, event_type).await
	}

	/// Repair stats counters (test passthrough)
	pub async fn repair_stats_counters(
		&mut self, watch_id: Option<&uuid::Uuid>, path: Option<&std::path::Path>,
	) -> crate::database::error::DatabaseResult<usize> {
		let mut cache = self.cache();
		cache.repair_stats_counters(watch_id, path).await
	}
}

/// Dummy struct for module visibility diagnostics
pub struct CoreTest;

#[cfg(test)]
mod tests {
	use super::*;
	use std::path::PathBuf;
	use tempfile::tempdir;

	async fn create_test_storage(test_name: &str) -> DatabaseResult<RedbStorage> {
		let temp_dir = tempdir().unwrap();
		let db_path = temp_dir.path().join(format!("{test_name}.redb"));
		let config =
			DatabaseConfig { database_path: db_path, ..DatabaseConfig::for_small_directories() };
		RedbStorage::new(config).await
	}

	#[tokio::test]
	async fn test_storage_initialization() {
		let _storage = create_test_storage("test_storage_initialization").await.unwrap();
	}

	#[tokio::test]
	async fn test_basic_operations() {
		let mut storage = create_test_storage("test_basic_operations").await.unwrap();

		// Test event storage
		let event_record = EventRecord::new(
			"created".to_string(),
			PathBuf::from("/test/file.txt"),
			false,
			chrono::Duration::hours(24),
			0, // sequence_number (dummy, will be overwritten by storage)
		);

		storage.store_event(&event_record).await.unwrap();

		// Test stats
		let _stats = storage.get_stats().await.unwrap();
		// total_events is u64, always >= 0; this check is redundant.
		// assert!(stats.total_events >= 0);
	}
}
