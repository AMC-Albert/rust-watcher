use crate::events::{EventType, FileSystemEvent, MoveDetectionMethod, MoveEvent};
use std::collections::HashMap;
use std::fs::File;
use std::hash::Hasher;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::time::Instant;
use tracing::{debug, warn};
use twox_hash::XxHash64;

/// Configuration for the move detector
#[derive(Debug, Clone)]
pub struct MoveDetectorConfig {
	/// Timeout for matching remove/create events
	pub timeout: Duration,
	/// Confidence threshold for considering a match valid (0.0 to 1.0)
	pub confidence_threshold: f32,
	/// Weight for size matching in confidence calculation
	pub weight_size_match: f32,
	/// Weight for time factor in confidence calculation
	pub weight_time_factor: f32,
	/// Weight for inode matching in confidence calculation (Unix only)
	pub weight_inode_match: f32,
	/// Weight for content hash matching in confidence calculation
	pub weight_content_hash: f32,
	/// Weight for name similarity in confidence calculation
	pub weight_name_similarity: f32,
	/// Maximum number of pending events to prevent memory leaks
	pub max_pending_events: usize,
	/// Maximum file size for content hashing (bytes)
	pub content_hash_max_file_size: u64,
}

impl Default for MoveDetectorConfig {
	fn default() -> Self {
		// Adjust weights and threshold based on platform capabilities
		#[cfg(unix)]
		let (confidence_threshold, weight_inode, weight_size, weight_name, weight_time) =
			(0.7, 0.4, 0.2, 0.1, 0.15);

		#[cfg(windows)]
		let (confidence_threshold, weight_inode, weight_size, weight_name, weight_time) =
			(0.5, 0.3, 0.25, 0.2, 0.2); // Lower threshold, higher name/time weights

		#[cfg(not(any(unix, windows)))]
		let (confidence_threshold, weight_inode, weight_size, weight_name, weight_time) =
			(0.5, 0.3, 0.25, 0.2, 0.2);

		Self {
			timeout: Duration::from_millis(1000),
			confidence_threshold,
			weight_size_match: weight_size,
			weight_time_factor: weight_time,
			weight_inode_match: weight_inode,
			weight_content_hash: 0.35,
			weight_name_similarity: weight_name,
			max_pending_events: 1000,
			content_hash_max_file_size: 1_048_576, // 1MB
		}
	}
}

#[derive(Debug, Clone)]
struct PendingEvent {
	event: FileSystemEvent,
	timestamp: Instant,
	inode: Option<u64>,
	content_hash: Option<String>,
	// Windows-specific identifier (creation_time_nanos << 32 | size_lower_32_bits)
	windows_id: Option<u64>,
}

#[derive(Debug, Clone)]
struct FileMetadata {
	size: Option<u64>,
	windows_id: Option<u64>,
	last_seen: Instant,
}

pub struct MoveDetector {
	// Bucketed pending removes for O(1) size-based lookups
	pending_removes_by_size: HashMap<u64, Vec<PendingEvent>>,
	pending_removes_no_size: Vec<PendingEvent>,
	pending_removes_by_inode: HashMap<u64, PendingEvent>, // Unix only
	pending_removes_by_windows_id: HashMap<u64, PendingEvent>, // Windows only

	// Bucketed pending creates for O(1) size-based lookups
	pending_creates_by_size: HashMap<u64, Vec<PendingEvent>>,
	pending_creates_no_size: Vec<PendingEvent>,
	pending_creates_by_inode: HashMap<u64, PendingEvent>, // Unix only
	pending_creates_by_windows_id: HashMap<u64, PendingEvent>, // Windows only

	// Cache metadata for files we've seen (for use when they get removed)
	metadata_cache: HashMap<PathBuf, FileMetadata>,

	// Rename event pairing (Windows sends Name(From) then Name(To))
	pending_rename_from: Option<(FileSystemEvent, Instant)>,

	config: MoveDetectorConfig,
}

impl MoveDetector {
	pub fn new(config: MoveDetectorConfig) -> Self {
		Self {
			pending_removes_by_size: HashMap::new(),
			pending_removes_no_size: Vec::new(),
			pending_removes_by_inode: HashMap::new(),
			pending_removes_by_windows_id: HashMap::new(),
			pending_creates_by_size: HashMap::new(),
			pending_creates_no_size: Vec::new(),
			pending_creates_by_inode: HashMap::new(),
			pending_creates_by_windows_id: HashMap::new(),
			metadata_cache: HashMap::new(),
			pending_rename_from: None,
			config,
		}
	}
	/// Create a new MoveDetector with default configuration and custom timeout
	pub fn with_timeout(timeout_ms: u64) -> Self {
		let config = MoveDetectorConfig {
			timeout: Duration::from_millis(timeout_ms),
			..Default::default()
		};
		Self::new(config)
	}
	/// Process a filesystem event and potentially detect moves
	pub async fn process_event(&mut self, event: FileSystemEvent) -> Vec<FileSystemEvent> {
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

	async fn handle_rename_from_event(&mut self, event: FileSystemEvent) -> Vec<FileSystemEvent> {
		debug!("Processing rename FROM event for: {:?}", event.path);

		// Store this as a pending rename-from event
		self.pending_rename_from = Some((event.clone(), Instant::now()));

		// Also cache metadata before the file is renamed
		self.cache_file_metadata(&event.path).await;

		// Don't emit any events yet, wait for the RenameTo
		vec![]
	}

	async fn handle_rename_to_event(&mut self, event: FileSystemEvent) -> Vec<FileSystemEvent> {
		debug!("Processing rename TO event for: {:?}", event.path);

		// Check if we have a pending rename-from event
		if let Some((from_event, timestamp)) = self.pending_rename_from.take() {
			// Check if the rename pair is within timeout
			if timestamp.elapsed() <= Duration::from_millis(100) {
				// Short timeout for rename pairs
				debug!(
					"Pairing rename events: {:?} -> {:?}",
					from_event.path, event.path
				);

				// Create a move event from the rename pair
				let move_event = MoveEvent {
					source_path: from_event.path.clone(),
					destination_path: event.path.clone(),
					confidence: 1.0, // High confidence for paired rename events
					detection_method: MoveDetectionMethod::NameAndTiming,
				};

				let mut move_event_fs = event.clone();
				move_event_fs = move_event_fs.with_move_data(move_event);

				debug!("Detected rename: {:?} -> {:?}", from_event.path, event.path);
				return vec![move_event_fs];
			} else {
				debug!("Rename FROM event expired, treating TO event as standalone");
			}
		} else {
			debug!("No pending rename FROM event, treating TO event as standalone");
		}

		// No matching rename-from event, treat as a regular create
		vec![event]
	}

	async fn cache_file_metadata(&mut self, path: &Path) {
		if let Ok(metadata) = tokio::fs::metadata(path).await {
			let windows_id = self.get_windows_id(path).await;
			let file_metadata = FileMetadata {
				size: if metadata.is_file() {
					Some(metadata.len())
				} else {
					None
				},
				windows_id,
				last_seen: Instant::now(),
			};
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

		let pending = PendingEvent {
			timestamp: Instant::now(),
			inode: self.get_inode(&event.path).await,
			content_hash: None, // File is being removed, can't hash
			windows_id: cached_metadata.as_ref().and_then(|m| m.windows_id),
			event: event.clone(),
		};

		// Check if this removal matches a recent create (reverse move detection)
		if let Some(matching_create) = self.find_matching_create(&pending).await {
			let move_event = MoveEvent {
				source_path: matching_create.event.path.clone(),
				destination_path: event.path.clone(),
				confidence: self.calculate_confidence(&matching_create, &pending),
				detection_method: self.determine_detection_method(&matching_create, &pending),
			};

			let mut move_event_fs = matching_create.event.clone();
			move_event_fs = move_event_fs.with_move_data(move_event);
			// Remove the matching create from pending
			self.remove_pending_create_by_id(matching_create.event.id);

			debug!(
				"Detected move: {:?} -> {:?}",
				matching_create.event.path, event.path
			);
			return vec![move_event_fs];
		}

		// Store this removal as pending
		if self.count_pending_removes() < self.config.max_pending_events {
			self.add_pending_remove(pending);
		} else {
			warn!(
				"Too many pending remove events, dropping event for: {:?}",
				event.path
			);
		}

		vec![event]
	}
	async fn handle_create_event(&mut self, event: FileSystemEvent) -> Vec<FileSystemEvent> {
		let pending = PendingEvent {
			timestamp: Instant::now(),
			inode: self.get_inode(&event.path).await,
			content_hash: self.get_content_hash(&event.path).await,
			windows_id: self.get_windows_id(&event.path).await,
			event: event.clone(),
		}; // Check if this creation matches a recent removal
		if let Some(matching_remove) = self.find_matching_remove(&pending).await {
			let move_event = MoveEvent {
				source_path: matching_remove.event.path.clone(),
				destination_path: event.path.clone(),
				confidence: self.calculate_confidence(&matching_remove, &pending),
				detection_method: self.determine_detection_method(&matching_remove, &pending),
			};

			debug!(
				"Detected move: {:?} -> {:?}",
				matching_remove.event.path, event.path
			);
			let move_event_fs = event.with_move_data(move_event);

			// Remove the matching removal from pending
			self.remove_pending_remove_by_id(matching_remove.event.id);
			return vec![move_event_fs];
		}

		// Store this creation as pending
		if self.count_pending_creates() < self.config.max_pending_events {
			self.add_pending_create(pending);
		} else {
			warn!(
				"Too many pending create events, dropping event for: {:?}",
				event.path
			);
		}

		vec![event]
	}
	async fn find_matching_remove(&self, create_event: &PendingEvent) -> Option<PendingEvent> {
		let mut best_match: Option<(PendingEvent, f32)> = None;

		debug!(
			"Finding matching remove for create event: path={:?}, size={:?}, inode={:?}, windows_id={:?}",
			create_event.event.path, create_event.event.size, create_event.inode, create_event.windows_id
		);

		// Fast path: inode matching (Unix only) - highest confidence
		if let Some(inode) = create_event.inode {
			debug!("Checking inode {} for matching remove", inode);
			if let Some(pending_remove) = self.pending_removes_by_inode.get(&inode) {
				if self.is_within_timeout(&pending_remove.timestamp) {
					debug!("Found inode match for inode {}", inode);
					return Some(pending_remove.clone());
				} else {
					debug!("Inode match {} found but expired", inode);
				}
			} else {
				debug!("No pending remove found for inode {}", inode);
			}
		}

		// Fast path: Windows ID matching - high confidence
		if let Some(windows_id) = create_event.windows_id {
			debug!("Checking Windows ID {} for matching remove", windows_id);
			if let Some(pending_remove) = self.pending_removes_by_windows_id.get(&windows_id) {
				if self.is_within_timeout(&pending_remove.timestamp) {
					debug!("Found Windows ID match for {}", windows_id);
					return Some(pending_remove.clone());
				} else {
					debug!("Windows ID match {} found but expired", windows_id);
				}
			} else {
				debug!("No pending remove found for Windows ID {}", windows_id);
			}
		}

		// Fast path: size-based matching
		if let Some(size) = create_event.event.size {
			debug!("Checking size {} for matching removes", size);
			if let Some(removes_with_size) = self.pending_removes_by_size.get(&size) {
				debug!(
					"Found {} pending removes with size {}",
					removes_with_size.len(),
					size
				);
				for pending_remove in removes_with_size {
					if self.is_within_timeout(&pending_remove.timestamp) {
						let confidence = self.calculate_confidence(pending_remove, create_event);
						debug!(
							"Confidence for remove {:?} -> create {:?}: {}",
							pending_remove.event.path, create_event.event.path, confidence
						);
						if confidence > self.config.confidence_threshold {
							match best_match {
								Some((_, best_confidence)) if confidence > best_confidence => {
									debug!("New best match with confidence {}", confidence);
									best_match = Some((pending_remove.clone(), confidence));
								}
								None => {
									debug!("First match with confidence {}", confidence);
									best_match = Some((pending_remove.clone(), confidence));
								}
								_ => {
									debug!("Match found but not better than current best");
								}
							}
						} else {
							debug!(
								"Confidence {} too low (< {})",
								confidence, self.config.confidence_threshold
							);
						}
					} else {
						debug!("Remove event for {:?} expired", pending_remove.event.path);
					}
				}
			} else {
				debug!("No pending removes found for size {}", size);
			}
		} // Fallback: check events without size
		debug!(
			"Checking {} events without size",
			self.pending_removes_no_size.len()
		);
		for pending_remove in &self.pending_removes_no_size {
			if self.is_within_timeout(&pending_remove.timestamp) {
				let confidence = self.calculate_confidence(pending_remove, create_event);
				debug!(
					"Confidence for remove {:?} -> create {:?}: {}",
					pending_remove.event.path, create_event.event.path, confidence
				);
				if confidence > self.config.confidence_threshold {
					match best_match {
						Some((_, best_confidence)) if confidence > best_confidence => {
							debug!("New best match with confidence {}", confidence);
							best_match = Some((pending_remove.clone(), confidence));
						}
						None => {
							debug!("First match with confidence {}", confidence);
							best_match = Some((pending_remove.clone(), confidence));
						}
						_ => {
							debug!("Match found but not better than current best");
						}
					}
				} else {
					debug!(
						"Confidence {} too low (< {})",
						confidence, self.config.confidence_threshold
					);
				}
			} else {
				debug!("Remove event for {:?} expired", pending_remove.event.path);
			}
		}

		match &best_match {
			Some((event, confidence)) => {
				debug!(
					"Best match found: {:?} with confidence {}",
					event.event.path, confidence
				);
			}
			None => {
				debug!(
					"No matching remove found for create {:?}",
					create_event.event.path
				);
			}
		}

		best_match.map(|(event, _)| event)
	}
	async fn find_matching_create(&self, remove_event: &PendingEvent) -> Option<PendingEvent> {
		let mut best_match: Option<(PendingEvent, f32)> = None;

		// Fast path: inode matching (Unix only) - highest confidence
		if let Some(inode) = remove_event.inode {
			if let Some(pending_create) = self.pending_creates_by_inode.get(&inode) {
				if self.is_within_timeout(&pending_create.timestamp) {
					return Some(pending_create.clone());
				}
			}
		}

		// Fast path: Windows ID matching - high confidence
		if let Some(windows_id) = remove_event.windows_id {
			if let Some(pending_create) = self.pending_creates_by_windows_id.get(&windows_id) {
				if self.is_within_timeout(&pending_create.timestamp) {
					return Some(pending_create.clone());
				}
			}
		}

		// Fast path: size-based matching
		if let Some(size) = remove_event.event.size {
			if let Some(creates_with_size) = self.pending_creates_by_size.get(&size) {
				for pending_create in creates_with_size {
					if self.is_within_timeout(&pending_create.timestamp) {
						let confidence = self.calculate_confidence(remove_event, pending_create);
						if confidence > self.config.confidence_threshold {
							match best_match {
								Some((_, best_confidence)) if confidence > best_confidence => {
									best_match = Some((pending_create.clone(), confidence));
								}
								None => {
									best_match = Some((pending_create.clone(), confidence));
								}
								_ => {}
							}
						}
					}
				}
			}
		}

		// Fallback: check events without size
		for pending_create in &self.pending_creates_no_size {
			if self.is_within_timeout(&pending_create.timestamp) {
				let confidence = self.calculate_confidence(remove_event, pending_create);
				if confidence > self.config.confidence_threshold {
					match best_match {
						Some((_, best_confidence)) if confidence > best_confidence => {
							best_match = Some((pending_create.clone(), confidence));
						}
						None => {
							best_match = Some((pending_create.clone(), confidence));
						}
						_ => {}
					}
				}
			}
		}

		best_match.map(|(event, _)| event)
	}
	fn calculate_confidence(&self, event1: &PendingEvent, event2: &PendingEvent) -> f32 {
		let mut confidence = 0.0;

		debug!(
			"Calculating confidence between {:?} and {:?}",
			event1.event.path, event2.event.path
		);

		// Same file type (directory vs file) - small weight since it's basic
		if event1.event.is_directory == event2.event.is_directory {
			let type_contribution = self.config.weight_size_match * 0.5; // Half of size weight
			confidence += type_contribution;
			debug!(
				"Same file type (+{}): directories={}",
				type_contribution, event1.event.is_directory
			);
		} else {
			debug!(
				"Different file types: {} vs {}",
				event1.event.is_directory, event2.event.is_directory
			);
		}

		// Same size (if available) - configurable weight
		if let (Some(size1), Some(size2)) = (event1.event.size, event2.event.size) {
			if size1 == size2 {
				confidence += self.config.weight_size_match;
				debug!("Same size (+{}): {}", self.config.weight_size_match, size1);
			} else {
				// Penalize different sizes but less than the positive contribution
				let penalty = self.config.weight_size_match * 0.3;
				confidence -= penalty;
				debug!("Different sizes (-{}): {} vs {}", penalty, size1, size2);
			}
		} else {
			debug!(
				"Size not available for one or both events: {:?} vs {:?}",
				event1.event.size, event2.event.size
			);
		}

		// Timing factor (closer in time = higher confidence) - configurable weight
		let time_diff = event1
			.timestamp
			.duration_since(event2.timestamp)
			.as_millis();
		let time_factor = (self.config.timeout.as_millis()
			- time_diff.min(self.config.timeout.as_millis())) as f32
			/ self.config.timeout.as_millis() as f32;
		let time_contribution = time_factor * self.config.weight_time_factor;
		confidence += time_contribution;
		debug!(
			"Time factor (+{}): time_diff={}ms, timeout={}ms, factor={:.3}",
			time_contribution,
			time_diff,
			self.config.timeout.as_millis(),
			time_factor
		);
		// Inode matching (Unix-like systems) - highest confidence when available
		if let (Some(inode1), Some(inode2)) = (event1.inode, event2.inode) {
			if inode1 == inode2 {
				confidence += self.config.weight_inode_match;
				debug!(
					"Same inode (+{}): {}",
					self.config.weight_inode_match, inode1
				);
			} else {
				debug!("Different inodes: {} vs {}", inode1, inode2);
			}
		} else if let (Some(wid1), Some(wid2)) = (event1.windows_id, event2.windows_id) {
			// Windows ID matching - high confidence when available
			if wid1 == wid2 {
				confidence += self.config.weight_inode_match; // Use same weight as inode
				debug!(
					"Same Windows ID (+{}): {}",
					self.config.weight_inode_match, wid1
				);
			} else {
				debug!("Different Windows IDs: {} vs {}", wid1, wid2);
			}
		} else {
			debug!(
				"Inodes/Windows IDs not available: {:?} vs {:?}",
				event1.inode, event2.inode
			);
		}

		// Content hash matching - very high confidence when available
		if let (Some(hash1), Some(hash2)) = (&event1.content_hash, &event2.content_hash) {
			if hash1 == hash2 {
				confidence += self.config.weight_content_hash;
				debug!("Same content hash (+{})", self.config.weight_content_hash);
			} else {
				debug!("Different content hashes");
			}
		} else {
			debug!(
				"Content hashes not available: {:?} vs {:?}",
				event1.content_hash.is_some(),
				event2.content_hash.is_some()
			);
		}
		// Name similarity - configurable weight
		let name_similarity =
			self.calculate_name_similarity(&event1.event.path, &event2.event.path);
		let mut name_contribution = name_similarity * self.config.weight_name_similarity;

		// Bonus for same filename in different directories (strong indicator of move)
		if name_similarity == 1.0 {
			if let (Some(dir1), Some(dir2)) =
				(event1.event.path.parent(), event2.event.path.parent())
			{
				if dir1 != dir2 {
					// Same filename, different directories - strong move indicator
					let directory_move_bonus = self.config.weight_name_similarity * 1.5; // 1.5x bonus
					name_contribution += directory_move_bonus;
					debug!(
						"Directory move bonus (+{:.3}): same filename in different directories",
						directory_move_bonus
					);
				}
			}
		}

		confidence += name_contribution;
		debug!(
			"Name similarity (+{:.3}): similarity={:.3}",
			name_contribution, name_similarity
		);

		// Confidence is now a weighted sum - cap at 1.0 for probability
		let final_confidence = confidence.clamp(0.0, 1.0);
		debug!(
			"Final weighted confidence: {:.3} (before clamp: {:.3})",
			final_confidence, confidence
		);
		final_confidence
	}
	fn calculate_name_similarity(&self, path1: &Path, path2: &Path) -> f32 {
		let name1 = path1.file_name().and_then(|n| n.to_str()).unwrap_or("");
		let name2 = path2.file_name().and_then(|n| n.to_str()).unwrap_or("");

		if name1 == name2 {
			// Same filename - check if this looks like a move between directories
			if let (Some(dir1), Some(dir2)) = (path1.parent(), path2.parent()) {
				if dir1 != dir2 {
					// Different directories with same filename - likely a move!
					// Give this a higher confidence boost
					debug!(
						"Same filename in different directories: {:?} -> {:?}",
						dir1, dir2
					);
					return 1.0; // Maximum similarity for same filename in different dirs
				}
			}
			return 1.0;
		}

		// Simple Levenshtein distance-based similarity
		let distance = levenshtein_distance(name1, name2);
		let max_len = name1.len().max(name2.len());

		if max_len == 0 {
			0.0
		} else {
			1.0 - (distance as f32 / max_len as f32)
		}
	}
	fn determine_detection_method(
		&self,
		event1: &PendingEvent,
		event2: &PendingEvent,
	) -> MoveDetectionMethod {
		// Priority order for detection methods
		if event1.inode.is_some() && event2.inode.is_some() && event1.inode == event2.inode {
			return MoveDetectionMethod::InodeMatching;
		}

		if event1.windows_id.is_some()
			&& event2.windows_id.is_some()
			&& event1.windows_id == event2.windows_id
		{
			return MoveDetectionMethod::InodeMatching; // Treat Windows ID same as inode
		}

		if event1.content_hash.is_some()
			&& event2.content_hash.is_some()
			&& event1.content_hash == event2.content_hash
		{
			return MoveDetectionMethod::ContentHash;
		}

		if let (Some(size1), Some(size2)) = (event1.event.size, event2.event.size) {
			if size1 == size2 {
				return MoveDetectionMethod::MetadataMatching;
			}
		}

		MoveDetectionMethod::NameAndTiming
	}

	fn is_within_timeout(&self, timestamp: &Instant) -> bool {
		timestamp.elapsed() <= self.config.timeout
	}
	async fn cleanup_expired_events(&mut self) {
		let now = Instant::now();

		// Clean up expired pending rename-from event
		if let Some((_, timestamp)) = &self.pending_rename_from {
			if now.duration_since(*timestamp) > Duration::from_millis(100) {
				debug!("Cleaning up expired rename FROM event");
				self.pending_rename_from = None;
			}
		}

		// Clean up inode-based removes
		self.pending_removes_by_inode
			.retain(|_, pending| now.duration_since(pending.timestamp) <= self.config.timeout);

		// Clean up Windows ID-based removes
		self.pending_removes_by_windows_id
			.retain(|_, pending| now.duration_since(pending.timestamp) <= self.config.timeout);

		// Clean up size-based removes
		for events in self.pending_removes_by_size.values_mut() {
			events.retain(|pending| now.duration_since(pending.timestamp) <= self.config.timeout);
		}
		self.pending_removes_by_size
			.retain(|_, events| !events.is_empty());
		// Clean up no-size removes
		self.pending_removes_no_size
			.retain(|pending| now.duration_since(pending.timestamp) <= self.config.timeout);

		// Clean up inode-based creates
		self.pending_creates_by_inode
			.retain(|_, pending| now.duration_since(pending.timestamp) <= self.config.timeout);

		// Clean up Windows ID-based creates
		self.pending_creates_by_windows_id
			.retain(|_, pending| now.duration_since(pending.timestamp) <= self.config.timeout);

		// Clean up size-based creates
		for events in self.pending_creates_by_size.values_mut() {
			events.retain(|pending| now.duration_since(pending.timestamp) <= self.config.timeout);
		}
		self.pending_creates_by_size
			.retain(|_, events| !events.is_empty());

		// Clean up no-size creates
		self.pending_creates_no_size
			.retain(|pending| now.duration_since(pending.timestamp) <= self.config.timeout);

		// Clean up old metadata cache entries
		self.metadata_cache
			.retain(|_, metadata| now.duration_since(metadata.last_seen) <= self.config.timeout);
	}
	async fn get_inode(&self, path: &Path) -> Option<u64> {
		#[cfg(unix)]
		{
			use std::os::unix::fs::MetadataExt;
			if let Ok(metadata) = std::fs::metadata(path) {
				return Some(metadata.ino());
			}
		}

		// On non-unix platforms, this will do nothing.
		#[cfg(not(unix))]
		let _ = path; // Prevent unused variable warning

		None
	}

	async fn get_windows_id(&self, path: &Path) -> Option<u64> {
		#[cfg(windows)]
		{
			use std::os::windows::fs::MetadataExt;
			if let Ok(metadata) = std::fs::metadata(path) {
				// Combine creation time and file size for a pseudo-unique identifier
				let creation_time = metadata.creation_time();
				let file_size = metadata.len();
				// Use upper 32 bits for creation time, lower 32 bits for size
				let windows_id = ((creation_time >> 32) << 32) | (file_size & 0xFFFFFFFF);
				return Some(windows_id);
			}
		}

		#[cfg(not(windows))]
		let _ = path; // Prevent unused variable warning

		None
	}
	async fn get_content_hash(&self, path: &Path) -> Option<String> {
		if !path.is_file() {
			return None;
		}

		let metadata = match tokio::fs::metadata(path).await {
			Ok(md) => md,
			Err(_) => return None,
		};
		// Only hash small files for performance
		if metadata.len() > self.config.content_hash_max_file_size {
			return None;
		}

		let mut file = match File::open(path) {
			Ok(f) => f,
			Err(_) => return None,
		};

		// Use a fast, non-cryptographic hash
		let mut hasher = XxHash64::with_seed(0);
		let mut buffer = [0; 8192]; // 8KB buffer

		loop {
			match file.read(&mut buffer) {
				Ok(0) => break, // End of file
				Ok(n) => hasher.write(&buffer[..n]),
				Err(_) => return None, // IO error
			}
		}

		Some(format!("{:x}", hasher.finish()))
	}

	// Helper methods for managing bucketed pending events
	fn add_pending_remove(&mut self, pending: PendingEvent) {
		debug!(
			"Adding pending remove: path={:?}, size={:?}, inode={:?}, windows_id={:?}",
			pending.event.path, pending.event.size, pending.inode, pending.windows_id
		);

		// Fast path: inode-based indexing (Unix only)
		if let Some(inode) = pending.inode {
			debug!("Storing remove by inode: {}", inode);
			self.pending_removes_by_inode.insert(inode, pending);
			return;
		}

		// Fast path: Windows ID-based indexing
		if let Some(windows_id) = pending.windows_id {
			debug!("Storing remove by Windows ID: {}", windows_id);
			self.pending_removes_by_windows_id
				.insert(windows_id, pending);
			return;
		}

		// Fast path: size-based indexing
		if let Some(size) = pending.event.size {
			debug!("Storing remove by size: {}", size);
			self.pending_removes_by_size
				.entry(size)
				.or_default()
				.push(pending);
		} else {
			debug!("Storing remove without size");
			self.pending_removes_no_size.push(pending);
		}
	}
	fn add_pending_create(&mut self, pending: PendingEvent) {
		// Fast path: inode-based indexing (Unix only)
		if let Some(inode) = pending.inode {
			self.pending_creates_by_inode.insert(inode, pending);
			return;
		}

		// Fast path: Windows ID-based indexing
		if let Some(windows_id) = pending.windows_id {
			self.pending_creates_by_windows_id
				.insert(windows_id, pending);
			return;
		}

		// Fast path: size-based indexing
		if let Some(size) = pending.event.size {
			self.pending_creates_by_size
				.entry(size)
				.or_default()
				.push(pending);
		} else {
			self.pending_creates_no_size.push(pending);
		}
	}
	fn remove_pending_create_by_id(&mut self, id: uuid::Uuid) {
		// Remove from inode-based storage
		self.pending_creates_by_inode
			.retain(|_, p| p.event.id != id);

		// Remove from Windows ID-based storage
		self.pending_creates_by_windows_id
			.retain(|_, p| p.event.id != id);

		// Remove from size-based storage
		for events in self.pending_creates_by_size.values_mut() {
			events.retain(|p| p.event.id != id);
		}
		self.pending_creates_by_size
			.retain(|_, events| !events.is_empty());

		// Remove from no-size storage
		self.pending_creates_no_size.retain(|p| p.event.id != id);
	}
	fn remove_pending_remove_by_id(&mut self, id: uuid::Uuid) {
		// Remove from inode-based storage
		self.pending_removes_by_inode
			.retain(|_, p| p.event.id != id);

		// Remove from Windows ID-based storage
		self.pending_removes_by_windows_id
			.retain(|_, p| p.event.id != id);

		// Remove from size-based storage
		for events in self.pending_removes_by_size.values_mut() {
			events.retain(|p| p.event.id != id);
		}
		self.pending_removes_by_size
			.retain(|_, events| !events.is_empty());

		// Remove from no-size storage
		self.pending_removes_no_size.retain(|p| p.event.id != id);
	}
	fn count_pending_removes(&self) -> usize {
		let inode_count = self.pending_removes_by_inode.len();
		let windows_id_count = self.pending_removes_by_windows_id.len();
		let size_count: usize = self.pending_removes_by_size.values().map(|v| v.len()).sum();
		let no_size_count = self.pending_removes_no_size.len();
		inode_count + windows_id_count + size_count + no_size_count
	}

	fn count_pending_creates(&self) -> usize {
		let inode_count = self.pending_creates_by_inode.len();
		let windows_id_count = self.pending_creates_by_windows_id.len();
		let size_count: usize = self.pending_creates_by_size.values().map(|v| v.len()).sum();
		let no_size_count = self.pending_creates_no_size.len();
		inode_count + windows_id_count + size_count + no_size_count
	}
}

// Simple Levenshtein distance implementation
fn levenshtein_distance(s1: &str, s2: &str) -> usize {
	let len1 = s1.chars().count();
	let len2 = s2.chars().count();

	if len1 == 0 {
		return len2;
	}
	if len2 == 0 {
		return len1;
	}
	let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];

	// Initialize first row and column
	for (i, row) in matrix.iter_mut().enumerate() {
		row[0] = i;
	}
	for j in 1..=len2 {
		matrix[0][j] = j;
	}

	let s1_chars: Vec<char> = s1.chars().collect();
	let s2_chars: Vec<char> = s2.chars().collect();

	for (i, c1) in s1_chars.iter().enumerate() {
		for (j, c2) in s2_chars.iter().enumerate() {
			let cost = if c1 == c2 { 0 } else { 1 };
			matrix[i + 1][j + 1] = (matrix[i][j + 1] + 1)
				.min(matrix[i + 1][j] + 1)
				.min(matrix[i][j] + cost);
		}
	}

	matrix[len1][len2]
}
