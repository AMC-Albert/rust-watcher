use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rust_watcher::{DatabaseAdapter, DatabaseConfig, EventType, FileSystemEvent};
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::runtime::Runtime;

fn bench_simple_storage(c: &mut Criterion) {
	let rt = Runtime::new().unwrap();

	c.bench_function("simple_event_storage", |b| {
		b.iter(|| {
			rt.block_on(async {
				let temp_dir = TempDir::new().unwrap();
				let config = DatabaseConfig {
					database_path: temp_dir.path().join("test.db"),
					event_retention: Duration::from_secs(3600),
					..Default::default()
				};

				let adapter = DatabaseAdapter::new(config).await.unwrap();
				let event = FileSystemEvent::new(
					EventType::Create,
					PathBuf::from("/test/file.txt"),
					false,
					Some(1024),
				);

				adapter.store_event(black_box(&event)).await.unwrap();
			})
		})
	});
}

criterion_group!(benches, bench_simple_storage);
criterion_main!(benches);
