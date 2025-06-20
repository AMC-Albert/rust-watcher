//! Database performance benchmarks
//!
//! These benchmarks establish baseline performance for database operations
//! before implementing the filesystem cache.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use rust_watcher::{DatabaseAdapter, DatabaseConfig, EventType, FileSystemEvent};
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::runtime::Runtime;

// Benchmark event storage operations
fn bench_event_storage(c: &mut Criterion) {
	let rt = Runtime::new().unwrap();

	c.bench_function("event_storage_single", |b| {
		b.iter(|| {
			rt.block_on(async {
				let temp_dir = TempDir::new().expect("Failed to create temp dir");
				let config = DatabaseConfig {
					database_path: temp_dir.path().join("bench.db"),
					event_retention: Duration::from_secs(3600), // 1 hour
					..Default::default()
				};

				let adapter = DatabaseAdapter::new(config).await.unwrap();

				let event = FileSystemEvent::new(
					EventType::Create,
					PathBuf::from("/test/path.txt"),
					false,
					Some(1024),
				);

				adapter.store_event(black_box(&event)).await.unwrap();
			})
		})
	});
}

// Benchmark batch event storage
fn bench_batch_event_storage(c: &mut Criterion) {
	let rt = Runtime::new().unwrap();

	let mut group = c.benchmark_group("batch_event_storage");

	for batch_size in [10, 100, 1000].iter() {
		group.bench_with_input(
			BenchmarkId::new("events", batch_size),
			batch_size,
			|b, &batch_size| {
				b.iter(|| {
					rt.block_on(async {
						let temp_dir = TempDir::new().expect("Failed to create temp dir");
						let config = DatabaseConfig {
							database_path: temp_dir.path().join("batch_bench.db"),
							event_retention: Duration::from_secs(3600),
							..Default::default()
						};

						let adapter = DatabaseAdapter::new(config).await.unwrap();

						for i in 0..batch_size {
							let event = FileSystemEvent::new(
								EventType::Create,
								PathBuf::from(format!("/test/path_{}.txt", i)),
								false,
								Some(1024),
							);

							adapter.store_event(black_box(&event)).await.unwrap();
						}
					})
				})
			},
		);
	}
	group.finish();
}

// Benchmark event querying operations
fn bench_event_querying(c: &mut Criterion) {
	let rt = Runtime::new().unwrap();

	c.bench_function("event_query_by_time_range", |b| {
		b.iter(|| {
			rt.block_on(async {
				let temp_dir = TempDir::new().expect("Failed to create temp dir");
				let config = DatabaseConfig {
					database_path: temp_dir.path().join("query_bench.db"),
					event_retention: Duration::from_secs(3600),
					..Default::default()
				};

				let adapter = DatabaseAdapter::new(config).await.unwrap();

				// Insert some test data
				for i in 0..100 {
					let event = FileSystemEvent::new(
						EventType::Create,
						PathBuf::from(format!("/test/setup_{}.txt", i)),
						false,
						Some(1024),
					);
					adapter.store_event(&event).await.unwrap();
				}

				// Query events by time range (this is what we're benchmarking)
				let start_time = chrono::Utc::now() - chrono::Duration::minutes(5);
				let end_time = chrono::Utc::now();
				let events = adapter
					.find_events_by_time_range(start_time, end_time)
					.await
					.unwrap();
				black_box(events);
			})
		})
	});
}

// Benchmark database cleanup operations
fn bench_cleanup_operations(c: &mut Criterion) {
	let rt = Runtime::new().unwrap();

	c.bench_function("cleanup_old_events", |b| {
		b.iter(|| {
			rt.block_on(async {
				let temp_dir = TempDir::new().expect("Failed to create temp dir");
				let config = DatabaseConfig {
					database_path: temp_dir.path().join("cleanup_bench.db"),
					event_retention: Duration::from_millis(100), // Very short for testing
					..Default::default()
				};

				let adapter = DatabaseAdapter::new(config).await.unwrap();

				// Insert test data that will be expired
				for i in 0..50 {
					let event = FileSystemEvent::new(
						EventType::Create,
						PathBuf::from(format!("/test/expired_{}.txt", i)),
						false,
						Some(1024),
					);
					adapter.store_event(&event).await.unwrap();
				}

				// Wait for events to expire
				tokio::time::sleep(Duration::from_millis(200)).await;

				// Cleanup (this is what we're benchmarking)
				let cleaned = adapter.cleanup_old_events().await.unwrap();
				black_box(cleaned);
			})
		})
	});
}

// Benchmark database stats collection
fn bench_stats_collection(c: &mut Criterion) {
	let rt = Runtime::new().unwrap();

	c.bench_function("get_database_stats", |b| {
		b.iter(|| {
			rt.block_on(async {
				let temp_dir = TempDir::new().expect("Failed to create temp dir");
				let config = DatabaseConfig {
					database_path: temp_dir.path().join("stats_bench.db"),
					event_retention: Duration::from_secs(3600),
					..Default::default()
				};

				let adapter = DatabaseAdapter::new(config).await.unwrap();

				// Insert some test data
				for i in 0..100 {
					let event = FileSystemEvent::new(
						EventType::Create,
						PathBuf::from(format!("/test/stats_{}.txt", i)),
						false,
						Some(1024),
					);
					adapter.store_event(&event).await.unwrap();
				}

				// Get stats (this is what we're benchmarking)
				let stats = adapter.get_stats().await.unwrap();
				black_box(stats);
			})
		})
	});
}

criterion_group!(
	benches,
	bench_event_storage,
	bench_batch_event_storage,
	bench_event_querying,
	bench_cleanup_operations,
	bench_stats_collection
);
criterion_main!(benches);
