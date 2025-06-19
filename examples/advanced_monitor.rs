use rust_watcher::{FileSystemWatcher, WatcherConfig};
use std::path::PathBuf;
use tracing::{info, Level};

/// Example demonstrating how to use the filesystem watcher
#[tokio::main]
async fn main() -> anyhow::Result<()> {
	// Initialize logging
	tracing_subscriber::fmt().with_max_level(Level::INFO).init();

	let watch_path = std::env::args()
		.nth(1)
		.map(PathBuf::from)
		.unwrap_or_else(|| std::env::current_dir().unwrap());

	info!("Starting filesystem monitor for: {:?}", watch_path);

	let config = WatcherConfig {
		path: watch_path,
		recursive: true,
		move_timeout_ms: 2000,
	};

	let mut watcher = FileSystemWatcher::new(config).await?;

	info!("Monitor started. Press Ctrl+C to stop.");

	// Start watching - this will run until interrupted
	watcher.start_watching().await?;

	Ok(())
}
