use crate::database::storage::filesystem_cache::RedbFilesystemCache;
use crate::database::{DatabaseAdapter, DatabaseConfig};
use crate::error::{ErrorRecoveryConfig, Result, WatcherError};
use crate::events::{EventType, FileSystemEvent};
use crate::move_detection::{MoveDetector, MoveDetectorConfig};
use crate::retry::RetryManager;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher as NotifyWatcher};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone)]
pub struct WatcherConfig {
	pub path: PathBuf,
	pub recursive: bool,
	pub move_detector_config: Option<MoveDetectorConfig>,
	pub error_recovery_config: Option<ErrorRecoveryConfig>,
	pub database_config: Option<DatabaseConfig>,
}

impl WatcherConfig {
	/// Validate the watcher configuration
	pub fn validate(&self) -> Result<()> {
		// Check if path exists
		if !self.path.exists() {
			return Err(WatcherError::InvalidPath {
				path: self.path.to_string_lossy().to_string(),
			});
		}

		// Check if path is readable
		match std::fs::metadata(&self.path) {
			Ok(metadata) => {
				if !metadata.is_dir() && !metadata.is_file() {
					return Err(WatcherError::ConfigurationError {
						parameter: "path".to_string(),
						reason: "Path is neither a file nor a directory".to_string(),
						expected: "directory or file".to_string(),
						actual: format!("{actual:?}", actual = metadata.file_type()),
					});
				}
			}
			Err(io_err) => match io_err.kind() {
				std::io::ErrorKind::PermissionDenied => {
					return Err(WatcherError::from_permission_denied(
						"read metadata",
						&self.path.to_string_lossy(),
						io_err,
					));
				}
				_ => {
					return Err(WatcherError::filesystem_error(
						"read metadata",
						&io_err.to_string(),
					));
				}
			},
		}

		// Validate move detector config if present
		if let Some(ref move_config) = self.move_detector_config {
			if let Err(reason) = move_config.validate() {
				return Err(WatcherError::ConfigurationError {
					parameter: "move_detector_config".to_string(),
					reason,
					expected: "valid move detector configuration".to_string(),
					actual: "invalid configuration".to_string(),
				});
			}
		}

		Ok(())
	}
	/// Create a configuration with error recovery
	pub fn with_error_recovery(mut self, config: ErrorRecoveryConfig) -> Self {
		self.error_recovery_config = Some(config);
		self
	}

	/// Create a configuration with database support
	pub fn with_database(mut self, config: DatabaseConfig) -> Self {
		self.database_config = Some(config);
		self
	}
}

#[derive(Debug)]
pub struct WatcherHandle {
	stop_sender: oneshot::Sender<()>,
}

impl WatcherHandle {
	pub async fn stop(self) -> Result<()> {
		self.stop_sender.send(()).map_err(|_| WatcherError::StopSignal)
	}
}

pub fn start(config: WatcherConfig) -> Result<(WatcherHandle, mpsc::Receiver<FileSystemEvent>)> {
	// Validate configuration first
	config.validate()?;

	let (event_tx, event_rx) = mpsc::channel(100);
	let (stop_tx, stop_rx) = oneshot::channel();

	let handle = WatcherHandle { stop_sender: stop_tx };

	tokio::spawn(run_watcher(config, event_tx, stop_rx));

	Ok((handle, event_rx))
}

async fn run_watcher(
	config: WatcherConfig, event_tx: mpsc::Sender<FileSystemEvent>,
	mut stop_rx: oneshot::Receiver<()>,
) {
	// Initialize database adapter if configured
	let database = if let Some(db_config) = config.database_config.clone() {
		match DatabaseAdapter::new(db_config).await {
			Ok(adapter) => {
				info!("Database adapter initialized successfully");
				adapter
			}
			Err(e) => {
				warn!(
					"Failed to initialize database, continuing without persistence: {}",
					e
				);
				DatabaseAdapter::disabled()
			}
		}
	} else {
		DatabaseAdapter::disabled()
	};

	// Initialize persistent filesystem cache
	let mut _dummy_tempdir = None;
	let mut fs_cache = if let Some(cache) = database.get_filesystem_cache().await {
		cache
	} else {
		// Use a unique dummy DB file per watcher instance to avoid concurrency issues
		let tempdir = TempDir::new().expect("Failed to create tempdir for dummy DB");
		let dummy_db_path =
			tempdir.path().join(format!("dummy-{id}.redb", id = uuid::Uuid::new_v4()));
		let cache =
			RedbFilesystemCache::new(Arc::new(redb::Database::create(&dummy_db_path).unwrap()));
		_dummy_tempdir = Some(tempdir); // Hold tempdir so file is deleted on drop
		cache
	};

	let move_detector_config = config.move_detector_config.unwrap_or_default();
	let mut move_detector = MoveDetector::new(move_detector_config, &mut fs_cache);

	// Initialize retry manager
	let retry_config = config.error_recovery_config.unwrap_or_default();
	let retry_manager = RetryManager::new(retry_config); // Initialize watcher with retry logic
	let watcher_result = retry_manager
		.execute_simple("initialize_watcher", || {
			create_filesystem_watcher(&config.path)
		})
		.await;

	let mut watcher = match watcher_result {
		Ok(w) => w,
		Err(e) => {
			error!(
				"Failed to initialize filesystem watcher after retries: {}",
				e
			);
			return;
		}
	};

	let (raw_event_tx, mut raw_event_rx) = mpsc::channel(100);
	let (notify_tx, notify_rx) = std::sync::mpsc::channel(); // Set up the watcher callback with direct error handling for now
	if let Err(e) = setup_watcher_callback(&mut watcher, &config.path, notify_tx.clone()).await {
		error!("Failed to setup watcher callback: {}", e);
		return;
	}

	// Spawn blocking task to bridge sync notify channel to async
	let _blocking_task = tokio::task::spawn_blocking(move || {
		for event in notify_rx {
			if raw_event_tx.blocking_send(event).is_err() {
				debug!("Event receiver dropped, stopping notify thread.");
				break;
			}
		}
	});

	// Main event processing loop with error recovery
	loop {
		tokio::select! {
			_ = &mut stop_rx => {
				info!("Watcher shutdown requested, stopping event processing.");
				break;
			}
			Some(event) = raw_event_rx.recv() => {
				if let Err(e) = process_single_event(event.clone(), &mut move_detector, &database, &event_tx).await {
					warn!("Failed to process filesystem event: {} - Event: {:?}", e, event);
				}
			}
			else => {
				info!("Raw event stream ended, stopping processing loop.");
				break;
			}
		}
	}
	info!("Watcher event loop finished. Channel will be closed.");
}

/// Create a filesystem watcher with proper error handling
async fn create_filesystem_watcher(path: &std::path::Path) -> Result<RecommendedWatcher> {
	let notify_config = Config::default().with_poll_interval(Duration::from_millis(50));

	let watcher = RecommendedWatcher::new(
		|_| {}, // Placeholder callback, will be set up later
		notify_config,
	)
	.map_err(|e| {
		error!("Failed to create filesystem watcher: {}", e);
		match &e.kind {
			notify::ErrorKind::Generic(msg) if msg.contains("permission") => {
				WatcherError::from_permission_denied(
					"create watcher",
					&path.to_string_lossy(),
					std::io::Error::new(std::io::ErrorKind::PermissionDenied, msg.clone()),
				)
			}
			notify::ErrorKind::Io(io_err) => match io_err.kind() {
				std::io::ErrorKind::PermissionDenied => WatcherError::from_permission_denied(
					"create watcher",
					&path.to_string_lossy(),
					std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Permission denied"),
				),
				_ => WatcherError::Notify(e),
			},
			_ => WatcherError::Notify(e),
		}
	})?;

	Ok(watcher)
}

/// Setup watcher callback and start watching
async fn setup_watcher_callback(
	watcher: &mut RecommendedWatcher, path: &std::path::Path,
	notify_tx: std::sync::mpsc::Sender<notify::Event>,
) -> Result<()> {
	// Replace the watcher callback
	*watcher = RecommendedWatcher::new(
		move |res| {
			if let Ok(event) = res {
				if let Err(e) = notify_tx.send(event) {
					error!("Error sending notify event: {}", e);
				}
			} else if let Err(e) = res {
				error!("Notify error: {}", e);
			}
		},
		Config::default().with_poll_interval(Duration::from_millis(50)),
	)
	.map_err(WatcherError::Notify)?;

	// Start watching the path
	let mode = RecursiveMode::Recursive; // You could make this configurable
	watcher.watch(path, mode).map_err(|e| {
		error!("Failed to watch path {:?}: {}", path, e);
		match &e.kind {
			notify::ErrorKind::Generic(msg) if msg.contains("permission") => {
				WatcherError::from_permission_denied(
					"watch path",
					&path.to_string_lossy(),
					std::io::Error::new(std::io::ErrorKind::PermissionDenied, msg.clone()),
				)
			}
			notify::ErrorKind::Io(io_err) => match io_err.kind() {
				std::io::ErrorKind::PermissionDenied => WatcherError::from_permission_denied(
					"watch path",
					&path.to_string_lossy(),
					std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Permission denied"),
				),
				_ => WatcherError::Notify(e),
			},
			_ => WatcherError::Notify(e),
		}
	})?;

	info!("Successfully started watching path: {:?}", path);
	Ok(())
}

/// Process a single filesystem event with proper error handling
async fn process_single_event<'a>(
	event: notify::Event, move_detector: &mut MoveDetector<'a>, database: &DatabaseAdapter,
	event_tx: &mpsc::Sender<FileSystemEvent>,
) -> Result<()> {
	debug!(
		"Raw filesystem event received: kind={:?}, paths={:?}",
		event.kind, event.paths
	);

	for path in event.paths {
		let fs_event = convert_notify_event(&event.kind, path, move_detector);
		debug!(
			"Converted to filesystem event: type={:?}, path={:?}, is_dir={}, size={:?}",
			fs_event.event_type, fs_event.path, fs_event.is_directory, fs_event.size
		);

		// Store event in database if enabled
		if let Err(e) = database.store_event(&fs_event).await {
			warn!("Failed to store event in database: {}", e);
			// Continue processing even if database storage fails
		}
		// Store metadata if this is a create/write event
		if matches!(fs_event.event_type, EventType::Create | EventType::Write) {
			if let Ok(metadata) = std::fs::metadata(&fs_event.path) {
				if let Err(e) = database.store_metadata(&fs_event.path, &metadata).await {
					warn!("Failed to store metadata in database: {}", e);
				}
			}
		}

		let processed_events = move_detector.process_event(fs_event).await;

		for processed in processed_events {
			// Log the final processed event
			log_processed_event(&processed);

			event_tx.send(processed).await.map_err(|_| {
				warn!("Event receiver dropped, ending processing loop.");
				WatcherError::ChannelSend
			})?;
		}
	}

	Ok(())
}

fn convert_notify_event(
	kind: &EventKind, path: PathBuf, move_detector: &MoveDetector<'_>,
) -> FileSystemEvent {
	let event_type = EventType::from(*kind);
	debug!(
		"Converting notify event: kind={:?}, path={:?} -> event_type={:?}",
		kind, path, event_type
	);

	let (is_directory, size) = if path.exists() {
		debug!("Path exists, reading metadata: {:?}", path);
		let metadata = std::fs::metadata(&path).ok();
		let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
		let file_size = metadata.as_ref().filter(|m| m.is_file()).map(|m| m.len());
		debug!("Metadata read: is_dir={}, size={:?}", is_dir, file_size);
		(is_dir, file_size)
	} else {
		debug!("Path does not exist, using heuristics: {:?}", path);
		// File/directory no longer exists - use improved heuristics
		let is_dir = match move_detector.infer_path_type(&path) {
			Some(is_directory) => {
				debug!(
					"Move detector inferred path type: is_directory={}",
					is_directory
				);
				is_directory
			}
			None => {
				// Fallback to original heuristic with improved logging
				let fallback_is_dir = path.extension().is_none();
				warn!("Could not determine path type for removed path: {:?}, falling back to extension-based heuristic: is_directory={}", 
					path, fallback_is_dir);
				fallback_is_dir
			}
		};
		(is_dir, None)
	};

	debug!(
		"Final conversion result: type={:?}, path={:?}, is_dir={}, size={:?}",
		event_type, path, is_directory, size
	);

	FileSystemEvent::new(event_type, path, is_directory, size)
}

/// Log processed events with appropriate level:
/// - INFO level for confirmed moves/renames
/// - DEBUG level for all other events
fn log_processed_event(event: &FileSystemEvent) {
	match &event.event_type {
		EventType::Move => {
			if let Some(move_data) = &event.move_data {
				info!(
					"MOVE DETECTED: {:?} -> {:?} (confidence: {:.2}, method: {:?})",
					move_data.source_path,
					move_data.destination_path,
					move_data.confidence,
					move_data.detection_method
				);
			} else {
				info!("MOVE: {:?} (generic move)", event.path);
			}
		}
		EventType::RenameFrom | EventType::RenameTo | EventType::Rename => {
			info!("RENAME: {:?} (type: {:?})", event.path, event.event_type);
		}
		_ => {
			debug!(
				"{}: {:?} (dir: {}, size: {:?})",
				match event.event_type {
					EventType::Create => "CREATE",
					EventType::Write => "WRITE",
					EventType::Remove => "REMOVE",
					EventType::Chmod => "CHMOD",
					EventType::Other(ref s) => s,
					_ => "OTHER",
				},
				event.path,
				event.is_directory,
				event.size
			);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::path::PathBuf;
	use tempfile::TempDir;
	#[test]
	fn test_watcher_config_creation() {
		let temp_dir = TempDir::new().unwrap();
		let config = WatcherConfig {
			path: temp_dir.path().to_path_buf(),
			recursive: true,
			move_detector_config: None,
			error_recovery_config: None,
			database_config: None,
		};

		assert_eq!(config.path, temp_dir.path());
		assert!(config.recursive);
		assert!(config.move_detector_config.is_none());
		assert!(config.error_recovery_config.is_none());
	}
	#[test]
	fn test_watcher_config_with_move_detector() {
		let temp_dir = TempDir::new().unwrap();
		let move_config = MoveDetectorConfig::default();
		let config = WatcherConfig {
			path: temp_dir.path().to_path_buf(),
			recursive: false,
			move_detector_config: Some(move_config),
			error_recovery_config: None,
			database_config: None,
		};

		assert!(!config.recursive);
		assert!(config.move_detector_config.is_some());
	}
	#[test]
	fn test_start_with_invalid_path() {
		let config = WatcherConfig {
			path: PathBuf::from("/nonexistent/path/that/should/not/exist"),
			recursive: true,
			move_detector_config: None,
			error_recovery_config: None,
			database_config: None,
		};

		let result = start(config);
		assert!(result.is_err());

		match result.unwrap_err() {
			WatcherError::InvalidPath { path } => {
				assert!(path.contains("nonexistent"));
			}
			other => panic!("Expected InvalidPath error, got: {other:?}"),
		}
	}

	#[test]
	fn test_watcher_handle_creation() {
		// Test that WatcherHandle can be created (unit test for the struct)
		let (tx, _rx) = oneshot::channel();
		let handle = WatcherHandle { stop_sender: tx };

		// Test that handle exists and has expected structure
		// We can't easily test the stop functionality without async runtime
		assert!(std::mem::size_of_val(&handle) > 0);
	}
}
