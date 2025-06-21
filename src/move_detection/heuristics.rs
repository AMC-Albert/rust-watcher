use crate::database::path_utils::paths_equal;
use crate::move_detection::events::PendingEventsStorage;
use crate::move_detection::metadata::MetadataCache;
use std::path::Path;

/// Heuristics for determining if a removed path was a file or directory
#[derive(Debug, Clone)]
pub struct PathTypeHeuristics {
	pub has_cached_metadata: bool,
	pub cached_had_size: bool,
	pub has_children_in_pending: bool,
	pub has_children_in_cache: bool,
	pub has_extension: bool,
	pub extension: Option<String>,
	pub confidence: f32,
}

impl PathTypeHeuristics {
	fn new() -> Self {
		Self {
			has_cached_metadata: false,
			cached_had_size: false,
			has_children_in_pending: false,
			has_children_in_cache: false,
			has_extension: false,
			extension: None,
			confidence: 0.0,
		}
	}

	/// Determine if the path was likely a directory based on heuristics
	pub fn is_likely_directory(&self) -> Option<bool> {
		if self.confidence < 0.3 {
			return None; // Not enough confidence
		}

		// Strong indicators for directory
		if self.has_children_in_pending || self.has_children_in_cache {
			return Some(true);
		}

		// Strong indicators for file
		if self.has_cached_metadata && self.cached_had_size {
			return Some(false);
		}

		// Weak indicators
		if self.has_extension {
			Some(false)
		} else if self.confidence > 0.5 {
			Some(true) // No extension and decent confidence = directory
		} else {
			None
		}
	}
}

/// Path type inference functionality
pub struct PathTypeInference;

impl PathTypeInference {
	/// Infer whether a removed path was likely a directory based on available context
	pub fn infer_path_type(
		path: &Path, metadata_cache: &MetadataCache, pending_events: &PendingEventsStorage,
	) -> Option<bool> {
		// First check if we have cached metadata for this exact path
		if let Some(metadata) = metadata_cache.get(path) {
			// If we have cached info and no size, it was likely a directory
			return Some(metadata.size.is_none());
		}

		// Check if any pending creates are under this path (indicating it's a directory)
		let has_children_in_pending = pending_events
			.creates_by_size
			.values()
			.flatten()
			.chain(pending_events.creates_no_size.iter())
			.any(|pending| {
				if let Some(parent) = pending.event.path.parent() {
					paths_equal(parent, path)
				} else {
					false
				}
			});

		if has_children_in_pending {
			return Some(true); // Has children, likely a directory
		}

		// Check recently cached paths for children
		let has_children_in_cache = metadata_cache.paths().any(|cached_path| {
			if let Some(parent) = cached_path.parent() {
				paths_equal(parent, path)
			} else {
				false
			}
		});

		if has_children_in_cache {
			return Some(true); // Has children, likely a directory
		}

		// Check file extension as fallback (original logic)
		if path.extension().is_none() {
			// No extension could indicate directory, but not reliable
			None // Return None to indicate uncertainty
		} else {
			Some(false) // Has extension, likely a file
		}
	}

	/// Get better heuristics for directory detection on removed paths
	pub fn get_path_type_heuristics(
		path: &Path, metadata_cache: &MetadataCache, pending_events: &PendingEventsStorage,
	) -> PathTypeHeuristics {
		let mut heuristics = PathTypeHeuristics::new();

		// Check cached metadata
		if let Some(metadata) = metadata_cache.get(path) {
			heuristics.has_cached_metadata = true;
			heuristics.cached_had_size = metadata.size.is_some();
			heuristics.confidence += 0.8; // High confidence from cache
		}

		// Check for child paths in pending events
		heuristics.has_children_in_pending = pending_events
			.creates_by_size
			.values()
			.flatten()
			.chain(pending_events.creates_no_size.iter())
			.any(|pending| {
				if let Some(parent) = pending.event.path.parent() {
					paths_equal(parent, path)
				} else {
					false
				}
			});

		if heuristics.has_children_in_pending {
			heuristics.confidence += 0.7;
		}

		// Check for child paths in metadata cache
		heuristics.has_children_in_cache = metadata_cache.paths().any(|cached_path| {
			if let Some(parent) = cached_path.parent() {
				paths_equal(parent, path)
			} else {
				false
			}
		});

		if heuristics.has_children_in_cache {
			heuristics.confidence += 0.6;
		}

		// File extension analysis
		if let Some(ext) = path.extension() {
			heuristics.has_extension = true;
			heuristics.extension = Some(ext.to_string_lossy().to_string());
			heuristics.confidence += 0.3; // Lower confidence, but still useful
		}

		// Path depth analysis (deeper paths less likely to be directories)
		let depth = path.components().count();
		if depth > 5 {
			heuristics.confidence += 0.1;
		}

		heuristics
	}
}

/// Simple Levenshtein distance implementation for name similarity
pub fn levenshtein_distance(s1: &str, s2: &str) -> usize {
	let len1 = s1.len();
	let len2 = s2.len();

	if len1 == 0 {
		return len2;
	}
	if len2 == 0 {
		return len1;
	}

	let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];
	// Initialize first row and column
	#[allow(clippy::needless_range_loop)]
	for i in 0..=len1 {
		matrix[i][0] = i;
	}
	#[allow(clippy::needless_range_loop)]
	for j in 0..=len2 {
		matrix[0][j] = j;
	}

	let s1_chars: Vec<char> = s1.chars().collect();
	let s2_chars: Vec<char> = s2.chars().collect();

	for i in 1..=len1 {
		for j in 1..=len2 {
			let cost = if s1_chars[i - 1] == s2_chars[j - 1] { 0 } else { 1 };
			matrix[i][j] = std::cmp::min(
				std::cmp::min(
					matrix[i - 1][j] + 1, // deletion
					matrix[i][j - 1] + 1, // insertion
				),
				matrix[i - 1][j - 1] + cost, // substitution
			);
		}
	}

	matrix[len1][len2]
}

/// Calculate name similarity between two paths
pub fn calculate_name_similarity(path1: &Path, path2: &Path) -> f32 {
	let name1 = path1.file_name().and_then(|n| n.to_str()).unwrap_or("");
	let name2 = path2.file_name().and_then(|n| n.to_str()).unwrap_or("");

	if name1.is_empty() || name2.is_empty() {
		return 0.0;
	}

	let distance = levenshtein_distance(name1, name2);
	let max_len = std::cmp::max(name1.len(), name2.len());

	if max_len == 0 {
		1.0
	} else {
		1.0 - (distance as f32 / max_len as f32)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::path::PathBuf;

	#[test]
	fn test_path_type_heuristics_creation() {
		let heuristics = PathTypeHeuristics::new();
		assert!(!heuristics.has_cached_metadata);
		assert!(!heuristics.cached_had_size);
		assert!(!heuristics.has_children_in_pending);
		assert!(!heuristics.has_children_in_cache);
		assert!(!heuristics.has_extension);
		assert_eq!(heuristics.extension, None);
		assert_eq!(heuristics.confidence, 0.0);
	}

	#[test]
	fn test_is_likely_directory_low_confidence() {
		let heuristics = PathTypeHeuristics { confidence: 0.2, ..PathTypeHeuristics::new() };

		assert_eq!(heuristics.is_likely_directory(), None);
	}

	#[test]
	fn test_is_likely_directory_with_children() {
		let heuristics = PathTypeHeuristics {
			confidence: 0.8,
			has_children_in_pending: true,
			..PathTypeHeuristics::new()
		};

		assert_eq!(heuristics.is_likely_directory(), Some(true));
	}

	#[test]
	fn test_is_likely_file_with_size() {
		let heuristics = PathTypeHeuristics {
			confidence: 0.8,
			has_cached_metadata: true,
			cached_had_size: true,
			..PathTypeHeuristics::new()
		};

		assert_eq!(heuristics.is_likely_directory(), Some(false));
	}

	#[test]
	fn test_is_likely_file_with_extension() {
		let heuristics = PathTypeHeuristics {
			confidence: 0.8,
			has_extension: true,
			extension: Some("txt".to_string()),
			..PathTypeHeuristics::new()
		};

		assert_eq!(heuristics.is_likely_directory(), Some(false));
	}

	#[test]
	fn test_calculate_name_similarity_identical() {
		let path1 = PathBuf::from("test.txt");
		let path2 = PathBuf::from("test.txt");

		let similarity = calculate_name_similarity(&path1, &path2);
		assert_eq!(similarity, 1.0);
	}

	#[test]
	fn test_calculate_name_similarity_different() {
		let path1 = PathBuf::from("file1.txt");
		let path2 = PathBuf::from("file2.txt");

		let similarity = calculate_name_similarity(&path1, &path2);
		assert!(similarity > 0.5); // Should be somewhat similar
		assert!(similarity < 1.0);
	}

	#[test]
	fn test_calculate_name_similarity_empty() {
		let path1 = PathBuf::from("");
		let path2 = PathBuf::from("test.txt");

		let similarity = calculate_name_similarity(&path1, &path2);
		assert_eq!(similarity, 0.0);
	}

	#[test]
	fn test_calculate_name_similarity_completely_different() {
		let path1 = PathBuf::from("abc.txt");
		let path2 = PathBuf::from("xyz.doc");

		let similarity = calculate_name_similarity(&path1, &path2);
		assert!(similarity < 0.5); // Should be low similarity
	}
}
