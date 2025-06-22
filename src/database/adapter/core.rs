//! Core database adapter implementation (migrated from adapter.rs)
//!
//! This module contains the core implementation of the database adapter,
//! including functions for connecting to the database, executing queries,
//! and managing transactions.

use crate::database::storage::filesystem_cache::RedbFilesystemCache;
use crate::database::types::FilesystemNode;
use crate::database::{
	config::DatabaseConfig,
	error::DatabaseResult,
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

use super::background::setup_background_manager;
use super::maintenance::BackgroundMaintenanceMetrics;

#[derive(Clone)]
pub struct DatabaseAdapter {
	storage: Arc<RwLock<Box<dyn DatabaseStorage>>>,
	config: DatabaseConfig,
	enabled: bool,
	maintenance_metrics: Arc<RwLock<BackgroundMaintenanceMetrics>>,
	#[allow(dead_code)]
	background_manager: Option<Arc<crate::database::background_tasks::BackgroundTaskManager>>,
}

impl DatabaseAdapter {
	/// Create a new database adapter with the given configuration
	pub async fn new(config: DatabaseConfig) -> DatabaseResult<Self> {
		let storage: Box<dyn DatabaseStorage> = Box::new(RedbStorage::new(config.clone()).await?);
		let enabled = true;
		let background_manager = setup_background_manager(storage.as_ref());
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

	pub fn is_enabled(&self) -> bool {
		self.enabled
	}

	pub fn database_path(&self) -> Option<&Path> {
		if self.enabled {
			Some(&self.config.database_path)
		} else {
			None
		}
	}

	pub async fn store_event(&self, event: &FileSystemEvent) -> DatabaseResult<()> {
		if !self.enabled {
			return Ok(());
		}
		// TODO: This is a workaround for missing EventRecord::from_event_with_retention. Use EventRecord::new instead.
		let record = EventRecord::new(
			format!("{:?}", event.event_type),
			event.path.clone(),
			event.is_directory,
			chrono::Duration::from_std(self.config.event_retention)
				.unwrap_or_else(|_| chrono::Duration::seconds(86400)),
			0, // sequence_number placeholder
		);
		let mut storage = self.storage.write().await;
		storage.store_event(&record).await
	}

	pub async fn store_metadata(
		&self, path: &Path, metadata: &std::fs::Metadata,
	) -> DatabaseResult<()> {
		if !self.enabled {
			return Ok(());
		}
		// TODO: This is a workaround for missing MetadataRecord::from_metadata. Use MetadataRecord::new instead.
		let record = MetadataRecord::new(path.to_path_buf(), metadata.is_dir());
		let mut storage = self.storage.write().await;
		storage.store_metadata(&record).await
	}

	pub async fn get_events_for_path(&self, path: &Path) -> DatabaseResult<Vec<EventRecord>> {
		if !self.enabled {
			return Ok(Vec::new());
		}
		let key = StorageKey::path_hash(path);
		let mut storage = self.storage.write().await;
		storage.get_events(&key).await
	}

	pub async fn get_metadata(&self, path: &Path) -> DatabaseResult<Option<MetadataRecord>> {
		if !self.enabled {
			return Ok(None);
		}
		let mut storage = self.storage.write().await;
		storage.get_metadata(path).await
	}

	pub async fn find_events_by_time_range(
		&self, start: DateTime<Utc>, end: DateTime<Utc>,
	) -> DatabaseResult<Vec<EventRecord>> {
		if !self.enabled {
			return Ok(Vec::new());
		}
		let mut storage = self.storage.write().await;
		storage.find_events_by_time_range(start, end).await
	}

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

	pub async fn cleanup_old_events_with_policy(
		&self, config: &crate::database::storage::event_retention::EventRetentionConfig,
	) -> DatabaseResult<usize> {
		if !self.enabled {
			return Ok(0);
		}
		let mut storage = self.storage.write().await;
		storage.cleanup_events_with_policy(config).await
	}

	pub async fn get_stats(&self) -> DatabaseResult<DatabaseStats> {
		if !self.enabled {
			return Ok(DatabaseStats::default());
		}
		let storage = self.storage.read().await;
		storage.get_stats().await
	}

	pub async fn compact(&self) -> DatabaseResult<()> {
		if !self.enabled {
			return Ok(());
		}
		let mut storage = self.storage.write().await;
		storage.compact().await
	}

	pub async fn health_check(&self) -> DatabaseResult<bool> {
		if !self.enabled {
			return Ok(true);
		}
		match self.get_stats().await {
			Ok(stats) => {
				debug!(
					"Database health check: {} events, {} metadata records",
					stats.total_events, stats.total_metadata
				);
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

	pub async fn get_filesystem_cache(&self) -> Option<RedbFilesystemCache> {
		if !self.enabled {
			return None;
		}
		let storage = self.storage.read().await;
		storage
			.as_any()
			.downcast_ref::<crate::database::storage::core::RedbStorage>()
			.map(|redb_storage| RedbFilesystemCache::new(redb_storage.database().clone()))
	}

	pub async fn get_raw_database(&self) -> Option<Arc<redb::Database>> {
		let storage = self.storage.read().await;
		storage
			.as_any()
			.downcast_ref::<crate::database::storage::RedbStorage>()
			.map(|redb_storage| redb_storage.get_database())
	}

	pub async fn get_maintenance_metrics(&self) -> BackgroundMaintenanceMetrics {
		self.maintenance_metrics.read().await.clone()
	}

	pub async fn start_background_manager(&self) {
		if !self.enabled {
			return;
		}
		if let Some(manager) = &self.background_manager {
			let manager = manager.clone();
			tokio::spawn(async move {
				manager.start().await;
			});
		}
	}
}

struct NoOpStorage;

#[async_trait::async_trait]
impl DatabaseStorage for NoOpStorage {
	fn as_any(&self) -> &dyn std::any::Any {
		self
	}
	async fn initialize(&mut self) -> DatabaseResult<()> {
		Ok(())
	}
	async fn store_event(&mut self, _event: &EventRecord) -> DatabaseResult<()> {
		Ok(())
	}
	async fn store_metadata(&mut self, _metadata: &MetadataRecord) -> DatabaseResult<()> {
		Ok(())
	}
	async fn get_events(&mut self, _key: &StorageKey) -> DatabaseResult<Vec<EventRecord>> {
		Ok(Vec::new())
	}
	async fn get_metadata(
		&mut self, _path: &std::path::Path,
	) -> DatabaseResult<Option<MetadataRecord>> {
		Ok(None)
	}
	async fn find_events_by_time_range(
		&mut self, _start: DateTime<Utc>, _end: DateTime<Utc>,
	) -> DatabaseResult<Vec<EventRecord>> {
		Ok(Vec::new())
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
	async fn cleanup_expired_events(&mut self, _cutoff: SystemTime) -> DatabaseResult<usize> {
		Ok(0)
	}
	async fn cleanup_events_with_policy(
		&mut self, _config: &crate::database::storage::event_retention::EventRetentionConfig,
	) -> DatabaseResult<usize> {
		Ok(0)
	}
	async fn get_stats(&self) -> DatabaseResult<DatabaseStats> {
		Ok(DatabaseStats::default())
	}
	async fn compact(&mut self) -> DatabaseResult<()> {
		Ok(())
	}
}
