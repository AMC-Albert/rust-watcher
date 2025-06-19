use crate::events::MoveDetectionMethod;
use crate::move_detection::config::MoveDetectorConfig;
use crate::move_detection::events::{PendingEvent, PendingEventsStorage};
use crate::move_detection::heuristics::calculate_name_similarity;
use std::hash::{Hash, Hasher};
use std::path::Path;
use twox_hash::XxHash64;

/// Move matching algorithms and confidence calculations
pub struct MoveMatching;

impl MoveMatching {
	/// Find a matching create event for a given remove event
	pub async fn find_matching_create(
		remove_event: &PendingEvent,
		storage: &PendingEventsStorage,
		config: &MoveDetectorConfig,
	) -> Option<PendingEvent> {
		// Quick inode-based matching for Unix systems
		#[cfg(unix)]
		if let Some(inode) = remove_event.inode {
			if let Some(create_event) = storage.creates_by_inode.get(&inode) {
				// Don't match events with the same path (not a move)
				if create_event.event.path != remove_event.event.path {
					let confidence = Self::calculate_confidence(remove_event, create_event, config);
					if confidence >= config.confidence_threshold {
						return Some(create_event.clone());
					}
				}
			}
		}

		// Windows-specific ID matching
		#[cfg(windows)]
		if let Some(windows_id) = remove_event.windows_id {
			if let Some(create_event) = storage.creates_by_windows_id.get(&windows_id) {
				// Don't match events with the same path (not a move)
				if create_event.event.path != remove_event.event.path {
					let confidence = Self::calculate_confidence(remove_event, create_event, config);
					if confidence >= config.confidence_threshold {
						return Some(create_event.clone());
					}
				}
			}
		}
		// Size-based matching with confidence calculation
		if let Some(size) = remove_event.event.size {
			// Remove event has size - look for creates with same size
			if let Some(candidates) = storage.creates_by_size.get(&size) {
				return Self::find_best_match_in_candidates(remove_event, candidates, config);
			}
		} else {
			// Remove event has no size - this happens when file was removed and we couldn't get metadata
			// We need to check ALL create events since we don't know what size to match

			// First check creates without size (directories, etc.)
			if let Some(match_result) =
				Self::find_best_match_in_candidates(remove_event, &storage.creates_no_size, config)
			{
				return Some(match_result);
			}

			// Then check ALL size-based creates (iterate through all size buckets)
			for candidates in storage.creates_by_size.values() {
				if let Some(match_result) =
					Self::find_best_match_in_candidates(remove_event, candidates, config)
				{
					return Some(match_result);
				}
			}
		}

		None
	}

	/// Find a matching remove event for a given create event
	pub async fn find_matching_remove(
		create_event: &PendingEvent,
		storage: &PendingEventsStorage,
		config: &MoveDetectorConfig,
	) -> Option<PendingEvent> {
		// Quick inode-based matching for Unix systems
		#[cfg(unix)]
		if let Some(inode) = create_event.inode {
			if let Some(remove_event) = storage.removes_by_inode.get(&inode) {
				// Don't match events with the same path (not a move)
				if remove_event.event.path != create_event.event.path {
					let confidence = Self::calculate_confidence(remove_event, create_event, config);
					if confidence >= config.confidence_threshold {
						return Some(remove_event.clone());
					}
				}
			}
		}

		// Windows-specific ID matching
		#[cfg(windows)]
		if let Some(windows_id) = create_event.windows_id {
			if let Some(remove_event) = storage.removes_by_windows_id.get(&windows_id) {
				// Don't match events with the same path (not a move)
				if remove_event.event.path != create_event.event.path {
					let confidence = Self::calculate_confidence(remove_event, create_event, config);
					if confidence >= config.confidence_threshold {
						return Some(remove_event.clone());
					}
				}
			}
		}
		// Size-based matching with confidence calculation
		if let Some(size) = create_event.event.size {
			// First check removes with the same size
			if let Some(candidates) = storage.removes_by_size.get(&size) {
				if let Some(match_result) =
					Self::find_best_match_in_candidates_for_create(create_event, candidates, config)
				{
					return Some(match_result);
				}
			}

			// Also check removes without size (files removed before we could get metadata)
			if let Some(match_result) = Self::find_best_match_in_candidates_for_create(
				create_event,
				&storage.removes_no_size,
				config,
			) {
				return Some(match_result);
			}
		} else {
			// Check candidates without size
			return Self::find_best_match_in_candidates_for_create(
				create_event,
				&storage.removes_no_size,
				config,
			);
		}

		None
	}

	/// Calculate confidence score for a potential move match
	pub fn calculate_confidence(
		remove_event: &PendingEvent,
		create_event: &PendingEvent,
		config: &MoveDetectorConfig,
	) -> f32 {
		let mut confidence = 0.0;
		// Size matching
		let size_match = match (remove_event.event.size, create_event.event.size) {
			(Some(size1), Some(size2)) if size1 == size2 => 1.0,
			(None, None) => 0.8,    // Both are directories or unknown
			(None, Some(_)) => 0.6, // Remove event has no size (common in real cut/paste), but create does
			(Some(_), None) => 0.6, // Create event has no size, but remove does
			_ => 0.0,               // Different sizes
		};
		confidence += size_match * config.weight_size_match;

		// Time factor (closer in time = higher confidence)
		let time_diff = if create_event.timestamp > remove_event.timestamp {
			create_event
				.timestamp
				.duration_since(remove_event.timestamp)
		} else {
			remove_event
				.timestamp
				.duration_since(create_event.timestamp)
		};

		let time_factor = if time_diff <= config.timeout {
			1.0 - (time_diff.as_millis() as f32 / config.timeout.as_millis() as f32)
		} else {
			0.0
		};
		confidence += time_factor * config.weight_time_factor;

		// Inode matching (Unix only)
		#[cfg(unix)]
		{
			let inode_match = match (remove_event.inode, create_event.inode) {
				(Some(inode1), Some(inode2)) if inode1 == inode2 => 1.0,
				_ => 0.0,
			};
			confidence += inode_match * config.weight_inode_match;
		}

		// Windows ID matching
		#[cfg(windows)]
		{
			let windows_id_match = match (remove_event.windows_id, create_event.windows_id) {
				(Some(id1), Some(id2)) if id1 == id2 => 1.0,
				_ => 0.0,
			};
			confidence += windows_id_match * config.weight_inode_match; // Reuse inode weight
		}

		// Content hash matching
		let content_hash_match = match (&remove_event.content_hash, &create_event.content_hash) {
			(Some(hash1), Some(hash2)) if hash1 == hash2 => 1.0,
			(None, None) => 0.5, // Both are directories or unhashable
			_ => 0.0,
		};
		confidence += content_hash_match * config.weight_content_hash;

		// Name similarity
		let name_similarity =
			calculate_name_similarity(&remove_event.event.path, &create_event.event.path);
		confidence += name_similarity * config.weight_name_similarity;

		confidence.clamp(0.0, 1.0)
	}

	/// Determine the detection method used for the match
	pub fn determine_detection_method(
		remove_event: &PendingEvent,
		create_event: &PendingEvent,
	) -> MoveDetectionMethod {
		// Check inode first (most reliable)
		#[cfg(unix)]
		if remove_event.inode.is_some() && remove_event.inode == create_event.inode {
			return MoveDetectionMethod::Inode;
		}

		// Check Windows ID
		#[cfg(windows)]
		if remove_event.windows_id.is_some() && remove_event.windows_id == create_event.windows_id {
			return MoveDetectionMethod::WindowsId;
		}

		// Check content hash
		if remove_event.content_hash.is_some()
			&& remove_event.content_hash == create_event.content_hash
		{
			return MoveDetectionMethod::ContentHash;
		}

		// Check size
		if remove_event.event.size.is_some() && remove_event.event.size == create_event.event.size {
			return MoveDetectionMethod::SizeAndTime;
		}

		// Fallback to heuristics
		MoveDetectionMethod::Heuristics
	}
	/// Find the best match among candidates
	fn find_best_match_in_candidates(
		remove_event: &PendingEvent,
		candidates: &[PendingEvent],
		config: &MoveDetectorConfig,
	) -> Option<PendingEvent> {
		candidates
			.iter()
			// Filter out candidates with the same path (not a move, just recreate at same location)
			.filter(|candidate| candidate.event.path != remove_event.event.path)
			.map(|candidate| {
				let confidence = Self::calculate_confidence(remove_event, candidate, config);
				(candidate, confidence)
			})
			.filter(|(_, confidence)| *confidence >= config.confidence_threshold)
			.max_by(|(_, conf1), (_, conf2)| {
				conf1
					.partial_cmp(conf2)
					.unwrap_or(std::cmp::Ordering::Equal)
			})
			.map(|(candidate, _)| candidate.clone())
	}
	/// Find the best match among candidates for create events
	fn find_best_match_in_candidates_for_create(
		create_event: &PendingEvent,
		candidates: &[PendingEvent],
		config: &MoveDetectorConfig,
	) -> Option<PendingEvent> {
		candidates
			.iter()
			// Filter out candidates with the same path (not a move, just recreate at same location)
			.filter(|candidate| candidate.event.path != create_event.event.path)
			.map(|candidate| {
				let confidence = Self::calculate_confidence(candidate, create_event, config);
				(candidate, confidence)
			})
			.filter(|(_, confidence)| *confidence >= config.confidence_threshold)
			.max_by(|(_, conf1), (_, conf2)| {
				conf1
					.partial_cmp(conf2)
					.unwrap_or(std::cmp::Ordering::Equal)
			})
			.map(|(candidate, _)| candidate.clone())
	}
}

/// Utilities for extracting file system metadata
pub struct MetadataExtractor;

impl MetadataExtractor {
	/// Get inode for a path (Unix only)
	pub async fn get_inode(_path: &Path) -> Option<u64> {
		#[cfg(unix)]
		{
			use std::os::unix::fs::MetadataExt;
			if let Ok(metadata) = std::fs::metadata(_path) {
				Some(metadata.ino())
			} else {
				None
			}
		}

		#[cfg(not(unix))]
		None
	}

	/// Get Windows-specific file identifier
	pub async fn get_windows_id(path: &Path) -> Option<u64> {
		#[cfg(windows)]
		{
			if let Ok(metadata) = std::fs::metadata(path) {
				// Use creation time (nanoseconds) combined with size for uniqueness
				let creation_time = metadata
					.created()
					.ok()?
					.duration_since(std::time::UNIX_EPOCH)
					.ok()?
					.as_nanos() as u64;
				let size = metadata.len();
				Some((creation_time << 32) | (size & 0xFFFFFFFF))
			} else {
				None
			}
		}

		#[cfg(not(windows))]
		{
			let _ = path; // Suppress unused parameter warning
			None
		}
	}

	/// Get content hash for a file (if small enough)
	pub async fn get_content_hash(path: &Path, max_size: u64) -> Option<String> {
		if !path.is_file() {
			return None;
		}

		let metadata = std::fs::metadata(path).ok()?;
		if metadata.len() > max_size {
			return None; // File too large
		}

		let mut file = std::fs::File::open(path).ok()?;
		let mut buffer = Vec::new();

		use std::io::Read;
		if file.read_to_end(&mut buffer).is_err() {
			return None;
		}

		let mut hasher = XxHash64::default();
		buffer.hash(&mut hasher);
		Some(format!("{:x}", hasher.finish()))
	}
}
