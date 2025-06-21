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
		// All weights must sum to 1.0
		#[cfg(unix)]
		let (
			confidence_threshold,
			weight_inode,
			weight_size,
			weight_name,
			weight_time,
			weight_content,
		) = (0.7, 0.35, 0.2, 0.1, 0.15, 0.2);

		#[cfg(windows)]
		let (
			confidence_threshold,
			weight_inode,
			weight_size,
			weight_name,
			weight_time,
			weight_content,
		) = (0.5, 0.25, 0.25, 0.2, 0.2, 0.1); // Lower threshold, higher name/time weights

		#[cfg(not(any(unix, windows)))]
		let (
			confidence_threshold,
			weight_inode,
			weight_size,
			weight_name,
			weight_time,
			weight_content,
		) = (0.5, 0.25, 0.25, 0.2, 0.2, 0.1);

		Self {
			timeout: Duration::from_millis(1000),
			confidence_threshold,
			weight_size_match: weight_size,
			weight_time_factor: weight_time,
			weight_inode_match: weight_inode,
			weight_content_hash: weight_content,
			weight_name_similarity: weight_name,
			max_pending_events: 1000,
			content_hash_max_file_size: 1024 * 1024, // 1MB
		}
	}
}

impl MoveDetectorConfig {
	/// Create a new configuration with custom timeout
	pub fn with_timeout(timeout_ms: u64) -> Self {
		Self { timeout: Duration::from_millis(timeout_ms), ..Default::default() }
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
				"Weights should sum to approximately 1.0, got {total_weight:.2}"
			));
		}

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_default_config() {
		let config = MoveDetectorConfig::default();

		// Test that default values are reasonable
		assert!(config.timeout.as_millis() > 0);
		assert!(config.confidence_threshold >= 0.0 && config.confidence_threshold <= 1.0);
		assert!(config.max_pending_events > 0);
		assert!(config.content_hash_max_file_size > 0);

		// Test that weights are positive
		assert!(config.weight_size_match >= 0.0);
		assert!(config.weight_time_factor >= 0.0);
		assert!(config.weight_inode_match >= 0.0);
		assert!(config.weight_content_hash >= 0.0);
		assert!(config.weight_name_similarity >= 0.0);
	}

	#[test]
	fn test_with_timeout() {
		let config = MoveDetectorConfig::with_timeout(2000);
		assert_eq!(config.timeout, Duration::from_millis(2000));

		// Other values should be default
		let default_config = MoveDetectorConfig::default();
		assert_eq!(
			config.confidence_threshold,
			default_config.confidence_threshold
		);
		assert_eq!(config.max_pending_events, default_config.max_pending_events);
	}

	#[test]
	fn test_config_validation_success() {
		let config = MoveDetectorConfig::default();
		assert!(config.validate().is_ok());
	}
	#[test]
	fn test_config_validation_confidence_threshold() {
		// Test invalid confidence threshold
		let config = MoveDetectorConfig { confidence_threshold: -0.1, ..Default::default() };
		assert!(config.validate().is_err());

		let config = MoveDetectorConfig { confidence_threshold: 1.1, ..Default::default() };
		assert!(config.validate().is_err());

		// Test valid thresholds
		let config = MoveDetectorConfig { confidence_threshold: 0.0, ..Default::default() };
		assert!(config.validate().is_ok());

		let config = MoveDetectorConfig { confidence_threshold: 1.0, ..Default::default() };
		assert!(config.validate().is_ok());

		let config = MoveDetectorConfig { confidence_threshold: 0.5, ..Default::default() };
		assert!(config.validate().is_ok());
	}
	#[test]
	fn test_config_validation_max_pending_events() {
		let config = MoveDetectorConfig { max_pending_events: 0, ..Default::default() };
		assert!(config.validate().is_err());

		let config = MoveDetectorConfig { max_pending_events: 1, ..Default::default() };
		assert!(config.validate().is_ok());
	}
	#[test]
	fn test_config_validation_weights() {
		// Modify weights to sum to something far from 1.0
		let config = MoveDetectorConfig {
			weight_size_match: 0.5,
			weight_time_factor: 0.5,
			weight_inode_match: 0.5,
			weight_content_hash: 0.5,
			weight_name_similarity: 0.5,
			..Default::default()
		};
		// Sum = 2.5, should fail
		assert!(config.validate().is_err());

		// Set weights to sum to 1.0
		let config = MoveDetectorConfig {
			weight_size_match: 0.2,
			weight_time_factor: 0.2,
			weight_inode_match: 0.2,
			weight_content_hash: 0.2,
			weight_name_similarity: 0.2,
			..Default::default()
		};
		// Sum = 1.0, should pass
		assert!(config.validate().is_ok());
	}

	#[test]
	fn test_platform_specific_defaults() {
		let config = MoveDetectorConfig::default();

		// Just test that platform-specific defaults are applied
		// The exact values depend on the platform, but they should be valid
		assert!(config.validate().is_ok());

		// On all platforms, weights should be reasonable
		let total_weight = config.weight_size_match
			+ config.weight_time_factor
			+ config.weight_inode_match
			+ config.weight_content_hash
			+ config.weight_name_similarity;

		assert!(
			(total_weight - 1.0).abs() <= 0.1,
			"Total weight should be close to 1.0, got {total_weight}",
		);
	}
}
