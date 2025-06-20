use rust_watcher::{start, MoveDetectorConfig, WatcherConfig};
use std::path::PathBuf;
use std::time::Duration;
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
	let move_config = MoveDetectorConfig {
		timeout: Duration::from_millis(2000),
		..Default::default()
	};
	let config = WatcherConfig {
		path: watch_path,
		recursive: true,
		move_detector_config: Some(move_config),
		error_recovery_config: None,
		database_config: None,
	};

	info!("Monitor started. Press Ctrl+C to stop.");
	// Start watching and get the event receiver
	let (_handle, mut event_receiver) = start(config)?;

	// Process events in a custom way
	tokio::spawn(async move {
		let mut event_count = 0;
		while let Some(event) = event_receiver.recv().await {
			event_count += 1;

			// Example: Count different event types
			if event_count % 10 == 0 {
				info!("📊 Processed {} events so far", event_count);
			}

			// Example: Custom handling for specific event types
			if event.is_move() {
				info!("🎯 Custom handler: Detected a move operation!");
			}
		}
	});

	// Wait for interrupt signal
	tokio::signal::ctrl_c().await?;
	info!("Shutting down monitor...");

	Ok(())
}
