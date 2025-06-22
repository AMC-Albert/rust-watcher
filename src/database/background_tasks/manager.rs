//! Background task manager for scalable, robust maintenance and health operations.
//!
//! This module provides a centralized, extensible framework for running background
//! maintenance tasks (e.g., index repair, compaction, health checks) with isolation,
//! adaptive scheduling, and observability.

use rand::{rng, Rng};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};

#[derive(Debug, Clone, Default)]
pub struct TaskMetrics {
	pub last_run: Option<Instant>,
	pub last_error: Option<String>,
	pub last_duration: Option<Duration>,
	pub success_count: u64,
	pub failure_count: u64,
	pub last_success: Option<Instant>,
	pub last_failure: Option<Instant>,
	pub last_result: Option<String>, // e.g., health status, stats summary
	pub avg_duration_ms: Option<f64>,
	pub consecutive_failures: u32,
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
	trigger_senders: HashMap<String, mpsc::Sender<()>>,
}

impl BackgroundTaskManager {
	pub fn new() -> Self {
		Self {
			tasks: Vec::new(),
			metrics: Arc::new(RwLock::new(HashMap::new())),
			trigger_senders: HashMap::new(),
		}
	}

	pub fn register_task(&mut self, task: Arc<dyn BackgroundTask>) {
		let (tx, mut rx) = mpsc::channel::<()>(2);
		let name = task.name().to_string();
		self.trigger_senders.insert(name.clone(), tx);
		self.tasks.push(task.clone());
		let metrics = self.metrics.clone();
		tokio::spawn(async move {
			let mut backoff = 0u32;
			loop {
				let start = Instant::now();
				let mut metrics_guard = metrics.write().await;
				let _ = metrics_guard.entry(name.clone()).or_default();
				drop(metrics_guard);
				let result = task.run().await;
				let mut metrics_guard = metrics.write().await;
				let entry = metrics_guard.entry(name.clone()).or_default();
				entry.last_run = Some(Instant::now());
				entry.last_duration = Some(start.elapsed());
				// Update average duration (simple moving average)
				let dur_ms = start.elapsed().as_millis() as f64;
				entry.avg_duration_ms = Some(match entry.avg_duration_ms {
					Some(avg) => (avg * 0.8) + (dur_ms * 0.2),
					None => dur_ms,
				});
				match &result {
					Ok(val) => {
						entry.success_count += 1;
						entry.last_success = Some(Instant::now());
						entry.last_result = Some(format!("{:?}", val));
						entry.consecutive_failures = 0;
						backoff = 0;
					}
					Err(e) => {
						entry.failure_count += 1;
						entry.last_failure = Some(Instant::now());
						entry.last_error = Some(format!("{e:?}"));
						entry.consecutive_failures += 1;
						backoff = (backoff + 1).min(6); // Exponential backoff, capped
					}
				}
				// Adaptive scheduling: backoff and jitter
				let base = if backoff == 0 {
					task.min_interval()
				} else {
					task.min_interval() * (1 << backoff)
				};
				let jitter = rng().random_range(0..base.as_millis().max(1) as u64 / 10);
				let sleep_dur = base + Duration::from_millis(jitter);
				tokio::select! {
					_ = tokio::time::sleep(sleep_dur) => {},
					_ = rx.recv() => {}, // On-demand trigger
				}
			}
		});
	}

	pub async fn start(self: Arc<Self>) {
		// No-op: tasks are spawned at registration time
	}

	pub async fn trigger(&self, task_name: &str) {
		if let Some(sender) = self.trigger_senders.get(task_name) {
			let _ = sender.try_send(());
		}
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
