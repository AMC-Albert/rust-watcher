use crate::error::{Result, WatcherError};
use crate::events::{EventType, FileSystemEvent};
use crate::move_detection::{MoveDetector, MoveDetectorConfig};
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher as NotifyWatcher};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone)]
pub struct WatcherConfig {
	pub path: PathBuf,
	pub recursive: bool,
	pub move_detector_config: Option<MoveDetectorConfig>,
}

#[derive(Debug)]
pub struct WatcherHandle {
	stop_sender: oneshot::Sender<()>,
}

impl WatcherHandle {
	pub async fn stop(self) -> Result<()> {
		self.stop_sender
			.send(())
			.map_err(|_| WatcherError::StopSignal)
	}
}

pub fn start(config: WatcherConfig) -> Result<(WatcherHandle, mpsc::Receiver<FileSystemEvent>)> {
	if !config.path.exists() {
		return Err(WatcherError::InvalidPath {
			path: config.path.to_string_lossy().to_string(),
		});
	}

	let (event_tx, event_rx) = mpsc::channel(100);
	let (stop_tx, stop_rx) = oneshot::channel();

	let handle = WatcherHandle {
		stop_sender: stop_tx,
	};

	tokio::spawn(run_watcher(config, event_tx, stop_rx));

	Ok((handle, event_rx))
}

async fn run_watcher(
	config: WatcherConfig,
	event_tx: mpsc::Sender<FileSystemEvent>,
	mut stop_rx: oneshot::Receiver<()>,
) {
	let (notify_tx, notify_rx) = std::sync::mpsc::channel();

	let notify_config = Config::default().with_poll_interval(Duration::from_millis(50));

	let mut watcher: RecommendedWatcher = match RecommendedWatcher::new(
		move |res| {
			if let Ok(event) = res {
				if let Err(e) = notify_tx.send(event) {
					error!("Error sending notify event: {}", e);
				}
			} else if let Err(e) = res {
				error!("Notify error: {}", e);
			}
		},
		notify_config,
	) {
		Ok(w) => w,
		Err(e) => {
			error!("Failed to create watcher: {}", e);
			return;
		}
	};

	let mode = if config.recursive {
		RecursiveMode::Recursive
	} else {
		RecursiveMode::NonRecursive
	};
	if let Err(e) = watcher.watch(&config.path, mode) {
		error!("Failed to watch path: {}", e);
		return;
	}

	let move_detector_config = config.move_detector_config.unwrap_or_default();
	let mut move_detector = MoveDetector::new(move_detector_config);

	let (raw_event_tx, mut raw_event_rx) = mpsc::channel(100);
	let _blocking_task = tokio::task::spawn_blocking(move || {
		for event in notify_rx {
			if raw_event_tx.blocking_send(event).is_err() {
				error!("Event receiver dropped, stopping notify thread.");
				break;
			}
		}
	});

	loop {
		tokio::select! {
			_ = &mut stop_rx => {
				info!("Watcher shutdown requested, stopping event processing.");
				break;
			}			Some(event) = raw_event_rx.recv() => {
				debug!("Raw filesystem event received: kind={:?}, paths={:?}", event.kind, event.paths);
				for path in event.paths {
					let fs_event = convert_notify_event(&event.kind, path, &move_detector);
					debug!("Converted to filesystem event: type={:?}, path={:?}, is_dir={}, size={:?}",
						fs_event.event_type, fs_event.path, fs_event.is_directory, fs_event.size);

					let processed_events = move_detector.process_event(fs_event).await;

					for processed in processed_events {
						// Log the final processed event
						log_processed_event(&processed);

						if event_tx.send(processed).await.is_err() {
							warn!("Event receiver dropped, ending processing loop.");
							return;
						}
					}
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

fn convert_notify_event(
	kind: &EventKind,
	path: PathBuf,
	move_detector: &MoveDetector,
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
		};

		assert_eq!(config.path, temp_dir.path());
		assert!(config.recursive);
		assert!(config.move_detector_config.is_none());
	}

	#[test]
	fn test_watcher_config_with_move_detector() {
		let temp_dir = TempDir::new().unwrap();
		let move_config = MoveDetectorConfig::default();
		let config = WatcherConfig {
			path: temp_dir.path().to_path_buf(),
			recursive: false,
			move_detector_config: Some(move_config),
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
		};

		let result = start(config);
		assert!(result.is_err());

		match result.unwrap_err() {
			WatcherError::InvalidPath { path } => {
				assert!(path.contains("nonexistent"));
			}
			other => panic!("Expected InvalidPath error, got: {:?}", other),
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
