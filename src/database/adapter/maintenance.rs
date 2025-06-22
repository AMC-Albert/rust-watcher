//! Maintenance metrics and related logic for DatabaseAdapter background tasks.
//!
//! This module defines metrics for tracking background maintenance task performance and results.

use chrono::{DateTime, Utc};

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
