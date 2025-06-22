//! Maintenance and statistics operations
//!
//! This module handles database maintenance, statistics collection,
//! and health monitoring operations.

use crate::database::{config::DatabaseConfig, error::DatabaseResult, types::DatabaseStats};
use redb::{Database, ReadableMultimapTable, ReadableTable};
use std::sync::Arc;

/// Trait for maintenance and statistics operations
#[async_trait::async_trait]
pub trait MaintenanceStorage: Send + Sync {
	/// Get comprehensive database statistics
	async fn get_comprehensive_stats(&self) -> DatabaseResult<DatabaseStats>;

	/// Compact the database to reclaim space
	async fn compact_database(&mut self) -> DatabaseResult<()>;

	/// Perform routine maintenance operations
	async fn perform_maintenance(&mut self) -> DatabaseResult<()>;

	/// Check database health
	async fn health_check(&self) -> DatabaseResult<bool>;
}

/// Implementation of maintenance storage using ReDB
#[allow(dead_code)]
pub struct MaintenanceImpl {
	database: Arc<Database>,
	config: DatabaseConfig,
}

impl MaintenanceImpl {
	pub fn new(database: Arc<Database>, config: DatabaseConfig) -> Self {
		Self { database, config }
	}

	/// Initialize maintenance tables
	pub async fn initialize(&mut self, _database: &Arc<Database>) -> DatabaseResult<()> {
		let write_txn = self.database.begin_write()?;
		{
			// Create maintenance tables if they don't exist
			let _stats_table = write_txn.open_table(super::tables::STATS_TABLE)?;
			let _maintenance_log = write_txn.open_table(super::tables::MAINTENANCE_LOG)?;
		}
		write_txn.commit()?;
		Ok(())
	}
}

#[async_trait::async_trait]
impl MaintenanceStorage for MaintenanceImpl {
	async fn get_comprehensive_stats(&self) -> DatabaseResult<DatabaseStats> {
		let read_txn = self.database.begin_read()?;

		// Count records in basic tables
		let _events_table = read_txn.open_table(super::tables::EVENTS_TABLE)?;
		let _metadata_table = read_txn.open_table(super::tables::METADATA_TABLE)?;

		// Note: ReDB doesn't have a direct len() method, so we'll estimate or iterate
		// For now, return basic stats structure

		Ok(DatabaseStats {
			total_events: 0,   // Would need to iterate to count
			total_metadata: 0, // Would need to iterate to count
			database_size: 0,  // Would need ReDB-specific API
			read_operations: 0,
			write_operations: 0,
			delete_operations: 0,
			cache_hit_rate: 0.0,
			avg_query_time_ms: 0.0,
			cleaned_up_events: 0,
			per_type_counts: std::collections::HashMap::new(),
		})
	}

	async fn compact_database(&mut self) -> DatabaseResult<()> {
		// ReDB handles compaction automatically
		// Could implement manual compaction triggers here if needed
		Ok(())
	}

	async fn perform_maintenance(&mut self) -> DatabaseResult<()> {
		use std::time::{Duration, SystemTime};
		use tracing::info;
		// Cleanup expired records (default retention: 30 days)
		let retention = Duration::from_secs(30 * 24 * 60 * 60);
		let before = SystemTime::now() - retention;
		let cleaned = cleanup_expired_events(&self.database, before).await?;
		info!("Maintenance: cleaned up {} expired events", cleaned);
		// Update statistics (repair counters if needed)
		let _ = crate::database::storage::maintenance::get_database_stats(&self.database).await;
		// Health check
		let _ = self.health_check().await;
		// Log maintenance activity (could be extended to DB log)
		info!("Maintenance: completed");
		Ok(())
	}

	async fn health_check(&self) -> DatabaseResult<bool> {
		// TODO: Implement comprehensive health checks
		// - Database connectivity
		// - Table integrity
		// - Performance metrics
		// - Resource usage
		Ok(true)
	}
}

/// Clean up expired events using the provided database
pub async fn cleanup_expired_events(
	database: &Arc<Database>, before: std::time::SystemTime,
) -> DatabaseResult<usize> {
	use crate::database::types::EventRecord;
	use chrono::{DateTime, Utc};

	let mut removed = 0usize;
	let write_txn = database.begin_write()?;
	{
		let mut events_log =
			write_txn.open_multimap_table(crate::database::storage::tables::EVENTS_LOG_TABLE)?;
		let mut time_index =
			write_txn.open_multimap_table(crate::database::storage::tables::TIME_INDEX_TABLE)?;
		let bucket_size_seconds = 3600; // Must match store_event
		let before_dt: DateTime<Utc> = before.into();
		let before_bucket = crate::database::types::StorageKey::TimeBucket(
			before_dt.timestamp() / bucket_size_seconds * bucket_size_seconds,
		);
		let mut to_remove = Vec::new();
		// Iterate all time buckets up to and including before_bucket
		for bucket in time_index.iter()? {
			let (bucket_guard, multimap_value) = bucket?;
			let bucket_bytes = bucket_guard.value();
			let bucket_key = bucket_bytes;
			// Only process buckets <= before_bucket
			if bucket_bytes <= &before_bucket.to_bytes()[..] {
				for value_guard in multimap_value.flatten() {
					let value = value_guard.value();
					if let Ok(event) = bincode::deserialize::<EventRecord>(value) {
						if event.expires_at < before_dt {
							// Remove from both time index and event log
							let path_hash_key =
								crate::database::types::StorageKey::path_hash(&event.path)
									.to_bytes();
							to_remove.push((bucket_key.to_vec(), path_hash_key, value.to_vec()));
						}
					}
				}
			}
		}
		for (bucket_key, path_hash_key, value) in to_remove {
			if time_index.remove(bucket_key.as_slice(), value.as_slice())? {
				// Remove from event log as well
				let _ = events_log.remove(path_hash_key.as_slice(), value.as_slice());
				removed += 1;
			}
		}
	}
	write_txn.commit()?;
	Ok(removed)
}

/// Get database statistics using the provided database
///
/// LIMITATIONS & TODOs (read before using in production):
/// - This implementation is O(N) over all events. For large event logs (millions of events),
///   this will cause significant latency (seconds to minutes depending on hardware and DB size).
/// - Only the event count is accurate. All other fields in DatabaseStats are placeholders and
///   should not be relied upon for monitoring or alerting.
/// - No stats for metadata, file size, or performance metrics. These require additional
///   bookkeeping or Redb API support.
/// - No caching or incremental update; every call scans the entire log. This will impact
///   performance if called frequently (e.g., in a dashboard or health check loop).
/// - If production workloads require frequent stats, implement an indexed or cached approach.
///   For example, maintain a counter in a separate table updated on every event insert/delete.
/// - Database compaction and cleanup are not implemented. Stale data will accumulate unless
///   explicit maintenance is performed in future versions.
/// - This is a stopgap for correctness, not a scalable solution. Do not use as-is for real-time
///   monitoring or in high-throughput environments.
///
/// Example: On a database with 10 million events, this function may take 10-30 seconds to return
/// depending on disk speed and system load. For anything beyond basic debugging, redesign is required.
///
/// Edge case: If the event log is corrupted or partially written, the count may be inaccurate or
/// the function may panic. No recovery or validation is performed here.
///
/// TODO: Replace with a scalable, indexed, and robust stats subsystem. See README and design docs.
pub async fn get_database_stats(database: &Arc<Database>) -> DatabaseResult<DatabaseStats> {
	// Use persistent event and metadata counters for O(1) stats queries. Repair if missing or out-of-sync.
	let read_txn = database.begin_read()?;
	let stats_table = read_txn.open_table(crate::database::storage::tables::STATS_TABLE)?;
	let count_bytes = stats_table.get(crate::database::storage::tables::EVENT_COUNT_KEY)?;
	let mut total_events = count_bytes
		.map(|v| u64::from_le_bytes(v.value().try_into().unwrap_or([0u8; 8])))
		.unwrap_or(u64::MAX); // Use u64::MAX as a sentinel for missing/corrupt

	if total_events == u64::MAX {
		// Counter missing/corrupt: rescan and repair
		if let Ok(events_log) =
			read_txn.open_multimap_table(crate::database::storage::tables::EVENTS_LOG_TABLE)
		{
			let mut count = 0u64;
			if let Ok(iter) = events_log.iter() {
				for (_key_guard, multimap_value) in iter.flatten() {
					for value_result in multimap_value {
						if value_result.is_ok() {
							count += 1;
						}
					}
				}
			}
			// Write repaired counter
			let write_txn = database.begin_write()?;
			let mut stats_table =
				write_txn.open_table(crate::database::storage::tables::STATS_TABLE)?;
			stats_table.insert(
				crate::database::storage::tables::EVENT_COUNT_KEY,
				&count.to_le_bytes()[..],
			)?;
			drop(stats_table);
			write_txn.commit()?;
			total_events = count;
		} else {
			total_events = 0;
		}
	}

	// Persistent metadata counter
	let metadata_count_bytes =
		stats_table.get(crate::database::storage::tables::METADATA_COUNT_KEY)?;
	let mut total_metadata = metadata_count_bytes
		.map(|v| u64::from_le_bytes(v.value().try_into().unwrap_or([0u8; 8])))
		.unwrap_or(u64::MAX); // Use u64::MAX as a sentinel for missing/corrupt

	if total_metadata == u64::MAX {
		// Counter missing/corrupt: rescan and repair
		if let Ok(metadata_table) =
			read_txn.open_table(crate::database::storage::tables::METADATA_TABLE)
		{
			let mut count = 0u64;
			let iter = metadata_table.iter();
			if let Ok(iter) = iter {
				for item in iter.flatten() {
					let (_key, _value) = item;
					count += 1;
				}
			}
			// Write repaired counter
			let write_txn = database.begin_write()?;
			let mut stats_table =
				write_txn.open_table(crate::database::storage::tables::STATS_TABLE)?;
			stats_table.insert(
				crate::database::storage::tables::METADATA_COUNT_KEY,
				&count.to_le_bytes()[..],
			)?;
			drop(stats_table);
			write_txn.commit()?;
			total_metadata = count;
		} else {
			total_metadata = 0;
		}
	}

	// Collect per-event-type stats
	let mut per_type_counts = std::collections::HashMap::new();
	for entry in stats_table.iter()? {
		let (key, value) = entry?;
		if let Ok(key_str) = std::str::from_utf8(key.value()) {
			if let Some(event_type) = key_str.strip_prefix("event_type:") {
				let count = u64::from_le_bytes(value.value().try_into().unwrap_or([0u8; 8]));
				per_type_counts.insert(event_type.to_string(), count);
			}
		}
	}

	Ok(crate::database::types::DatabaseStats {
		total_events,
		total_metadata,
		database_size: 0, // Not implemented
		read_operations: 0,
		write_operations: 0,
		delete_operations: 0,
		cache_hit_rate: 0.0,
		avg_query_time_ms: 0.0,
		cleaned_up_events: 0,
		per_type_counts,
	})
}

/// Compact database using the provided database
pub async fn compact_database(_database: &Arc<Database>) -> DatabaseResult<()> {
	// TODO: Implement database compaction
	// For now, return success - this would be implemented properly in Phase 1.2
	Ok(())
}

/// Repair the time index by scanning the event log and rebuilding all time buckets
pub async fn repair_time_index(database: &Arc<Database>) -> DatabaseResult<()> {
	use crate::database::types::EventRecord;
	let write_txn = database.begin_write()?;
	{
		let mut time_index =
			write_txn.open_multimap_table(crate::database::storage::tables::TIME_INDEX_TABLE)?;
		// Remove all entries from the time index manually
		let mut to_remove = Vec::new();
		for entry in time_index.iter()? {
			let (bucket_guard, multimap_value) = entry?;
			let bucket_key = bucket_guard.value().to_vec();
			for value_guard in multimap_value.flatten() {
				let value = value_guard.value().to_vec();
				to_remove.push((bucket_key.clone(), value));
			}
		}
		for (bucket_key, value) in to_remove {
			time_index.remove(bucket_key.as_slice(), value.as_slice())?;
		}
		let events_log =
			write_txn.open_multimap_table(crate::database::storage::tables::EVENTS_LOG_TABLE)?;
		let bucket_size_seconds = 3600;
		for entry in events_log.iter()? {
			let (_key_guard, multimap_value) = entry?;
			for value_guard in multimap_value.flatten() {
				let value = value_guard.value();
				if let Ok(event) = bincode::deserialize::<EventRecord>(value) {
					let time_bucket = crate::database::types::StorageKey::time_bucket(
						event.timestamp,
						bucket_size_seconds,
					);
					time_index.insert(time_bucket.to_bytes().as_slice(), value)?;
				}
			}
		}
	}
	write_txn.commit()?;
	Ok(())
}
