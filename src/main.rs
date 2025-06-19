use clap::Parser;
use rust_watcher::{FileSystemWatcher, WatcherConfig};
use std::path::PathBuf;
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
	let level = if cli.verbose {
		Level::DEBUG
	} else {
		Level::INFO
	};
	tracing_subscriber::fmt().with_max_level(level).init();

	info!("Starting filesystem watcher for path: {:?}", cli.path);

	let config = WatcherConfig {
		path: cli.path,
		recursive: cli.recursive,
		move_timeout_ms: cli.timeout,
	};

	let mut watcher = FileSystemWatcher::new(config).await?;

	// Start watching and handle events
	watcher.start_watching().await?;

	// Keep the program running
	tokio::signal::ctrl_c().await?;
	info!("Shutting down watcher...");

	Ok(())
}
