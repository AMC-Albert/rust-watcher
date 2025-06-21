//! Benchmark utility for filesystem_cache efficiency on large directories
//!
//! Usage: cargo run --bin fs_cache_bench -- <directory_path>
//!
//! This tool will recursively walk the given directory, cache all entries using
//! RedbFilesystemCache, and report timing and throughput statistics.

use redb::Database;
use rust_watcher::database::storage::filesystem_cache::trait_def::FilesystemCacheStorage;
use rust_watcher::database::storage::filesystem_cache::RedbFilesystemCache;
use rust_watcher::database::types::{FilesystemNode, WatchMetadata};
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;
use walkdir::WalkDir;

fn main() {
	let args: Vec<String> = env::args().collect();
	if args.len() != 2 {
		eprintln!("Usage: {} <directory_path>", args[0]);
		std::process::exit(1);
	}
	let dir = PathBuf::from(&args[1]);
	if !dir.is_dir() {
		eprintln!("Provided path is not a directory: {dir:?}");
		std::process::exit(1);
	}

	// Setup database in a temp location
	let db_path = std::env::temp_dir().join(format!("fs_cache_bench-{}.redb", Uuid::new_v4()));
	let db = Arc::new(Database::create(&db_path).expect("Failed to create database"));
	let mut cache = RedbFilesystemCache::new(db.clone());
	let watch_id = Uuid::new_v4();
	let metadata = WatchMetadata {
		watch_id,
		root_path: dir.clone(),
		created_at: chrono::Utc::now(),
		last_scan: None,
		node_count: 0,
		is_active: true,
		config_hash: 0,
		permissions: None,
	};
	pollster::block_on(cache.store_watch_metadata(&metadata))
		.expect("Failed to store watch metadata");

	let start = Instant::now();
	let mut node_count = 0u64;
	for entry in WalkDir::new(&dir).into_iter().filter_map(Result::ok) {
		let path = entry.path();
		match std::fs::metadata(path) {
			Ok(meta) => {
				let node = FilesystemNode::new(path.to_path_buf(), &meta);
				pollster::block_on(cache.store_filesystem_node(&watch_id, &node))
					.expect("Cache insert failed");
				node_count += 1;
			}
			Err(_) => {
				// Could not stat file, skip
			}
		}
	}
	let elapsed = start.elapsed();
	println!(
		"Cached {node_count} nodes from {dir:?} in {elapsed:?} ({:.2} nodes/sec)",
		node_count as f64 / elapsed.as_secs_f64()
	);

	// Optionally, print cache stats
	let stats = pollster::block_on(cache.get_cache_stats(&watch_id)).unwrap_or_default();
	println!(
		"Cache stats: nodes={nodes}, dirs={dirs}, files={files}, symlinks={symlinks}, size={size} bytes",
		nodes = stats.total_nodes,
		dirs = stats.directories,
		files = stats.files,
		symlinks = stats.symlinks,
		size = stats.cache_size_bytes
	);
	println!("Database file: {db_path:?}");
}
