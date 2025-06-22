//! Background task manager for scalable, robust maintenance and health operations.
//!
//! This module provides a centralized, extensible framework for running background
//! maintenance tasks (e.g., index repair, compaction, health checks) with isolation,
//! adaptive scheduling, and observability.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Default)]
pub struct TaskMetrics {
	pub last_run: Option<Instant>,
	pub last_error: Option<String>,
	pub last_duration: Option<Duration>,
	pub success_count: u64,
	pub failure_count: u64,
}

pub trait BackgroundTask: Send + Sync {
	fn name(&self) -> &'static str;
	fn min_interval(&self) -> Duration;
	fn max_interval(&self) -> Duration;
	fn should_run(&self, _last_run: Option<Instant>, _last_error: Option<&str>) -> bool {
		// Default: always run on schedule
		true
	}
	fn run(&self) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send>>;
}

pub struct BackgroundTaskManager {
	tasks: Vec<Arc<dyn BackgroundTask>>,
	metrics: Arc<RwLock<HashMap<String, TaskMetrics>>>,
}

impl BackgroundTaskManager {
	pub fn new() -> Self {
		Self { tasks: Vec::new(), metrics: Arc::new(RwLock::new(HashMap::new())) }
	}

	pub fn register_task(&mut self, task: Arc<dyn BackgroundTask>) {
		self.tasks.push(task);
	}

	pub async fn start(self: Arc<Self>) {
		for task in &self.tasks {
			let task = task.clone();
			let metrics = self.metrics.clone();
			tokio::spawn(async move {
				loop {
					let start = Instant::now();
					let name = task.name();
					let mut metrics_guard = metrics.write().await;
					let _ = metrics_guard.entry(name.to_string()).or_default(); // Only for side effect
					let result = task.run().await;
					let mut metrics_guard = metrics.write().await;
					let entry = metrics_guard.entry(name.to_string()).or_default();
					entry.last_run = Some(Instant::now());
					entry.last_duration = Some(start.elapsed());
					match &result {
						Ok(_) => entry.success_count += 1,
						Err(e) => {
							entry.failure_count += 1;
							entry.last_error = Some(format!("{e:?}"));
						}
					}
					// TODO: Adaptive scheduling, backoff, jitter
					tokio::time::sleep(task.min_interval()).await;
				}
			});
		}
	}

	pub async fn trigger(&self, _task_name: &str) {
		// TODO: Implement on-demand trigger for a specific task
	}

	pub async fn get_metrics(&self) -> HashMap<String, TaskMetrics> {
		self.metrics.read().await.clone()
	}
}

impl Default for BackgroundTaskManager {
	fn default() -> Self {
		Self::new()
	}
}

// Example: TimeIndexRepairTask and CompactionTask would implement BackgroundTask
// and be registered with the manager at startup.
//
// TODO: Implement concrete tasks and adaptive scheduling.
