use crate::error::{Result, WatcherError};
use crate::events::{EventType, FileSystemEvent};
use crate::move_detector::{MoveDetector, MoveDetectorConfig};
use log::{error, info, warn};
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher as NotifyWatcher};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

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
				for path in event.paths {
					let fs_event = convert_notify_event(&event.kind, path, &move_detector);
					let processed_events = move_detector.process_event(fs_event).await;
					for processed in processed_events {
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

	let (is_directory, size) = if path.exists() {
		let metadata = std::fs::metadata(&path).ok();
		let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
		let file_size = metadata.as_ref().filter(|m| m.is_file()).map(|m| m.len());
		(is_dir, file_size)
	} else {
		// File/directory no longer exists - use improved heuristics
		let is_dir = match move_detector.infer_path_type(&path) {
			Some(is_directory) => is_directory,
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

	FileSystemEvent::new(event_type, path, is_directory, size)
}
