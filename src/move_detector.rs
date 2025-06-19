use crate::events::{EventType, FileSystemEvent, MoveDetectionMethod, MoveEvent};
use std::collections::HashMap;
use std::fs::File;
use std::hash::Hasher;
use std::io::Read;
use std::path::Path;
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
		Self {
			timeout: Duration::from_millis(1000),
			confidence_threshold: 0.7,
			weight_size_match: 0.2,
			weight_time_factor: 0.15,
			weight_inode_match: 0.4, // High weight for most reliable method
			weight_content_hash: 0.35,
			weight_name_similarity: 0.1,
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
}

pub struct MoveDetector {
	// Bucketed pending removes for O(1) size-based lookups
	pending_removes_by_size: HashMap<u64, Vec<PendingEvent>>,
	pending_removes_no_size: Vec<PendingEvent>,
	pending_removes_by_inode: HashMap<u64, PendingEvent>, // Unix only

	// Bucketed pending creates for O(1) size-based lookups
	pending_creates_by_size: HashMap<u64, Vec<PendingEvent>>,
	pending_creates_no_size: Vec<PendingEvent>,
	pending_creates_by_inode: HashMap<u64, PendingEvent>, // Unix only

	config: MoveDetectorConfig,
}

impl MoveDetector {
	pub fn new(config: MoveDetectorConfig) -> Self {
		Self {
			pending_removes_by_size: HashMap::new(),
			pending_removes_no_size: Vec::new(),
			pending_removes_by_inode: HashMap::new(),
			pending_creates_by_size: HashMap::new(),
			pending_creates_no_size: Vec::new(),
			pending_creates_by_inode: HashMap::new(),
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
		self.cleanup_expired_events().await;

		match event.event_type {
			EventType::Remove => self.handle_remove_event(event).await,
			EventType::Create => self.handle_create_event(event).await,
			_ => vec![event], // Pass through other events
		}
	}

	async fn handle_remove_event(&mut self, event: FileSystemEvent) -> Vec<FileSystemEvent> {
		let pending = PendingEvent {
			timestamp: Instant::now(),
			inode: self.get_inode(&event.path).await,
			content_hash: None, // File is being removed, can't hash
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
	}	async fn find_matching_remove(&self, create_event: &PendingEvent) -> Option<PendingEvent> {
		let mut best_match: Option<(PendingEvent, f32)> = None;

		debug!(
			"Finding matching remove for create event: path={:?}, size={:?}, inode={:?}",
			create_event.event.path, create_event.event.size, create_event.inode
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
		// Fast path: size-based matching
		if let Some(size) = create_event.event.size {
			debug!("Checking size {} for matching removes", size);
			if let Some(removes_with_size) = self.pending_removes_by_size.get(&size) {
				debug!("Found {} pending removes with size {}", removes_with_size.len(), size);				for pending_remove in removes_with_size {
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
							debug!("Confidence {} too low (< {})", confidence, self.config.confidence_threshold);
						}
					} else {
						debug!("Remove event for {:?} expired", pending_remove.event.path);
					}
				}
			} else {
				debug!("No pending removes found for size {}", size);
			}
		}		// Fallback: check events without size
		debug!("Checking {} events without size", self.pending_removes_no_size.len());
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
					debug!("Confidence {} too low (< {})", confidence, self.config.confidence_threshold);
				}
			} else {
				debug!("Remove event for {:?} expired", pending_remove.event.path);
			}
		}

		match &best_match {
			Some((event, confidence)) => {
				debug!("Best match found: {:?} with confidence {}", event.event.path, confidence);
			}
			None => {
				debug!("No matching remove found for create {:?}", create_event.event.path);
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
		let mut factors = 0;

		debug!(
			"Calculating confidence between {:?} and {:?}",
			event1.event.path, event2.event.path
		);

		// Same file type (directory vs file)
		if event1.event.is_directory == event2.event.is_directory {
			confidence += 0.2;
			factors += 1;
			debug!("Same file type (+0.2): directories={}", event1.event.is_directory);
		} else {
			debug!("Different file types: {} vs {}", event1.event.is_directory, event2.event.is_directory);
		}

		// Same size (if available)
		if let (Some(size1), Some(size2)) = (event1.event.size, event2.event.size) {
			if size1 == size2 {
				confidence += 0.3;
				debug!("Same size (+0.3): {}", size1);
			} else {
				confidence -= 0.1; // Penalize different sizes
				debug!("Different sizes (-0.1): {} vs {}", size1, size2);
			}
			factors += 1;
		} else {
			debug!("Size not available for one or both events: {:?} vs {:?}", event1.event.size, event2.event.size);
		}

		// Timing factor (closer in time = higher confidence)
		let time_diff = event1
			.timestamp
			.duration_since(event2.timestamp)
			.as_millis();
		let time_factor = (self.config.timeout.as_millis()
			- time_diff.min(self.config.timeout.as_millis())) as f32
			/ self.config.timeout.as_millis() as f32;
		confidence += time_factor * 0.3;
		factors += 1;
		debug!("Time factor (+{}): time_diff={}ms, timeout={}ms", 
			time_factor * 0.3, time_diff, self.config.timeout.as_millis());

		// Inode matching (Unix-like systems)
		if let (Some(inode1), Some(inode2)) = (event1.inode, event2.inode) {
			if inode1 == inode2 {
				confidence += 0.4; // High confidence for inode match
				debug!("Same inode (+0.4): {}", inode1);
			} else {
				debug!("Different inodes: {} vs {}", inode1, inode2);
			}
			factors += 1;
		} else {
			debug!("Inodes not available: {:?} vs {:?}", event1.inode, event2.inode);
		}

		// Content hash matching
		if let (Some(hash1), Some(hash2)) = (&event1.content_hash, &event2.content_hash) {
			if hash1 == hash2 {
				confidence += 0.5; // Very high confidence for content match
				debug!("Same content hash (+0.5)");
			} else {
				debug!("Different content hashes");
			}
			factors += 1;
		} else {
			debug!("Content hashes not available: {:?} vs {:?}", 
				event1.content_hash.is_some(), event2.content_hash.is_some());
		}

		// Name similarity
		let name_similarity =
			self.calculate_name_similarity(&event1.event.path, &event2.event.path);
		confidence += name_similarity * 0.2;
		factors += 1;
		debug!("Name similarity (+{}): similarity={}", name_similarity * 0.2, name_similarity);

		// Normalize by number of factors considered
		let final_confidence = if factors > 0 {
			confidence / factors as f32
		} else {
			0.0
		};
		debug!("Final confidence: {} / {} factors = {}", confidence, factors, final_confidence);
		final_confidence
	}

	fn calculate_name_similarity(&self, path1: &Path, path2: &Path) -> f32 {
		let name1 = path1.file_name().and_then(|n| n.to_str()).unwrap_or("");
		let name2 = path2.file_name().and_then(|n| n.to_str()).unwrap_or("");

		if name1 == name2 {
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
		// Clean up inode-based removes
		self.pending_removes_by_inode
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
		// Clean up size-based creates
		for events in self.pending_creates_by_size.values_mut() {
			events.retain(|pending| now.duration_since(pending.timestamp) <= self.config.timeout);
		}
		self.pending_creates_by_size
			.retain(|_, events| !events.is_empty());

		// Clean up no-size creates
		self.pending_creates_no_size
			.retain(|pending| now.duration_since(pending.timestamp) <= self.config.timeout);
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

		Some(format!("{:x}", hasher.finish()))	}

	// Helper methods for managing bucketed pending events
	fn add_pending_remove(&mut self, pending: PendingEvent) {
		debug!(
			"Adding pending remove: path={:?}, size={:?}, inode={:?}",
			pending.event.path, pending.event.size, pending.inode
		);

		// Fast path: inode-based indexing (Unix only)
		if let Some(inode) = pending.inode {
			debug!("Storing remove by inode: {}", inode);
			self.pending_removes_by_inode.insert(inode, pending);
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
		let size_count: usize = self.pending_removes_by_size.values().map(|v| v.len()).sum();
		let no_size_count = self.pending_removes_no_size.len();
		inode_count + size_count + no_size_count
	}

	fn count_pending_creates(&self) -> usize {
		let inode_count = self.pending_creates_by_inode.len();
		let size_count: usize = self.pending_creates_by_size.values().map(|v| v.len()).sum();
		let no_size_count = self.pending_creates_no_size.len();
		inode_count + size_count + no_size_count
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
