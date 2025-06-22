//! Concrete background task implementations for maintenance and health.
//!
//! This module provides BackgroundTask implementations for time index repair,
//! database compaction, and other core maintenance operations.

use crate::database::background_tasks::BackgroundTask;
use crate::database::storage::maintenance;
use anyhow::Error;
use redb::Database;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

/// Background task for repairing the time index.
pub struct TimeIndexRepairTask {
	pub db: Arc<Database>,
}

impl BackgroundTask for TimeIndexRepairTask {
	fn name(&self) -> &'static str {
		"time_index_repair"
	}
	fn min_interval(&self) -> Duration {
		Duration::from_secs(600) // 10 minutes
	}
	fn max_interval(&self) -> Duration {
		Duration::from_secs(3600) // 1 hour
	}
	fn run(&self) -> Pin<Box<dyn std::future::Future<Output = Result<(), Error>> + Send>> {
		let db = self.db.clone();
		Box::pin(async move { maintenance::repair_time_index(&db).await.map_err(Error::from) })
	}
}

/// Background task for compacting the database.
pub struct CompactionTask {
	pub db: Arc<Database>,
}

impl BackgroundTask for CompactionTask {
	fn name(&self) -> &'static str {
		"compaction"
	}
	fn min_interval(&self) -> Duration {
		Duration::from_secs(1800) // 30 minutes
	}
	fn max_interval(&self) -> Duration {
		Duration::from_secs(7200) // 2 hours
	}
	fn run(&self) -> Pin<Box<dyn std::future::Future<Output = Result<(), Error>> + Send>> {
		let db = self.db.clone();
		Box::pin(async move { maintenance::compact_database(&db).await.map_err(Error::from) })
	}
}

// TODO: Add more tasks (e.g., health check, stats refresh) as needed.
