use crate::error::{Result, WatcherError};
use crate::events::{EventType, FileSystemEvent};
use crate::move_detector::MoveDetector;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
use tokio::sync::mpsc as tokio_mpsc;
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone)]
pub struct WatcherConfig {
	pub path: PathBuf,
	pub recursive: bool,
	pub move_timeout_ms: u64,
}

pub struct FileSystemWatcher {
	config: WatcherConfig,
	move_detector: MoveDetector,
	event_sender: Option<tokio_mpsc::UnboundedSender<FileSystemEvent>>,
	event_receiver: Option<tokio_mpsc::UnboundedReceiver<FileSystemEvent>>,
}

impl FileSystemWatcher {
	pub async fn new(config: WatcherConfig) -> Result<Self> {
		let (tx, rx) = tokio_mpsc::unbounded_channel();

		Ok(Self {
			move_detector: MoveDetector::new(config.move_timeout_ms),
			config,
			event_sender: Some(tx),
			event_receiver: Some(rx),
		})
	}

	pub async fn start_watching(&mut self) -> Result<()> {
		let path = self.config.path.clone();
		let recursive = self.config.recursive;

		if !path.exists() {
			return Err(WatcherError::InvalidPath {
				path: path.to_string_lossy().to_string(),
			});
		}

		info!(
			"Starting to watch path: {:?} (recursive: {})",
			path, recursive
		);

		// Create a channel for notify events
		let (notify_tx, notify_rx) = mpsc::channel();

		// Create the notify watcher
		let mut watcher = RecommendedWatcher::new(
			notify_tx,
			Config::default().with_poll_interval(Duration::from_millis(100)),
		)?;

		// Add the path to watch
		let mode = if recursive {
			RecursiveMode::Recursive
		} else {
			RecursiveMode::NonRecursive
		};

		watcher.watch(&path, mode)?;

		// Get the event sender (we'll move it to the processing task)
		let event_tx = self
			.event_sender
			.take()
			.ok_or(WatcherError::NotInitialized)?; // Spawn a task to handle notify events and convert them to our format
		tokio::spawn(async move {
			Self::process_notify_events(notify_rx, event_tx).await;
		});

		// Start the main event processing loop
		self.process_events().await?;

		Ok(())
	}

	async fn process_notify_events(
		notify_rx: mpsc::Receiver<notify::Result<Event>>,
		event_tx: tokio_mpsc::UnboundedSender<FileSystemEvent>,
	) {
		// This needs to run in a blocking thread since notify uses std::sync::mpsc
		tokio::task::spawn_blocking(move || {
			for result in notify_rx {
				match result {
					Ok(event) => {
						debug!("Received notify event: {:?}", event);

						for path in event.paths {
							let fs_event = Self::convert_notify_event(&event.kind, path);

							if let Err(e) = event_tx.send(fs_event) {
								error!("Failed to send event: {}", e);
								break;
							}
						}
					}
					Err(e) => {
						error!("Notify error: {}", e);
					}
				}
			}
		})
		.await
		.unwrap_or_else(|e| {
			error!("Notify processing task panicked: {}", e);
		});
	}
	fn convert_notify_event(kind: &EventKind, path: PathBuf) -> FileSystemEvent {
		let event_type = EventType::from(*kind);

		// Get metadata if the path still exists
		let (is_directory, size) = if path.exists() {
			let metadata = std::fs::metadata(&path).ok();
			let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
			let file_size = metadata.as_ref().filter(|m| m.is_file()).map(|m| m.len());
			(is_dir, file_size)
		} else {
			// For removed files, we can't get metadata, so make educated guesses
			let is_dir = path.extension().is_none(); // Simple heuristic
			(is_dir, None)
		};

		FileSystemEvent::new(event_type, path, is_directory, size)
	}

	async fn process_events(&mut self) -> Result<()> {
		let mut receiver = self
			.event_receiver
			.take()
			.ok_or(WatcherError::NotInitialized)?;

		info!("Event processing loop started");

		while let Some(event) = receiver.recv().await {
			debug!("Processing event: {:?}", event);

			// Process the event through the move detector
			let processed_events = self.move_detector.process_event(event).await;

			// Handle the processed events
			for processed_event in processed_events {
				self.handle_processed_event(processed_event).await;
			}
		}

		warn!("Event processing loop ended");
		Ok(())
	}

	async fn handle_processed_event(&self, event: FileSystemEvent) {
		if event.is_move() {
			if let Some(move_data) = &event.move_data {
				info!(
					"🔄 MOVE DETECTED: {} -> {} (confidence: {:.2}, method: {:?})",
					move_data.source_path.display(),
					move_data.destination_path.display(),
					move_data.confidence,
					move_data.detection_method
				);
			}
		} else {
			match event.event_type {
				EventType::Create => {
					info!(
						"📁 CREATE: {} {}",
						event.path.display(),
						if event.is_directory {
							"(dir)"
						} else {
							"(file)"
						}
					);
				}
				EventType::Remove => {
					info!(
						"🗑️  REMOVE: {} {}",
						event.path.display(),
						if event.is_directory {
							"(dir)"
						} else {
							"(file)"
						}
					);
				}
				EventType::Write => {
					info!("✏️  WRITE: {}", event.path.display());
				}
				EventType::Rename => {
					info!("📝 RENAME: {}", event.path.display());
				}
				EventType::Chmod => {
					info!("🔒 CHMOD: {}", event.path.display());
				}
				EventType::Other(ref desc) => {
					info!("❓ OTHER ({}): {}", desc, event.path.display());
				}
				_ => {
					debug!("Unhandled event type: {:?}", event.event_type);
				}
			}
		}

		// Log as JSON for structured logging/analysis
		if let Ok(json) = event.to_json() {
			debug!("Event JSON: {}", json);
		}
	}

	/// Get a clone of the event sender for external use
	pub fn get_event_sender(&self) -> Option<tokio_mpsc::UnboundedSender<FileSystemEvent>> {
		self.event_sender.clone()
	}
}
