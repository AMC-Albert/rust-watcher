use crate::database::storage::filesystem_cache::trait_def::FilesystemCacheStorage;
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

pub struct MoveDetector<'a> {
	/// Event storage organized for efficient lookups
	pending_events: PendingEventsStorage,

	/// Cache metadata for files we've seen (for use when they get removed)
	metadata_cache: MetadataCache,

	/// Reference to persistent filesystem cache
	cache: &'a mut dyn FilesystemCacheStorage,

	/// Configuration for move detection
	config: MoveDetectorConfig,

	/// Resource usage statistics
	stats: ResourceStats,
}

impl<'a> MoveDetector<'a> {
	pub fn new(config: MoveDetectorConfig, cache: &'a mut dyn FilesystemCacheStorage) -> Self {
		Self {
			pending_events: PendingEventsStorage::new(),
			metadata_cache: MetadataCache::new(),
			cache,
			config,
			stats: ResourceStats::new(),
		}
	}

	/// Create a new MoveDetector with default configuration and custom timeout
	pub fn with_timeout(timeout_ms: u64, cache: &'a mut dyn FilesystemCacheStorage) -> Self {
		let config = MoveDetectorConfig::with_timeout(timeout_ms);
		Self::new(config, cache)
	}
	/// Process a filesystem event and potentially detect moves
	pub async fn process_event(&mut self, event: FileSystemEvent) -> Vec<FileSystemEvent> {
		debug!(
			"Processing event: type={:?}, path={:?}, is_dir={}, size={:?}",
			event.event_type, event.path, event.is_directory, event.size
		);

		self.stats.record_event_processed();

		// Cache metadata for files we can still access (not for remove events)
		if !matches!(event.event_type, EventType::Remove | EventType::RenameFrom) {
			self.cache_file_metadata(&event.path).await;
		}

		// Log pending events state before processing
		let summary = self.get_pending_events_summary();
		debug!("Pending events state: removes={} (size: {}, no_size: {}, inode: {}, win_id: {}), creates={} (size: {}, no_size: {}, inode: {}, win_id: {}), has_rename_from={}",
			summary.total_removes(),
			summary.removes_by_size_buckets,
			summary.removes_no_size,
			summary.removes_by_inode,
			summary.removes_by_windows_id,
			summary.total_creates(),
			summary.creates_by_size_buckets,
			summary.creates_no_size,
			summary.creates_by_inode,
			summary.creates_by_windows_id,
			summary.has_pending_rename_from
		);

		self.cleanup_expired_events().await;

		let result = match event.event_type {
			EventType::Remove => {
				debug!("Handling Remove event for: {:?}", event.path);
				self.handle_remove_event(event).await
			}
			EventType::Create => {
				debug!("Handling Create event for: {:?}", event.path);
				self.handle_create_event(event).await
			}
			EventType::RenameFrom => {
				debug!("Handling RenameFrom event for: {:?}", event.path);
				self.handle_rename_from_event(event).await
			}
			EventType::RenameTo => {
				debug!("Handling RenameTo event for: {:?}", event.path);
				self.handle_rename_to_event(event).await
			}
			EventType::Rename => {
				// Generic rename event - treat as both remove and create
				debug!("Processing generic rename event for: {:?}", event.path);
				vec![event] // Pass through for now
			}
			_ => {
				debug!(
					"Passing through event: type={:?}, path={:?}",
					event.event_type, event.path
				);
				vec![event] // Pass through other events
			}
		};

		if result.len() > 1 {
			debug!("Returning {} events from processing", result.len());
		}

		result
	}

	/// Infer whether a removed path was likely a directory based on available context
	pub fn infer_path_type(&self, path: &Path) -> Option<bool> {
		PathTypeInference::infer_path_type(path, &self.metadata_cache, &self.pending_events)
	}

	/// Get better heuristics for directory detection on removed paths
	pub fn get_path_type_heuristics(
		&self, path: &Path,
	) -> crate::move_detection::heuristics::PathTypeHeuristics {
		PathTypeInference::get_path_type_heuristics(
			path,
			&self.metadata_cache,
			&self.pending_events,
		)
	}

	/// Get resource usage statistics
	pub fn get_resource_stats(&mut self) -> ResourceStats {
		self.stats.update(&self.pending_events, &self.metadata_cache);
		self.stats.clone()
	}

	/// Get summary of pending events for debugging
	pub fn get_pending_events_summary(&self) -> PendingEventsSummary {
		PendingEventsSummary::from_storage(&self.pending_events)
	}

	/// Cache metadata for a file path
	async fn cache_file_metadata(&mut self, path: &Path) {
		if let Ok(metadata) = std::fs::metadata(path) {
			let size = if metadata.is_file() { Some(metadata.len()) } else { None };
			let windows_id = MetadataExtractor::get_windows_id(path).await;

			let file_metadata = FileMetadata::new(size, windows_id);
			self.metadata_cache.insert(path.to_path_buf(), file_metadata);
		}
	}
	async fn handle_remove_event(&mut self, mut event: FileSystemEvent) -> Vec<FileSystemEvent> {
		// Try to get cached metadata for this file (since it's being removed)
		let mut cached_metadata = self.metadata_cache.remove(&event.path);
		if cached_metadata.is_none() {
			// Fallback: query persistent cache for metadata
			if let Ok(Some(node)) = self.cache.get_unified_node(&event.path).await {
				let (size, windows_id) = match &node.node_type {
					crate::database::types::NodeType::File { size, .. } => {
						(Some(*size), node.metadata.windows_id)
					}
					_ => (None, node.metadata.windows_id),
				};
				cached_metadata = Some(FileMetadata::new(size, windows_id));
			}
		}
		debug!(
			"Remove event: cached_metadata available={}",
			cached_metadata.is_some()
		);

		// Update event with cached metadata if available
		if let Some(metadata) = &cached_metadata {
			if event.size.is_none() {
				event.size = metadata.size;
				debug!("Updated event size from cache: {:?}", event.size);
			}
		}

		let inode = MetadataExtractor::get_inode(&event.path).await;
		let windows_id = cached_metadata.as_ref().and_then(|m| m.windows_id);

		debug!(
			"Remove event metadata: inode={:?}, windows_id={:?}",
			inode, windows_id
		);

		let pending =
			PendingEvent::new(event.clone()).with_inode(inode).with_windows_id(windows_id);

		// Check if this removal matches a recent create (reverse move detection)
		debug!("Searching for matching create event...");
		if let Some(matching_create) =
			MoveMatching::find_matching_create(&pending, &self.pending_events, &self.config).await
		{
			debug!(
				"Found matching create event: {:?}",
				matching_create.event.path
			);

			let confidence =
				MoveMatching::calculate_confidence(&pending, &matching_create, &self.config);
			let detection_method =
				MoveMatching::determine_detection_method(&pending, &matching_create);

			debug!(
				"Move confidence calculated: {:.2}, method: {:?}",
				confidence, detection_method
			);

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
			self.pending_events.remove_create_by_id(matching_create.event.id);

			debug!(
				"Detected move: {:?} -> {:?} (confidence: {:.2})",
				matching_create.event.path, event.path, confidence
			);
			return vec![move_event_fs];
		} else {
			debug!("No matching create event found");
		} // Store this removal as pending
		if self.pending_events.count_removes() < self.config.max_pending_events {
			self.pending_events.add_remove(pending);
			debug!(
				"Added remove event to pending storage (total removes: {})",
				self.pending_events.count_removes()
			);
		} else {
			warn!(
				"Too many pending remove events, dropping event for: {:?}",
				event.path
			);
		}

		vec![event]
	}
	async fn handle_create_event(&mut self, event: FileSystemEvent) -> Vec<FileSystemEvent> {
		let inode = MetadataExtractor::get_inode(&event.path).await;
		let content_hash = MetadataExtractor::get_content_hash(
			&event.path,
			self.config.content_hash_max_file_size,
		)
		.await;
		let windows_id = MetadataExtractor::get_windows_id(&event.path).await;
		debug!(
			"Create event metadata: inode={:?}, content_hash={:?}, windows_id={:?}",
			inode,
			content_hash.as_ref().map(|h| h.to_string()),
			windows_id
		);

		let pending = PendingEvent::new(event.clone())
			.with_inode(inode)
			.with_content_hash(content_hash)
			.with_windows_id(windows_id);

		// Check if this creation matches a recent removal
		debug!("Searching for matching remove event...");
		if let Some(matching_remove) =
			MoveMatching::find_matching_remove(&pending, &self.pending_events, &self.config).await
		{
			debug!(
				"Found matching remove event: {:?}",
				matching_remove.event.path
			);

			let confidence =
				MoveMatching::calculate_confidence(&matching_remove, &pending, &self.config);
			let detection_method =
				MoveMatching::determine_detection_method(&matching_remove, &pending);

			debug!(
				"Move confidence calculated: {:.2}, method: {:?}",
				confidence, detection_method
			);

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
		} else {
			debug!("No matching remove event found");
		}

		// Store this creation as pending
		if self.pending_events.count_creates() < self.config.max_pending_events {
			self.pending_events.add_create(pending);
			debug!(
				"Added create event to pending storage (total creates: {})",
				self.pending_events.count_creates()
			);
		} else {
			warn!(
				"Too many pending create events, dropping event for: {:?}",
				event.path
			);
		}

		vec![event]
	}
	async fn handle_rename_from_event(&mut self, event: FileSystemEvent) -> Vec<FileSystemEvent> {
		debug!(
			"Storing RenameFrom event for later pairing: {:?}",
			event.path
		);
		// Store the rename "from" event temporarily
		self.pending_events.pending_rename_from = Some((event.clone(), Instant::now()));

		// Don't emit anything yet - wait for the "to" event
		vec![]
	}

	async fn handle_rename_to_event(&mut self, event: FileSystemEvent) -> Vec<FileSystemEvent> {
		// Check if we have a matching "from" event
		if let Some((from_event, _timestamp)) = self.pending_events.pending_rename_from.take() {
			debug!(
				"Found matching RenameFrom event: {:?} -> {:?}",
				from_event.path, event.path
			);

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
			debug!(
				"Detected rename: {:?} -> {:?} (confidence: 1.0)",
				from_event.path, event_path
			);
			vec![move_event_fs]
		} else {
			debug!("No matching RenameFrom event found, treating as create");
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

		// Count events before cleanup for logging
		let initial_removes = self.pending_events.count_removes();
		let initial_creates = self.pending_events.count_creates();

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
		let had_rename_from = self.pending_events.pending_rename_from.is_some();
		if let Some((_, timestamp)) = &self.pending_events.pending_rename_from {
			if now.duration_since(*timestamp) > timeout {
				debug!("Cleaning up expired RenameFrom event");
				self.pending_events.pending_rename_from = None;
			}
		}

		// Count events after cleanup and log if any were removed
		let final_removes = self.pending_events.count_removes();
		let final_creates = self.pending_events.count_creates();

		if initial_removes != final_removes
			|| initial_creates != final_creates
			|| had_rename_from && self.pending_events.pending_rename_from.is_none()
		{
			debug!(
				"Cleanup completed: removes {} -> {}, creates {} -> {}",
				initial_removes, final_removes, initial_creates, final_creates
			);
		}

		// Clean up old metadata cache entries
		self.metadata_cache.cleanup_old_entries(timeout * 2); // Keep metadata longer than events
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::database::storage::filesystem_cache::trait_def::CacheStats;
	use std::path::PathBuf;
	use std::time::Duration;

	#[test]
	fn test_move_detector_creation() {
		let config = MoveDetectorConfig::default();
		struct DummyCache;
		#[async_trait::async_trait]
		impl FilesystemCacheStorage for DummyCache {
			async fn store_filesystem_node(
				&mut self, _: &uuid::Uuid, _: &crate::database::types::FilesystemNode,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn get_filesystem_node(
				&mut self, _: &uuid::Uuid, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<
				Option<crate::database::types::FilesystemNode>,
			> {
				Ok(None)
			}
			async fn list_directory_for_watch(
				&mut self, _: &uuid::Uuid, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn store_watch_metadata(
				&mut self, _: &crate::database::types::WatchMetadata,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn get_watch_metadata(
				&mut self, _: &uuid::Uuid,
			) -> crate::database::error::DatabaseResult<Option<crate::database::types::WatchMetadata>>
			{
				Ok(None)
			}
			async fn remove_watch(
				&mut self, _: &uuid::Uuid,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn store_shared_node(
				&mut self, _: &crate::database::types::SharedNodeInfo,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn get_shared_node(
				&mut self, _: u64,
			) -> crate::database::error::DatabaseResult<
				Option<crate::database::types::SharedNodeInfo>,
			> {
				Ok(None)
			}
			async fn batch_store_filesystem_nodes(
				&mut self, _: &uuid::Uuid, _: &[crate::database::types::FilesystemNode],
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn find_nodes_by_prefix(
				&mut self, _: &uuid::Uuid, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn get_cache_stats(
				&mut self, _: &uuid::Uuid,
			) -> crate::database::error::DatabaseResult<CacheStats> {
				Ok(Default::default())
			}
			async fn cleanup_stale_cache(
				&mut self, _: &uuid::Uuid, _: u64,
			) -> crate::database::error::DatabaseResult<usize> {
				Ok(0)
			}
			async fn list_directory_unified(
				&mut self, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn get_unified_node(
				&mut self, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<
				Option<crate::database::types::FilesystemNode>,
			> {
				Ok(None)
			}
			async fn list_ancestors(
				&mut self, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn list_descendants(
				&mut self, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn search_nodes(
				&mut self, _: &str,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn remove_filesystem_node(
				&mut self, _: &uuid::Uuid, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn rename_filesystem_node(
				&mut self, _: &uuid::Uuid, _: &std::path::Path, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
		}
		let mut dummy_cache = DummyCache;
		let detector = MoveDetector::new(config, &mut dummy_cache);

		// Test that detector is properly initialized
		assert!(detector.pending_events.pending_rename_from.is_none());
		assert!(detector.pending_events.removes_by_size.is_empty());
		assert!(detector.pending_events.removes_no_size.is_empty());
		assert!(detector.pending_events.creates_by_size.is_empty());
		assert!(detector.pending_events.creates_no_size.is_empty());
	}

	#[test]
	fn test_move_detector_with_timeout() {
		struct DummyCache;
		#[async_trait::async_trait]
		impl FilesystemCacheStorage for DummyCache {
			async fn store_filesystem_node(
				&mut self, _: &uuid::Uuid, _: &crate::database::types::FilesystemNode,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn get_filesystem_node(
				&mut self, _: &uuid::Uuid, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<
				Option<crate::database::types::FilesystemNode>,
			> {
				Ok(None)
			}
			async fn list_directory_for_watch(
				&mut self, _: &uuid::Uuid, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn store_watch_metadata(
				&mut self, _: &crate::database::types::WatchMetadata,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn get_watch_metadata(
				&mut self, _: &uuid::Uuid,
			) -> crate::database::error::DatabaseResult<Option<crate::database::types::WatchMetadata>>
			{
				Ok(None)
			}
			async fn remove_watch(
				&mut self, _: &uuid::Uuid,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn store_shared_node(
				&mut self, _: &crate::database::types::SharedNodeInfo,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn get_shared_node(
				&mut self, _: u64,
			) -> crate::database::error::DatabaseResult<
				Option<crate::database::types::SharedNodeInfo>,
			> {
				Ok(None)
			}
			async fn batch_store_filesystem_nodes(
				&mut self, _: &uuid::Uuid, _: &[crate::database::types::FilesystemNode],
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn find_nodes_by_prefix(
				&mut self, _: &uuid::Uuid, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn get_cache_stats(
				&mut self, _: &uuid::Uuid,
			) -> crate::database::error::DatabaseResult<CacheStats> {
				Ok(Default::default())
			}
			async fn cleanup_stale_cache(
				&mut self, _: &uuid::Uuid, _: u64,
			) -> crate::database::error::DatabaseResult<usize> {
				Ok(0)
			}
			async fn list_directory_unified(
				&mut self, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn get_unified_node(
				&mut self, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<
				Option<crate::database::types::FilesystemNode>,
			> {
				Ok(None)
			}
			async fn list_ancestors(
				&mut self, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn list_descendants(
				&mut self, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn search_nodes(
				&mut self, _: &str,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn remove_filesystem_node(
				&mut self, _: &uuid::Uuid, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn rename_filesystem_node(
				&mut self, _: &uuid::Uuid, _: &std::path::Path, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
		}
		let mut dummy_cache = DummyCache;
		let detector = MoveDetector::with_timeout(5000, &mut dummy_cache);

		// Test that detector is created with custom timeout
		assert_eq!(detector.config.timeout, Duration::from_millis(5000));
	}
	#[test]
	fn test_infer_path_type() {
		let config = MoveDetectorConfig::default();
		struct DummyCache;
		#[async_trait::async_trait]
		impl FilesystemCacheStorage for DummyCache {
			async fn store_filesystem_node(
				&mut self, _: &uuid::Uuid, _: &crate::database::types::FilesystemNode,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn get_filesystem_node(
				&mut self, _: &uuid::Uuid, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<
				Option<crate::database::types::FilesystemNode>,
			> {
				Ok(None)
			}
			async fn list_directory_for_watch(
				&mut self, _: &uuid::Uuid, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn store_watch_metadata(
				&mut self, _: &crate::database::types::WatchMetadata,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn get_watch_metadata(
				&mut self, _: &uuid::Uuid,
			) -> crate::database::error::DatabaseResult<Option<crate::database::types::WatchMetadata>>
			{
				Ok(None)
			}
			async fn remove_watch(
				&mut self, _: &uuid::Uuid,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn store_shared_node(
				&mut self, _: &crate::database::types::SharedNodeInfo,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn get_shared_node(
				&mut self, _: u64,
			) -> crate::database::error::DatabaseResult<
				Option<crate::database::types::SharedNodeInfo>,
			> {
				Ok(None)
			}
			async fn batch_store_filesystem_nodes(
				&mut self, _: &uuid::Uuid, _: &[crate::database::types::FilesystemNode],
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn find_nodes_by_prefix(
				&mut self, _: &uuid::Uuid, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn get_cache_stats(
				&mut self, _: &uuid::Uuid,
			) -> crate::database::error::DatabaseResult<CacheStats> {
				Ok(Default::default())
			}
			async fn cleanup_stale_cache(
				&mut self, _: &uuid::Uuid, _: u64,
			) -> crate::database::error::DatabaseResult<usize> {
				Ok(0)
			}
			async fn list_directory_unified(
				&mut self, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn get_unified_node(
				&mut self, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<
				Option<crate::database::types::FilesystemNode>,
			> {
				Ok(None)
			}
			async fn list_ancestors(
				&mut self, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn list_descendants(
				&mut self, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn search_nodes(
				&mut self, _: &str,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn remove_filesystem_node(
				&mut self, _: &uuid::Uuid, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn rename_filesystem_node(
				&mut self, _: &uuid::Uuid, _: &std::path::Path, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
		}
		let mut dummy_cache = DummyCache;
		let detector = MoveDetector::new(config, &mut dummy_cache);

		// Test path type inference - since there's no metadata or pending events,
		// the function will fall back to basic heuristics
		// Files with extensions should be detected as files
		let file_result = detector.infer_path_type(&PathBuf::from("file.txt"));
		if let Some(is_dir) = file_result {
			assert!(!is_dir);
		}

		// Paths without extensions might not be determinable without context
		let folder_result = detector.infer_path_type(&PathBuf::from("folder"));
		// This might return None if there's insufficient information
		println!("Folder inference result: {folder_result:?}");
	}
	#[test]
	fn test_get_pending_events_summary() {
		let config = MoveDetectorConfig::default();
		struct DummyCache;
		#[async_trait::async_trait]
		impl FilesystemCacheStorage for DummyCache {
			async fn store_filesystem_node(
				&mut self, _: &uuid::Uuid, _: &crate::database::types::FilesystemNode,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn get_filesystem_node(
				&mut self, _: &uuid::Uuid, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<
				Option<crate::database::types::FilesystemNode>,
			> {
				Ok(None)
			}
			async fn list_directory_for_watch(
				&mut self, _: &uuid::Uuid, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn store_watch_metadata(
				&mut self, _: &crate::database::types::WatchMetadata,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn get_watch_metadata(
				&mut self, _: &uuid::Uuid,
			) -> crate::database::error::DatabaseResult<Option<crate::database::types::WatchMetadata>>
			{
				Ok(None)
			}
			async fn remove_watch(
				&mut self, _: &uuid::Uuid,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn store_shared_node(
				&mut self, _: &crate::database::types::SharedNodeInfo,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn get_shared_node(
				&mut self, _: u64,
			) -> crate::database::error::DatabaseResult<
				Option<crate::database::types::SharedNodeInfo>,
			> {
				Ok(None)
			}
			async fn batch_store_filesystem_nodes(
				&mut self, _: &uuid::Uuid, _: &[crate::database::types::FilesystemNode],
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn find_nodes_by_prefix(
				&mut self, _: &uuid::Uuid, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn get_cache_stats(
				&mut self, _: &uuid::Uuid,
			) -> crate::database::error::DatabaseResult<CacheStats> {
				Ok(Default::default())
			}
			async fn cleanup_stale_cache(
				&mut self, _: &uuid::Uuid, _: u64,
			) -> crate::database::error::DatabaseResult<usize> {
				Ok(0)
			}
			async fn list_directory_unified(
				&mut self, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn get_unified_node(
				&mut self, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<
				Option<crate::database::types::FilesystemNode>,
			> {
				Ok(None)
			}
			async fn list_ancestors(
				&mut self, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn list_descendants(
				&mut self, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn search_nodes(
				&mut self, _: &str,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn remove_filesystem_node(
				&mut self, _: &uuid::Uuid, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn rename_filesystem_node(
				&mut self, _: &uuid::Uuid, _: &std::path::Path, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
		}
		let mut dummy_cache = DummyCache;
		let detector = MoveDetector::new(config, &mut dummy_cache);

		let summary = detector.get_pending_events_summary();
		assert_eq!(summary.removes_by_size_buckets, 0);
		assert_eq!(summary.removes_no_size, 0);
		assert!(!summary.has_pending_rename_from);
	}

	#[test]
	fn test_get_resource_stats() {
		let config = MoveDetectorConfig::default();
		struct DummyCache;
		#[async_trait::async_trait]
		impl FilesystemCacheStorage for DummyCache {
			async fn store_filesystem_node(
				&mut self, _: &uuid::Uuid, _: &crate::database::types::FilesystemNode,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn get_filesystem_node(
				&mut self, _: &uuid::Uuid, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<
				Option<crate::database::types::FilesystemNode>,
			> {
				Ok(None)
			}
			async fn list_directory_for_watch(
				&mut self, _: &uuid::Uuid, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn store_watch_metadata(
				&mut self, _: &crate::database::types::WatchMetadata,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn get_watch_metadata(
				&mut self, _: &uuid::Uuid,
			) -> crate::database::error::DatabaseResult<Option<crate::database::types::WatchMetadata>>
			{
				Ok(None)
			}
			async fn remove_watch(
				&mut self, _: &uuid::Uuid,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn store_shared_node(
				&mut self, _: &crate::database::types::SharedNodeInfo,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn get_shared_node(
				&mut self, _: u64,
			) -> crate::database::error::DatabaseResult<
				Option<crate::database::types::SharedNodeInfo>,
			> {
				Ok(None)
			}
			async fn batch_store_filesystem_nodes(
				&mut self, _: &uuid::Uuid, _: &[crate::database::types::FilesystemNode],
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn find_nodes_by_prefix(
				&mut self, _: &uuid::Uuid, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn get_cache_stats(
				&mut self, _: &uuid::Uuid,
			) -> crate::database::error::DatabaseResult<CacheStats> {
				Ok(Default::default())
			}
			async fn cleanup_stale_cache(
				&mut self, _: &uuid::Uuid, _: u64,
			) -> crate::database::error::DatabaseResult<usize> {
				Ok(0)
			}
			async fn list_directory_unified(
				&mut self, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn get_unified_node(
				&mut self, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<
				Option<crate::database::types::FilesystemNode>,
			> {
				Ok(None)
			}
			async fn list_ancestors(
				&mut self, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn list_descendants(
				&mut self, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn search_nodes(
				&mut self, _: &str,
			) -> crate::database::error::DatabaseResult<Vec<crate::database::types::FilesystemNode>>
			{
				Ok(vec![])
			}
			async fn remove_filesystem_node(
				&mut self, _: &uuid::Uuid, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
			async fn rename_filesystem_node(
				&mut self, _: &uuid::Uuid, _: &std::path::Path, _: &std::path::Path,
			) -> crate::database::error::DatabaseResult<()> {
				Ok(())
			}
		}
		let mut dummy_cache = DummyCache;
		let mut detector = MoveDetector::new(config, &mut dummy_cache);

		// Get resource stats should not panic on empty detector
		let stats = detector.get_resource_stats();
		assert_eq!(stats.total_events_processed, 0);
	}
}
