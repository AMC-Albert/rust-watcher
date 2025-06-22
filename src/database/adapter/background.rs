//! Background task manager setup and registration for DatabaseAdapter.
//!
//! This module handles the initialization and registration of background tasks
//! such as compaction, health checks, stats refresh, and time index repair.

use crate::database::background_tasks::{
	BackgroundTaskManager, CompactionTask, HealthCheckTask, StatsRefreshTask, TimeIndexRepairTask,
};
use crate::database::storage::RedbStorage;
use std::sync::Arc;

pub fn setup_background_manager(
	storage: &dyn crate::database::storage::DatabaseStorage,
) -> Option<Arc<BackgroundTaskManager>> {
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
		let mut manager = BackgroundTaskManager::new();
		manager.register_task(repair);
		manager.register_task(compact);
		manager.register_task(health);
		manager.register_task(stats);
		Some(Arc::new(manager))
	} else {
		None
	}
}
