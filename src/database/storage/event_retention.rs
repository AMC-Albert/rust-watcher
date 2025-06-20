// Implements retention and cleanup logic for the event log.
// This module is designed for large-scale, append-only event logs where old events must be pruned efficiently.
//
// Limitations:
// - Cleanup is best-effort; failures may leave stale data until the next run.
// - Performance depends on underlying database and event volume.
// - Retention policy is configurable but not dynamic at runtime (requires reconfiguration).
// - No transactional guarantee: concurrent inserts/deletes may cause temporary inconsistencies.
// - Edge cases: duplicate events are not deduplicated; ordering is by insertion timestamp only.
//
// Exposes both explicit cleanup API and optional background task integration.

use crate::database::storage::core::DatabaseStorage;
use std::time::{Duration, SystemTime}; // Use the correct trait

/// Retention policy configuration for event cleanup.
pub struct EventRetentionConfig {
	/// Retain events newer than this duration (relative to now).
	pub max_event_age: Duration,
	/// Maximum number of events to retain (optional, None = unlimited).
	pub max_events: Option<usize>,
	/// If true, cleanup runs periodically in the background.
	pub background: bool,
	/// Interval for background cleanup (if enabled).
	pub background_interval: Option<Duration>,
}

impl Default for EventRetentionConfig {
	fn default() -> Self {
		Self {
			max_event_age: Duration::from_secs(60 * 60 * 24 * 30), // 30 days
			max_events: None,
			background: false,
			background_interval: None,
		}
	}
}

/// Performs cleanup of old events according to the provided retention config.
pub async fn cleanup_old_events<S: DatabaseStorage>(
	storage: &mut S,
	config: &EventRetentionConfig,
) -> crate::database::error::DatabaseResult<usize> {
	// Remove events older than max_event_age.
	let cutoff = SystemTime::now()
		.checked_sub(config.max_event_age)
		.unwrap_or(SystemTime::UNIX_EPOCH);
	let mut removed = storage.delete_events_older_than(cutoff).await?;
	// Optionally enforce max_events limit (remove oldest if over limit).
	if let Some(max) = config.max_events {
		let total = storage.count_events().await?;
		if total > max {
			let to_remove = total - max;
			removed += storage.delete_oldest_events(to_remove).await?;
		}
	}
	Ok(removed)
}

// Edge cases:
// - If storage backend does not support efficient range deletes, cleanup may be slow.
// - If background task panics or fails, events may accumulate until next run.
// - No transactional guarantee: concurrent inserts/deletes may cause temporary inconsistencies.
// - Duplicate events are not deduplicated; ordering is by insertion timestamp only.
