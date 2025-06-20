//! Core database storage traits and implementation
//!
//! This module defines the main storage traits and provides the primary
//! RedbStorage implementation that coordinates all storage operations.

use crate::database::{
	config::DatabaseConfig,
	error::DatabaseResult,
	types::{DatabaseStats, EventRecord, MetadataRecord, StorageKey},
};
use chrono::{DateTime, Utc};
use redb::Database;
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

/// Main trait for database storage operations
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

	/// Compact database
	async fn compact(&mut self) -> DatabaseResult<()>;

	/// Close the database
	async fn close(self) -> DatabaseResult<()>;
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
}

#[async_trait::async_trait]
impl DatabaseStorage for RedbStorage {
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
		&mut self,
		start: DateTime<Utc>,
		end: DateTime<Utc>,
	) -> DatabaseResult<Vec<EventRecord>> {
		super::indexing::find_events_by_time_range(&self.database, start, end).await
	}

	async fn cleanup_expired_events(&mut self, before: SystemTime) -> DatabaseResult<usize> {
		super::maintenance::cleanup_expired_events(&self.database, before).await
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
}

/// Dummy struct for module visibility diagnostics
pub struct CoreTest;

#[cfg(test)]
mod tests {
	use super::*;
	use std::path::PathBuf;
	use tempfile::tempdir;

	async fn create_test_storage() -> DatabaseResult<RedbStorage> {
		let temp_dir = tempdir().unwrap();
		let _db_path = temp_dir.path().join("test.db");
		let config = DatabaseConfig::for_small_directories();
		RedbStorage::new(config).await
	}

	#[tokio::test]
	async fn test_storage_initialization() {
		let _storage = create_test_storage().await.unwrap();
		// TODO: Implement a real check for database validity if needed
		// assert!(storage.database().is_ok());
	}

	#[tokio::test]
	async fn test_basic_operations() {
		let mut storage = create_test_storage().await.unwrap();

		// Test event storage
		let event_record = EventRecord::new(
			"created".to_string(),
			PathBuf::from("/test/file.txt"),
			false,
			chrono::Duration::hours(24),
		);

		storage.store_event(&event_record).await.unwrap();

		// Test stats
		let _stats = storage.get_stats().await.unwrap();
		// total_events is u64, always >= 0; this check is redundant.
		// assert!(stats.total_events >= 0);
	}
}
