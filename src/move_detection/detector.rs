use crate::events::{EventType, FileSystemEvent, MoveEvent};
use crate::move_detection::config::MoveDetectorConfig;
use crate::move_detection::events::{PendingEvent, PendingEventsStorage};
use crate::move_detection::heuristics::PathTypeInference;
use crate::move_detection::matching::{MetadataExtractor, MoveMatching};
use crate::move_detection::metadata::{FileMetadata, MetadataCache};
use crate::move_detection::monitoring::{PendingEventsSummary, ResourceStats};
use std::path::Path;
use tokio::time::Instant;
use tracing::{debug, warn};

pub struct MoveDetector {
	/// Event storage organized for efficient lookups
	pending_events: PendingEventsStorage,

	/// Cache metadata for files we've seen (for use when they get removed)
	metadata_cache: MetadataCache,

	/// Configuration for move detection
	config: MoveDetectorConfig,

	/// Resource usage statistics
	stats: ResourceStats,
}

impl MoveDetector {
	pub fn new(config: MoveDetectorConfig) -> Self {
		Self {
			pending_events: PendingEventsStorage::new(),
			metadata_cache: MetadataCache::new(),
			config,
			stats: ResourceStats::new(),
		}
	}

	/// Create a new MoveDetector with default configuration and custom timeout
	pub fn with_timeout(timeout_ms: u64) -> Self {
		let config = MoveDetectorConfig::with_timeout(timeout_ms);
		Self::new(config)
	}

	/// Process a filesystem event and potentially detect moves
	pub async fn process_event(&mut self, event: FileSystemEvent) -> Vec<FileSystemEvent> {
		self.stats.record_event_processed();

		// Cache metadata for files we can still access (not for remove events)
		if !matches!(event.event_type, EventType::Remove | EventType::RenameFrom) {
			self.cache_file_metadata(&event.path).await;
		}

		self.cleanup_expired_events().await;

		match event.event_type {
			EventType::Remove => self.handle_remove_event(event).await,
			EventType::Create => self.handle_create_event(event).await,
			EventType::RenameFrom => self.handle_rename_from_event(event).await,
			EventType::RenameTo => self.handle_rename_to_event(event).await,
			EventType::Rename => {
				// Generic rename event - treat as both remove and create
				debug!("Processing generic rename event for: {:?}", event.path);
				vec![event] // Pass through for now
			}
			_ => vec![event], // Pass through other events
		}
	}

	/// Infer whether a removed path was likely a directory based on available context
	pub fn infer_path_type(&self, path: &Path) -> Option<bool> {
		PathTypeInference::infer_path_type(path, &self.metadata_cache, &self.pending_events)
	}

	/// Get better heuristics for directory detection on removed paths
	pub fn get_path_type_heuristics(
		&self,
		path: &Path,
	) -> crate::move_detection::heuristics::PathTypeHeuristics {
		PathTypeInference::get_path_type_heuristics(
			path,
			&self.metadata_cache,
			&self.pending_events,
		)
	}

	/// Get resource usage statistics
	pub fn get_resource_stats(&mut self) -> ResourceStats {
		self.stats
			.update(&self.pending_events, &self.metadata_cache);
		self.stats.clone()
	}

	/// Get summary of pending events for debugging
	pub fn get_pending_events_summary(&self) -> PendingEventsSummary {
		PendingEventsSummary::from_storage(&self.pending_events)
	}

	/// Cache metadata for a file path
	async fn cache_file_metadata(&mut self, path: &Path) {
		if let Ok(metadata) = std::fs::metadata(path) {
			let size = if metadata.is_file() {
				Some(metadata.len())
			} else {
				None
			};
			let windows_id = MetadataExtractor::get_windows_id(path).await;

			let file_metadata = FileMetadata::new(size, windows_id);
			self.metadata_cache
				.insert(path.to_path_buf(), file_metadata);
		}
	}

	async fn handle_remove_event(&mut self, mut event: FileSystemEvent) -> Vec<FileSystemEvent> {
		// Try to get cached metadata for this file (since it's being removed)
		let cached_metadata = self.metadata_cache.remove(&event.path);

		// Update event with cached metadata if available
		if let Some(metadata) = &cached_metadata {
			if event.size.is_none() {
				event.size = metadata.size;
			}
		}

		let pending = PendingEvent::new(event.clone())
			.with_inode(MetadataExtractor::get_inode(&event.path).await)
			.with_windows_id(cached_metadata.as_ref().and_then(|m| m.windows_id));

		// Check if this removal matches a recent create (reverse move detection)
		if let Some(matching_create) =
			MoveMatching::find_matching_create(&pending, &self.pending_events, &self.config).await
		{
			let confidence =
				MoveMatching::calculate_confidence(&pending, &matching_create, &self.config);
			let detection_method =
				MoveMatching::determine_detection_method(&pending, &matching_create);

			let move_event = MoveEvent {
				source_path: matching_create.event.path.clone(),
				destination_path: event.path.clone(),
				confidence,
				detection_method,
			};

			self.stats.record_move_detected(confidence);

			let mut move_event_fs = matching_create.event.clone();
			move_event_fs = move_event_fs.with_move_data(move_event);

			// Remove the matching create from pending
			self.pending_events
				.remove_create_by_id(matching_create.event.id);

			debug!(
				"Detected move: {:?} -> {:?} (confidence: {:.2})",
				matching_create.event.path, event.path, confidence
			);
			return vec![move_event_fs];
		}

		// Store this removal as pending
		if self.pending_events.count_removes() < self.config.max_pending_events {
			self.pending_events.add_remove(pending);
		} else {
			warn!(
				"Too many pending remove events, dropping event for: {:?}",
				event.path
			);
		}

		vec![event]
	}

	async fn handle_create_event(&mut self, event: FileSystemEvent) -> Vec<FileSystemEvent> {
		let pending = PendingEvent::new(event.clone())
			.with_inode(MetadataExtractor::get_inode(&event.path).await)
			.with_content_hash(
				MetadataExtractor::get_content_hash(
					&event.path,
					self.config.content_hash_max_file_size,
				)
				.await,
			)
			.with_windows_id(MetadataExtractor::get_windows_id(&event.path).await);
		// Check if this creation matches a recent removal
		if let Some(matching_remove) =
			MoveMatching::find_matching_remove(&pending, &self.pending_events, &self.config).await
		{
			let confidence =
				MoveMatching::calculate_confidence(&matching_remove, &pending, &self.config);
			let detection_method =
				MoveMatching::determine_detection_method(&matching_remove, &pending);

			let event_path = event.path.clone(); // Clone path before moving event

			let move_event = MoveEvent {
				source_path: matching_remove.event.path.clone(),
				destination_path: event_path.clone(),
				confidence,
				detection_method,
			};

			self.stats.record_move_detected(confidence);

			let move_event_fs = event.with_move_data(move_event);

			debug!(
				"Detected move: {:?} -> {:?} (confidence: {:.2})",
				matching_remove.event.path, event_path, confidence
			);
			return vec![move_event_fs];
		}

		// Store this creation as pending
		if self.pending_events.count_creates() < self.config.max_pending_events {
			self.pending_events.add_create(pending);
		} else {
			warn!(
				"Too many pending create events, dropping event for: {:?}",
				event.path
			);
		}

		vec![event]
	}

	async fn handle_rename_from_event(&mut self, event: FileSystemEvent) -> Vec<FileSystemEvent> {
		// Store the rename "from" event temporarily
		self.pending_events.pending_rename_from = Some((event.clone(), Instant::now()));

		// Don't emit anything yet - wait for the "to" event
		vec![]
	}
	async fn handle_rename_to_event(&mut self, event: FileSystemEvent) -> Vec<FileSystemEvent> {
		// Check if we have a matching "from" event
		if let Some((from_event, _timestamp)) = self.pending_events.pending_rename_from.take() {
			let event_path = event.path.clone(); // Clone path before moving event

			// Create a move event from the rename pair
			let move_event = MoveEvent {
				source_path: from_event.path.clone(),
				destination_path: event_path.clone(),
				confidence: 1.0, // Rename events are definitive
				detection_method: crate::events::MoveDetectionMethod::Rename,
			};

			self.stats.record_move_detected(1.0);

			let move_event_fs = event.with_move_data(move_event);
			debug!("Detected rename: {:?} -> {:?}", from_event.path, event_path);
			vec![move_event_fs]
		} else {
			// No matching "from" event - treat as regular create
			warn!(
				"Received rename 'to' event without matching 'from' event: {:?}",
				event.path
			);
			return self.handle_create_event(event).await;
		}
	}

	/// Clean up expired pending events and old metadata
	async fn cleanup_expired_events(&mut self) {
		let now = Instant::now();
		let timeout = self.config.timeout;

		// Clean up expired remove events
		self.pending_events.removes_by_size.retain(|_, events| {
			events.retain(|event| now.duration_since(event.timestamp) <= timeout);
			!events.is_empty()
		});

		self.pending_events
			.removes_no_size
			.retain(|event| now.duration_since(event.timestamp) <= timeout);

		self.pending_events
			.removes_by_inode
			.retain(|_, event| now.duration_since(event.timestamp) <= timeout);

		self.pending_events
			.removes_by_windows_id
			.retain(|_, event| now.duration_since(event.timestamp) <= timeout);

		// Clean up expired create events
		self.pending_events.creates_by_size.retain(|_, events| {
			events.retain(|event| now.duration_since(event.timestamp) <= timeout);
			!events.is_empty()
		});

		self.pending_events
			.creates_no_size
			.retain(|event| now.duration_since(event.timestamp) <= timeout);

		self.pending_events
			.creates_by_inode
			.retain(|_, event| now.duration_since(event.timestamp) <= timeout);

		self.pending_events
			.creates_by_windows_id
			.retain(|_, event| now.duration_since(event.timestamp) <= timeout);

		// Clean up old rename from event
		if let Some((_, timestamp)) = &self.pending_events.pending_rename_from {
			if now.duration_since(*timestamp) > timeout {
				self.pending_events.pending_rename_from = None;
			}
		}

		// Clean up old metadata cache entries
		self.metadata_cache.cleanup_old_entries(timeout * 2); // Keep metadata longer than events
	}
}
