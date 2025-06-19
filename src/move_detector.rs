use crate::events::{EventType, FileSystemEvent, MoveDetectionMethod, MoveEvent};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::time::Instant;
use tracing::{debug, warn};

#[derive(Debug, Clone)]
struct PendingEvent {
	event: FileSystemEvent,
	timestamp: Instant,
	inode: Option<u64>,
	content_hash: Option<String>,
}

pub struct MoveDetector {
	pending_removes: HashMap<PathBuf, PendingEvent>,
	pending_creates: HashMap<PathBuf, PendingEvent>,
	timeout: Duration,
	max_pending: usize,
}

impl MoveDetector {
	pub fn new(timeout_ms: u64) -> Self {
		Self {
			pending_removes: HashMap::new(),
			pending_creates: HashMap::new(),
			timeout: Duration::from_millis(timeout_ms),
			max_pending: 1000, // Prevent memory leaks
		}
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
			self.pending_creates
				.retain(|_, p| p.event.id != matching_create.event.id);

			debug!(
				"Detected move: {:?} -> {:?}",
				matching_create.event.path, event.path
			);
			return vec![move_event_fs];
		}

		// Store this removal as pending
		if self.pending_removes.len() < self.max_pending {
			self.pending_removes.insert(event.path.clone(), pending);
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
			self.pending_removes
				.retain(|_, p| p.event.id != matching_remove.event.id);

			return vec![move_event_fs];
		}

		// Store this creation as pending
		if self.pending_creates.len() < self.max_pending {
			self.pending_creates.insert(event.path.clone(), pending);
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

		for pending_remove in self.pending_removes.values() {
			if self.is_within_timeout(&pending_remove.timestamp) {
				let confidence = self.calculate_confidence(pending_remove, create_event);

				if confidence > 0.3 {
					// Minimum confidence threshold
					match best_match {
						Some((_, best_confidence)) if confidence > best_confidence => {
							best_match = Some((pending_remove.clone(), confidence));
						}
						None => {
							best_match = Some((pending_remove.clone(), confidence));
						}
						_ => {}
					}
				}
			}
		}

		best_match.map(|(event, _)| event)
	}

	async fn find_matching_create(&self, remove_event: &PendingEvent) -> Option<PendingEvent> {
		let mut best_match: Option<(PendingEvent, f32)> = None;

		for pending_create in self.pending_creates.values() {
			if self.is_within_timeout(&pending_create.timestamp) {
				let confidence = self.calculate_confidence(remove_event, pending_create);

				if confidence > 0.3 {
					// Minimum confidence threshold
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

		// Same file type (directory vs file)
		if event1.event.is_directory == event2.event.is_directory {
			confidence += 0.2;
			factors += 1;
		}

		// Same size (if available)
		if let (Some(size1), Some(size2)) = (event1.event.size, event2.event.size) {
			if size1 == size2 {
				confidence += 0.3;
			} else {
				confidence -= 0.1; // Penalize different sizes
			}
			factors += 1;
		}

		// Timing factor (closer in time = higher confidence)
		let time_diff = event1
			.timestamp
			.duration_since(event2.timestamp)
			.as_millis();
		let time_factor = (self.timeout.as_millis() - time_diff.min(self.timeout.as_millis()))
			as f32 / self.timeout.as_millis() as f32;
		confidence += time_factor * 0.3;
		factors += 1;

		// Inode matching (Unix-like systems)
		if let (Some(inode1), Some(inode2)) = (event1.inode, event2.inode) {
			if inode1 == inode2 {
				confidence += 0.4; // High confidence for inode match
			}
			factors += 1;
		}

		// Content hash matching
		if let (Some(hash1), Some(hash2)) = (&event1.content_hash, &event2.content_hash) {
			if hash1 == hash2 {
				confidence += 0.5; // Very high confidence for content match
			}
			factors += 1;
		}

		// Name similarity
		let name_similarity =
			self.calculate_name_similarity(&event1.event.path, &event2.event.path);
		confidence += name_similarity * 0.2;
		factors += 1;

		// Normalize by number of factors considered
		if factors > 0 {
			confidence / factors as f32
		} else {
			0.0
		}
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
		timestamp.elapsed() <= self.timeout
	}

	async fn cleanup_expired_events(&mut self) {
		let now = Instant::now();

		self.pending_removes
			.retain(|_, pending| now.duration_since(pending.timestamp) <= self.timeout);

		self.pending_creates
			.retain(|_, pending| now.duration_since(pending.timestamp) <= self.timeout);
	}

	async fn get_inode(&self, _path: &Path) -> Option<u64> {
		// Platform-specific inode retrieval would go here
		// For now, return None (not implemented for Windows compatibility)
		None
	}

	async fn get_content_hash(&self, path: &Path) -> Option<String> {
		// For files only, and only if they're small enough to hash quickly
		if path.is_file() {
			if let Ok(metadata) = std::fs::metadata(path) {
				if metadata.len() < 1024 * 1024 { // 1MB limit
					 // Simple hash implementation would go here
					 // For now, return None to keep example simple
				}
			}
		}
		None
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

	for i in 1..=len1 {
		matrix[i][0] = i;
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
