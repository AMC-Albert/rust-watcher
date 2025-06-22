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

/// Background task for health checking the database.
pub struct HealthCheckTask {
	pub db: Arc<Database>,
}

impl BackgroundTask for HealthCheckTask {
	fn name(&self) -> &'static str {
		"health_check"
	}
	fn min_interval(&self) -> Duration {
		Duration::from_secs(300) // 5 minutes
	}
	fn max_interval(&self) -> Duration {
		Duration::from_secs(1800) // 30 minutes
	}
	fn run(&self) -> Pin<Box<dyn std::future::Future<Output = Result<(), Error>> + Send>> {
		let db = self.db.clone();
		Box::pin(async move {
			let _ok = maintenance::health_check(&db).await?;
			Ok(())
		})
	}
}

/// Background task for refreshing database stats.
pub struct StatsRefreshTask {
	pub db: Arc<Database>,
}

impl BackgroundTask for StatsRefreshTask {
	fn name(&self) -> &'static str {
		"stats_refresh"
	}
	fn min_interval(&self) -> Duration {
		Duration::from_secs(600) // 10 minutes
	}
	fn max_interval(&self) -> Duration {
		Duration::from_secs(3600) // 1 hour
	}
	fn run(&self) -> Pin<Box<dyn std::future::Future<Output = Result<(), Error>> + Send>> {
		let db = self.db.clone();
		Box::pin(async move {
			let _stats = maintenance::get_database_stats(&db).await?;
			Ok(())
		})
	}
}

// TODO: Add more tasks as needed.
