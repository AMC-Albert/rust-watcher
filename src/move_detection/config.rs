use std::time::Duration;

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
			content_hash_max_file_size: 1024 * 1024, // 1MB
		}
	}
}

impl MoveDetectorConfig {
	/// Create a new configuration with custom timeout
	pub fn with_timeout(timeout_ms: u64) -> Self {
		Self {
			timeout: Duration::from_millis(timeout_ms),
			..Default::default()
		}
	}

	/// Validate the configuration and return errors if invalid
	pub fn validate(&self) -> Result<(), String> {
		if self.confidence_threshold < 0.0 || self.confidence_threshold > 1.0 {
			return Err("confidence_threshold must be between 0.0 and 1.0".to_string());
		}

		if self.max_pending_events == 0 {
			return Err("max_pending_events must be greater than 0".to_string());
		}

		// Check that weights sum to approximately 1.0 (allow some tolerance)
		let total_weight = self.weight_size_match
			+ self.weight_time_factor
			+ self.weight_inode_match
			+ self.weight_content_hash
			+ self.weight_name_similarity;

		if (total_weight - 1.0).abs() > 0.1 {
			return Err(format!(
				"Weights should sum to approximately 1.0, got {:.2}",
				total_weight
			));
		}

		Ok(())
	}
}
