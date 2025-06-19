use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rust_watcher::{EventType, FileSystemEvent};
use rust_watcher::{FileSystemWatcher, MoveDetector, WatcherConfig};
use std::path::PathBuf;
use tempfile::TempDir;

// Benchmark move detection performance
fn benchmark_move_detection(c: &mut Criterion) {
	let rt = tokio::runtime::Runtime::new().unwrap();

	c.bench_function("move_detector_process_event", |b| {
		b.iter(|| {
			rt.block_on(async {
				let mut detector = MoveDetector::new(1000);

				let remove_event = FileSystemEvent::new(
					EventType::Remove,
					PathBuf::from("/test/benchmark_file.txt"),
					false,
					Some(1024),
				);

				let create_event = FileSystemEvent::new(
					EventType::Create,
					PathBuf::from("/test/moved_benchmark_file.txt"),
					false,
					Some(1024),
				);

				detector.process_event(remove_event).await;
				let result = detector.process_event(create_event).await;
				black_box(result);
			});
		});
	});
}

// Benchmark watcher initialization
fn benchmark_watcher_init(c: &mut Criterion) {
	let rt = tokio::runtime::Runtime::new().unwrap();

	c.bench_function("watcher_initialization", |b| {
		b.iter(|| {
			rt.block_on(async {
				let temp_dir = TempDir::new().unwrap();
				let config = WatcherConfig {
					path: temp_dir.path().to_path_buf(),
					recursive: true,
					move_timeout_ms: 1000,
				};

				let watcher = FileSystemWatcher::new(config).await.unwrap();
				black_box(watcher);
			});
		});
	});
}

// Benchmark confidence calculation
fn benchmark_confidence_calculation(c: &mut Criterion) {
	let rt = tokio::runtime::Runtime::new().unwrap();

	c.bench_function("confidence_calculation", |b| {
		b.iter(|| {
			rt.block_on(async {
				let mut detector = MoveDetector::new(1000);

				// Create multiple events to test batch processing
				let mut events = Vec::new();

				for i in 0..100 {
					let remove_event = FileSystemEvent::new(
						EventType::Remove,
						PathBuf::from(format!("/test/file_{}.txt", i)),
						false,
						Some(1024 + i as u64),
					);

					let create_event = FileSystemEvent::new(
						EventType::Create,
						PathBuf::from(format!("/test/moved_file_{}.txt", i)),
						false,
						Some(1024 + i as u64),
					);

					events.push((remove_event, create_event));
				}

				// Process all events
				for (remove, create) in events {
					detector.process_event(remove).await;
					let result = detector.process_event(create).await;
					black_box(result);
				}
			});
		});
	});
}

criterion_group!(
	benches,
	benchmark_move_detection,
	benchmark_watcher_init,
	benchmark_confidence_calculation
);
criterion_main!(benches);
