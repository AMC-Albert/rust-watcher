//! Database adapter providing a clean, decoupled interface for multiple modules
//!
//! This adapter abstracts database operations and provides a unified interface
//! that can be used by the watcher core, move detection, and future modules
//! without tight coupling to the underlying storage implementation.

use crate::database::storage::filesystem_cache::RedbFilesystemCache;
use crate::database::types::FilesystemNode;
use crate::database::{
	config::DatabaseConfig,
	error::{DatabaseError, DatabaseResult},
	storage::{DatabaseStorage, RedbStorage},
	types::{DatabaseStats, EventRecord, MetadataRecord, StorageKey},
};
use crate::events::FileSystemEvent;
use chrono::{DateTime, Utc};
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::database::background_tasks::{
	BackgroundTaskManager, CompactionTask, HealthCheckTask, StatsRefreshTask, TimeIndexRepairTask,
};
/// Metrics for background maintenance tasks
#[derive(Debug, Default, Clone)]
pub struct BackgroundMaintenanceMetrics {
	pub last_run: Option<DateTime<Utc>>,
	pub last_duration_ms: Option<u128>,
	pub last_error: Option<String>,
	pub success_count: u64,
	pub failure_count: u64,
	pub last_compaction_ok: bool,
	pub last_repair_ok: bool,
}

impl BackgroundMaintenanceMetrics {
	pub fn new() -> Self {
		Self::default()
	}
}

/// High-level database adapter that provides a clean interface for multiple modules
#[derive(Clone)]
pub struct DatabaseAdapter {
	storage: Arc<RwLock<Box<dyn DatabaseStorage>>>,
	config: DatabaseConfig,
	enabled: bool,
	maintenance_metrics: Arc<RwLock<BackgroundMaintenanceMetrics>>,
	#[allow(dead_code)]
	background_manager: Option<Arc<BackgroundTaskManager>>, // New field
}

impl DatabaseAdapter {
	/// Create a new database adapter with the given configuration
	pub async fn new(config: DatabaseConfig) -> DatabaseResult<Self> {
		let storage: Box<dyn DatabaseStorage> = Box::new(RedbStorage::new(config.clone()).await?);
		let enabled = true;
		let background_manager = if enabled {
			let mut manager = BackgroundTaskManager::new();
			// Register all tasks here
			if let Some(db) = storage
				.as_any()
				.downcast_ref::<RedbStorage>()
				.map(|redb_storage| redb_storage.get_database())
			{
				let db = db.clone();
				let repair = Arc::new(TimeIndexRepairTask { db: db.clone() });
				let compact = Arc::new(CompactionTask { db: db.clone() });
				let health = Arc::new(HealthCheckTask { db: db.clone() });
				let stats = Arc::new(StatsRefreshTask { db });
				manager.register_task(repair);
				manager.register_task(compact);
				manager.register_task(health);
				manager.register_task(stats);
			}
			Some(Arc::new(manager))
		} else {
			None
		};
		Ok(Self {
			storage: Arc::new(RwLock::new(storage)),
			config,
			enabled,
			maintenance_metrics: Arc::new(RwLock::new(BackgroundMaintenanceMetrics::new())),
			background_manager,
		})
	}

	/// Create a disabled adapter (no-op implementation for when database is not needed)
	pub fn disabled() -> Self {
		Self {
			storage: Arc::new(RwLock::new(Box::new(NoOpStorage))),
			config: DatabaseConfig::default(),
			enabled: false,
			maintenance_metrics: Arc::new(RwLock::new(BackgroundMaintenanceMetrics::new())),
			background_manager: None,
		}
	}

	/// Check if the database adapter is enabled
	pub fn is_enabled(&self) -> bool {
		self.enabled
	}

	/// Get the database file path (if any)
	pub fn database_path(&self) -> Option<&Path> {
		if self.enabled {
			Some(&self.config.database_path)
		} else {
			None
		}
	}
	/// Store a filesystem event
	pub async fn store_event(&self, event: &FileSystemEvent) -> DatabaseResult<()> {
		if !self.enabled {
			return Ok(());
		}

		let record = EventRecord::from_event_with_retention(event, self.config.event_retention)?;
		let mut storage = self.storage.write().await;
		storage.store_event(&record).await
	}

	/// Store metadata for a file or directory
	pub async fn store_metadata(
		&self, path: &Path, metadata: &std::fs::Metadata,
	) -> DatabaseResult<()> {
		if !self.enabled {
			return Ok(());
		}

		let record = MetadataRecord::from_metadata(path, metadata)?;
		let mut storage = self.storage.write().await;
		storage.store_metadata(&record).await
	}
	/// Get events for a specific path
	pub async fn get_events_for_path(&self, path: &Path) -> DatabaseResult<Vec<EventRecord>> {
		if !self.enabled {
			return Ok(Vec::new());
		}

		let key = StorageKey::path_hash(path);
		let mut storage = self.storage.write().await;
		storage.get_events(&key).await
	}

	/// Get metadata for a specific path
	pub async fn get_metadata(&self, path: &Path) -> DatabaseResult<Option<MetadataRecord>> {
		if !self.enabled {
			return Ok(None);
		}

		let mut storage = self.storage.write().await;
		storage.get_metadata(path).await
	}

	/// Find events in a time range (useful for correlating events)
	pub async fn find_events_by_time_range(
		&self, start: DateTime<Utc>, end: DateTime<Utc>,
	) -> DatabaseResult<Vec<EventRecord>> {
		if !self.enabled {
			return Ok(Vec::new());
		}

		let mut storage = self.storage.write().await;
		storage.find_events_by_time_range(start, end).await
	}
	/// Clean up old events based on the configured retention policy
	pub async fn cleanup_old_events(&self) -> DatabaseResult<usize> {
		if !self.enabled {
			return Ok(0);
		}

		let cutoff = SystemTime::now()
			.checked_sub(self.config.event_retention)
			.unwrap_or_else(SystemTime::now);

		let mut storage = self.storage.write().await;
		let count = storage.cleanup_expired_events(cutoff).await?;

		if count > 0 {
			info!("Cleaned up {} expired database records", count);
		}

		Ok(count)
	}

	/// Clean up old events using a configurable retention policy
	pub async fn cleanup_old_events_with_policy(
		&self, config: &crate::database::storage::event_retention::EventRetentionConfig,
	) -> DatabaseResult<usize> {
		if !self.enabled {
			return Ok(0);
		}
		let mut storage = self.storage.write().await;
		storage.cleanup_events_with_policy(config).await
	}

	/// Get database statistics
	pub async fn get_stats(&self) -> DatabaseResult<DatabaseStats> {
		if !self.enabled {
			return Ok(DatabaseStats::default());
		}

		let storage = self.storage.read().await;
		storage.get_stats().await
	}

	/// Compact the database to reclaim space
	pub async fn compact(&self) -> DatabaseResult<()> {
		if !self.enabled {
			return Ok(());
		}

		let mut storage = self.storage.write().await;
		storage.compact().await
	}

	/// Check database health and perform maintenance if needed
	pub async fn health_check(&self) -> DatabaseResult<bool> {
		if !self.enabled {
			return Ok(true);
		}

		match self.get_stats().await {
			Ok(stats) => {
				debug!(
					"Database health check: {} events, {} metadata records",
					stats.total_events, stats.total_metadata
				); // Check if we need to perform maintenance
				if stats.total_events > (self.config.memory_buffer_size as u64) * 10 {
					warn!(
						"Database has {} events, consider running cleanup",
						stats.total_events
					);
				}

				Ok(true)
			}
			Err(e) => {
				error!("Database health check failed: {}", e);
				Ok(false)
			}
		}
	}

	/// Get a RedbFilesystemCache if enabled, else None
	pub async fn get_filesystem_cache(&self) -> Option<RedbFilesystemCache> {
		if !self.enabled {
			return None;
		}
		let storage = self.storage.read().await;
		// Downcast to RedbStorage to access the database
		storage
			.as_any()
			.downcast_ref::<crate::database::storage::core::RedbStorage>()
			.map(|redb_storage| RedbFilesystemCache::new(redb_storage.database().clone()))
	}

	/// Expose the underlying Arc<Database> for advanced operations (test/maintenance only)
	pub async fn get_raw_database(&self) -> Option<Arc<redb::Database>> {
		let storage = self.storage.read().await;
		storage
			.as_any()
			.downcast_ref::<crate::database::storage::RedbStorage>()
			.map(|redb_storage| redb_storage.get_database())
	}

	/// Get a snapshot of background maintenance metrics
	pub async fn get_maintenance_metrics(&self) -> BackgroundMaintenanceMetrics {
		self.maintenance_metrics.read().await.clone()
	}

	/// Start the new background task manager with core maintenance tasks
	pub async fn start_background_manager(&self) {
		if !self.enabled {
			// Early exit: do nothing
			return;
		}
		if let Some(manager) = &self.background_manager {
			// Only start the manager, do not register tasks here
			let manager = manager.clone();
			tokio::spawn(async move {
				manager.start().await;
			});
		}
	}
}

/// No-op storage implementation for when database is disabled
struct NoOpStorage;

#[async_trait::async_trait]
impl DatabaseStorage for NoOpStorage {
	fn as_any(&self) -> &dyn std::any::Any {
		self
	}
	async fn initialize(&mut self) -> DatabaseResult<()> {
		Ok(())
	}

	async fn store_event(&mut self, _record: &EventRecord) -> DatabaseResult<()> {
		Ok(())
	}

	async fn get_events(&mut self, _key: &StorageKey) -> DatabaseResult<Vec<EventRecord>> {
		Ok(Vec::new())
	}

	async fn store_metadata(&mut self, _record: &MetadataRecord) -> DatabaseResult<()> {
		Ok(())
	}

	async fn get_metadata(&mut self, _path: &Path) -> DatabaseResult<Option<MetadataRecord>> {
		Ok(None)
	}

	async fn find_events_by_time_range(
		&mut self, _start: DateTime<Utc>, _end: DateTime<Utc>,
	) -> DatabaseResult<Vec<EventRecord>> {
		Ok(Vec::new())
	}

	async fn cleanup_expired_events(&mut self, _before: SystemTime) -> DatabaseResult<usize> {
		Ok(0)
	}

	async fn get_stats(&self) -> DatabaseResult<DatabaseStats> {
		Ok(DatabaseStats::default())
	}

	async fn compact(&mut self) -> DatabaseResult<()> {
		Ok(())
	}

	async fn close(self) -> DatabaseResult<()> {
		Ok(())
	}

	async fn store_filesystem_node(
		&mut self, _watch_id: &uuid::Uuid, _node: &crate::database::types::FilesystemNode,
		_event_type: &str,
	) -> crate::database::error::DatabaseResult<()> {
		Ok(())
	}

	async fn get_filesystem_node(
		&mut self, _watch_id: &uuid::Uuid, _path: &std::path::Path,
	) -> crate::database::error::DatabaseResult<Option<crate::database::types::FilesystemNode>> {
		Ok(None)
	}

	async fn get_node(
		&mut self, _watch_id: &uuid::Uuid, _path: &std::path::Path,
	) -> crate::database::error::DatabaseResult<Option<crate::database::types::FilesystemNode>> {
		Ok(None)
	}

	async fn list_directory_for_watch(
		&mut self, _watch_id: &uuid::Uuid, _parent_path: &std::path::Path,
	) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>> {
		Ok(vec![])
	}

	async fn batch_store_filesystem_nodes(
		&mut self, _watch_id: &uuid::Uuid, _nodes: &[crate::database::types::FilesystemNode],
		_event_type: &str,
	) -> crate::database::error::DatabaseResult<()> {
		Ok(())
	}

	async fn store_watch_metadata(
		&mut self, _metadata: &crate::database::types::WatchMetadata,
	) -> crate::database::error::DatabaseResult<()> {
		Ok(())
	}

	async fn get_watch_metadata(
		&mut self, _watch_id: &uuid::Uuid,
	) -> crate::database::error::DatabaseResult<Option<crate::database::types::WatchMetadata>> {
		Ok(None)
	}

	async fn cleanup_events_with_policy(
		&mut self, _config: &crate::database::storage::event_retention::EventRetentionConfig,
	) -> DatabaseResult<usize> {
		Ok(0)
	}

	async fn delete_events_older_than(
		&mut self, _cutoff: std::time::SystemTime,
	) -> DatabaseResult<usize> {
		Ok(0)
	}
	async fn count_events(&self) -> DatabaseResult<usize> {
		Ok(0)
	}
	async fn delete_oldest_events(&mut self, _n: usize) -> DatabaseResult<usize> {
		Ok(0)
	}
	async fn search_nodes(&mut self, _pattern: &str) -> DatabaseResult<Vec<FilesystemNode>> {
		Ok(Vec::new())
	}
}

/// Extension trait to convert events to database records
impl EventRecord {
	#[allow(clippy::result_large_err)]
	pub fn from_event(event: &FileSystemEvent) -> DatabaseResult<Self> {
		Self::from_event_with_retention(event, std::time::Duration::from_secs(24 * 60 * 60))
	}
	#[allow(clippy::result_large_err)]
	pub fn from_event_with_retention(
		event: &FileSystemEvent, retention: std::time::Duration,
	) -> DatabaseResult<Self> {
		use chrono::Duration;

		let expires_at = Utc::now()
			+ Duration::from_std(retention).map_err(|e| {
				DatabaseError::InitializationFailed(format!("Invalid retention duration: {e}"))
			})?;

		Ok(EventRecord {
			event_id: event.id, // Preserve the original event id for append-only and deduplication semantics
			event_type: format!("{:?}", event.event_type),
			path: event.path.clone(),
			timestamp: event.timestamp,
			is_directory: event.is_directory,
			size: event.size,
			inode: None,            // Could extract from metadata if needed
			windows_id: None,       // Platform-specific
			content_hash: None,     // Could be computed for files if needed
			confidence: None,       // Only set for move events
			detection_method: None, // Only set for move events
			expires_at,
			sequence_number: 0, // Placeholder; will be set transactionally by event storage logic. This avoids accidental reuse and enforces append-only semantics.
		})
	}
}

/// Extension trait to convert metadata to database records
impl MetadataRecord {
	#[allow(clippy::result_large_err)]
	pub fn from_metadata(path: &Path, metadata: &std::fs::Metadata) -> DatabaseResult<Self> {
		use chrono::{DateTime, Utc};

		let modified_at = metadata.modified().ok().map(DateTime::<Utc>::from);

		Ok(MetadataRecord {
			path: path.to_path_buf(),
			size: Some(metadata.len()),
			inode: None,        // Platform-specific - could extract on Unix
			windows_id: None,   // Platform-specific
			content_hash: None, // Could be computed for files if needed
			cached_at: Utc::now(),
			is_directory: metadata.is_dir(),
			modified_at,
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::events::{EventType, FileSystemEvent};
	use std::path::PathBuf;
	use tempfile::TempDir;
	use tokio;
	use uuid::Uuid;

	fn create_test_event(
		event_type: EventType, path: PathBuf, size: Option<u64>,
	) -> FileSystemEvent {
		FileSystemEvent {
			id: Uuid::new_v4(),
			event_type,
			path,
			timestamp: chrono::Utc::now(),
			is_directory: false,
			size,
			move_data: None,
		}
	}

	#[tokio::test]
	async fn test_adapter_creation_and_config() {
		let temp_dir = TempDir::new().unwrap();
		let db_path = temp_dir.path().join("test_adapter_creation_and_config.redb");

		let config = DatabaseConfig { database_path: db_path.clone(), ..Default::default() };

		let adapter = DatabaseAdapter::new(config).await.unwrap();
		assert!(adapter.is_enabled());
		assert_eq!(adapter.database_path(), Some(db_path.as_path()));

		let disabled = DatabaseAdapter::disabled();
		assert!(!disabled.is_enabled());
		assert_eq!(disabled.database_path(), None);
	}
	#[tokio::test]
	async fn test_event_storage_and_retrieval() {
		let temp_dir = TempDir::new().unwrap();
		let db_path = temp_dir.path().join("test_event_storage_and_retrieval.redb");

		let config = DatabaseConfig { database_path: db_path, ..Default::default() };

		let adapter = DatabaseAdapter::new(config).await.unwrap();
		// Use absolute path directly to avoid Windows canonicalize bugs
		let test_path = temp_dir.path().join("test.txt");

		// Store multiple events for the same path
		let mut events = Vec::new();
		for i in 0..3 {
			let event_path = test_path.clone();
			// File already created above for canonicalization
			let event = create_test_event(EventType::Create, event_path, Some(1024 + i));
			adapter.store_event(&event).await.unwrap();
			events.push(event);
		}

		// Retrieve events
		let mut retrieved = adapter.get_events_for_path(&test_path).await.unwrap();
		// Sort both vectors by size for stable comparison
		let mut expected = events.clone();
		retrieved.sort_by_key(|e| e.size);
		expected.sort_by_key(|e| e.size);
		assert_eq!(retrieved.len(), expected.len());
		for (expected, actual) in expected.iter().zip(retrieved.iter()) {
			assert_eq!(
				format!("{:?}", expected.event_type),
				actual.event_type,
				"Event type mismatch"
			);
			assert_eq!(expected.path, actual.path, "Path mismatch");
			assert_eq!(expected.size, actual.size, "Size mismatch");
		}
	}

	#[tokio::test]
	async fn test_metadata_storage_and_retrieval() {
		let temp_dir = TempDir::new().unwrap();
		let db_path = temp_dir.path().join("test_metadata_storage_and_retrieval.redb");
		let test_file = temp_dir.path().join("test.txt");
		std::fs::write(&test_file, "test").unwrap();
		// Sleep to avoid race conditions on some filesystems
		std::thread::sleep(std::time::Duration::from_millis(50));
		let test_file = test_file.canonicalize().unwrap();

		let config = DatabaseConfig { database_path: db_path, ..Default::default() };

		let adapter = DatabaseAdapter::new(config).await.unwrap();
		let metadata = std::fs::metadata(&test_file).unwrap();

		// Store metadata
		adapter.store_metadata(&test_file, &metadata).await.unwrap();
		// Retrieve metadata
		let retrieved = adapter.get_metadata(&test_file).await.unwrap();
		assert!(retrieved.is_some());
		let retrieved = retrieved.unwrap();
		assert_eq!(retrieved.path, test_file);
		assert_eq!(retrieved.size, Some(metadata.len()));
	}

	#[tokio::test]
	async fn test_query_operations() {
		let temp_dir = TempDir::new().unwrap();
		let db_path = temp_dir.path().join("test_query_operations.redb");

		let config = DatabaseConfig { database_path: db_path, ..Default::default() };

		let adapter = DatabaseAdapter::new(config).await.unwrap();
		// Store events with different sizes for different paths
		let mut all_events = Vec::new();
		let now = Utc::now();
		for i in 0..5 {
			let path = temp_dir.path().join(format!("file_{i}.txt"));
			std::fs::write(&path, format!("test-{i}")).unwrap();
			let path = path.canonicalize().unwrap();
			let mut event =
				create_test_event(EventType::Create, path.clone(), Some(1000 + i * 100));
			// Set event timestamp to 'now' so it is always in the query range
			event.timestamp = now;
			adapter.store_event(&event).await.unwrap();
			all_events.push((path, event));
		}

		// Query by time range
		let hour_ago = now - chrono::Duration::hours(1);
		let recent_events = adapter.find_events_by_time_range(hour_ago, now).await.unwrap();
		assert!(
			recent_events.len() >= all_events.len(),
			"Should retrieve at least as many events as stored"
		);

		// For each path, verify all events are present
		for (path, expected_event) in &all_events {
			let retrieved = adapter.get_events_for_path(path).await.unwrap();
			assert!(!retrieved.is_empty(), "No events found for path");
			let found = retrieved
				.iter()
				.any(|ev| ev.path == expected_event.path && ev.size == expected_event.size);
			assert!(
				found,
				"Expected event not found for path {}",
				path.display()
			);
		}
	}
	#[tokio::test]
	async fn test_disabled_adapter_operations() {
		let adapter = DatabaseAdapter::disabled();
		let test_path = PathBuf::from("/test/path");

		let event = create_test_event(EventType::Create, test_path.clone(), Some(1024));

		// All operations should succeed but do nothing
		adapter.store_event(&event).await.unwrap();

		let events = adapter.get_events_for_path(&test_path).await.unwrap();
		assert!(events.is_empty());

		let metadata = adapter.get_metadata(&test_path).await.unwrap();
		assert!(metadata.is_none());

		let stats = adapter.get_stats().await.unwrap();
		assert_eq!(stats.total_events, 0);
		assert_eq!(stats.total_metadata, 0);

		let cleaned = adapter.cleanup_old_events().await.unwrap();
		assert_eq!(cleaned, 0);
	}

	#[tokio::test]
	async fn test_health_check_and_maintenance() {
		let temp_dir = TempDir::new().unwrap();
		let db_path = temp_dir.path().join("health.redb");

		let config = DatabaseConfig { database_path: db_path, ..Default::default() };

		let adapter = DatabaseAdapter::new(config).await.unwrap();

		// Health check on empty database
		let health = adapter.health_check().await.unwrap();
		assert!(health);
		// Add some data
		for i in 0..10 {
			let path = temp_dir.path().join(format!("file_{i}.txt"));
			let event = create_test_event(EventType::Create, path, Some(1024));
			adapter.store_event(&event).await.unwrap();
		}

		// Health check with data
		let health = adapter.health_check().await.unwrap();
		assert!(health);

		// Test compaction
		adapter.compact().await.unwrap();

		// Test cleanup (should clean nothing since events are recent)
		let cleaned = adapter.cleanup_old_events().await.unwrap();
		assert_eq!(cleaned, 0);
	}
}
