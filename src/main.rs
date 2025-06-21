use clap::Parser;
use rust_watcher::{start, MoveDetectorConfig, WatcherConfig};
use std::path::PathBuf;
use std::time::Duration;
use tracing::{info, Level};

#[derive(Parser)]
#[command(name = "rust-watcher")]
#[command(
	about = "A comprehensive filesystem watcher that tracks file/directory moves and renames"
)]
struct Cli {
	/// Path to watch
	#[arg(short, long)]
	path: PathBuf,

	/// Enable verbose logging
	#[arg(short, long)]
	verbose: bool,

	/// Recursive watching
	#[arg(short, long, default_value_t = true)]
	recursive: bool,

	/// Timeout for move detection in milliseconds
	#[arg(short, long, default_value_t = 1000)]
	timeout: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let cli = Cli::parse();

	// Initialize tracing
	let level = if cli.verbose { Level::DEBUG } else { Level::INFO };
	tracing_subscriber::fmt().with_max_level(level).init();
	info!("Starting filesystem watcher for path: {:?}", cli.path);
	let move_config =
		MoveDetectorConfig { timeout: Duration::from_millis(cli.timeout), ..Default::default() };
	let config = WatcherConfig {
		watch_id: uuid::Uuid::new_v4(), // Generate a unique watch ID for this session
		path: cli.path,
		recursive: cli.recursive,
		move_detector_config: Some(move_config),
		error_recovery_config: None,
		database_config: Some(rust_watcher::DatabaseConfig {
			database_path: PathBuf::from("./watcher.redb"),
			..Default::default()
		}),
	};

	// Start watching and get the event receiver
	let (_handle, mut event_receiver) = start(config)?;

	// Handle events in a loop
	tokio::spawn(async move {
		while let Some(event) = event_receiver.recv().await {
			// Events are already logged by the watcher's handle_processed_event
			// But we could do additional processing here if needed
			let _ = event; // Acknowledge we received the event
		}
	});

	// Keep the program running
	tokio::signal::ctrl_c().await?;
	info!("Shutting down watcher...");

	Ok(())
}
