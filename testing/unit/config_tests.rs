#[cfg(test)]
mod tests {
	use rust_watcher::{MoveDetectorConfig, WatcherConfig};
	use std::path::PathBuf;
	use std::time::Duration;
	use tempfile::TempDir;
	#[test]
	fn test_move_detector_config_defaults() {
		let config = MoveDetectorConfig::default();

		assert_eq!(config.timeout, Duration::from_millis(1000));
		// Platform-specific defaults
		#[cfg(windows)]
		{
			assert_eq!(config.confidence_threshold, 0.5);
			assert_eq!(config.weight_size_match, 0.25);
			assert_eq!(config.weight_time_factor, 0.2);
			assert_eq!(config.weight_inode_match, 0.3);
			assert_eq!(config.weight_name_similarity, 0.2);
		}
		#[cfg(unix)]
		{
			assert_eq!(config.confidence_threshold, 0.7);
			assert_eq!(config.weight_size_match, 0.2);
			assert_eq!(config.weight_time_factor, 0.15);
			assert_eq!(config.weight_inode_match, 0.4);
			assert_eq!(config.weight_name_similarity, 0.1);
		}
		#[cfg(not(any(unix, windows)))]
		{
			assert_eq!(config.confidence_threshold, 0.5);
			assert_eq!(config.weight_size_match, 0.25);
			assert_eq!(config.weight_time_factor, 0.2);
			assert_eq!(config.weight_inode_match, 0.3);
			assert_eq!(config.weight_name_similarity, 0.2);
		}

		assert_eq!(config.weight_content_hash, 0.35);
		assert_eq!(config.max_pending_events, 1000);
		assert_eq!(config.content_hash_max_file_size, 1_048_576);
	}

	#[test]
	fn test_move_detector_config_custom() {
		let config = MoveDetectorConfig {
			timeout: Duration::from_millis(2000),
			confidence_threshold: 0.8,
			weight_size_match: 0.3,
			weight_time_factor: 0.2,
			weight_inode_match: 0.5,
			weight_content_hash: 0.4,
			weight_name_similarity: 0.15,
			max_pending_events: 500,
			content_hash_max_file_size: 2_097_152,
		};

		assert_eq!(config.timeout, Duration::from_millis(2000));
		assert_eq!(config.confidence_threshold, 0.8);
		assert_eq!(config.weight_size_match, 0.3);
		assert_eq!(config.weight_time_factor, 0.2);
		assert_eq!(config.weight_inode_match, 0.5);
		assert_eq!(config.weight_content_hash, 0.4);
		assert_eq!(config.weight_name_similarity, 0.15);
		assert_eq!(config.max_pending_events, 500);
		assert_eq!(config.content_hash_max_file_size, 2_097_152);
	}

	#[test]
	fn test_watcher_config_creation() {
		let temp_dir = TempDir::new().unwrap();

		// Test basic config
		let config = WatcherConfig {
			path: temp_dir.path().to_path_buf(),
			recursive: true,
			move_detector_config: None,
		};

		assert_eq!(config.path, temp_dir.path());
		assert!(config.recursive);
		assert!(config.move_detector_config.is_none());
	}

	#[test]
	fn test_watcher_config_with_move_detector() {
		let temp_dir = TempDir::new().unwrap();
		let move_config = MoveDetectorConfig {
			timeout: Duration::from_millis(1500),
			confidence_threshold: 0.75,
			..Default::default()
		};

		let config = WatcherConfig {
			path: temp_dir.path().to_path_buf(),
			recursive: false,
			move_detector_config: Some(move_config.clone()),
		};

		assert_eq!(config.path, temp_dir.path());
		assert!(!config.recursive);
		assert!(config.move_detector_config.is_some());

		let stored_config = config.move_detector_config.unwrap();
		assert_eq!(stored_config.timeout, Duration::from_millis(1500));
		assert_eq!(stored_config.confidence_threshold, 0.75);
	}
	#[test]
	fn test_move_detector_config_validation_ranges() {
		// Test confidence threshold bounds
		let config_low = MoveDetectorConfig {
			confidence_threshold: 0.0,
			..Default::default()
		};
		assert_eq!(config_low.confidence_threshold, 0.0);

		let config_high = MoveDetectorConfig {
			confidence_threshold: 1.0,
			..Default::default()
		};
		assert_eq!(config_high.confidence_threshold, 1.0);

		// Test weights are reasonable and configured for the platform
		let config = MoveDetectorConfig::default();
		let total_weight = config.weight_size_match
			+ config.weight_time_factor
			+ config.weight_inode_match
			+ config.weight_content_hash
			+ config.weight_name_similarity;
		// Weights should be reasonable (between 1.0 and 1.5 depending on platform)
		// Different platforms have different optimal weight distributions
		assert!(
			(1.0..=1.5).contains(&total_weight),
			"Total weight should be between 1.0 and 1.5, got: {}",
			total_weight
		);

		// All individual weights should be positive and reasonable
		assert!(config.weight_size_match > 0.0 && config.weight_size_match <= 0.5);
		assert!(config.weight_time_factor > 0.0 && config.weight_time_factor <= 0.5);
		assert!(config.weight_inode_match > 0.0 && config.weight_inode_match <= 0.5);
		assert!(config.weight_content_hash > 0.0 && config.weight_content_hash <= 0.5);
		assert!(config.weight_name_similarity > 0.0 && config.weight_name_similarity <= 0.5);
	}

	#[test]
	fn test_move_detector_config_timeout_values() {
		// Test various timeout values
		let configs = vec![
			Duration::from_millis(100),
			Duration::from_millis(1000),
			Duration::from_millis(5000),
			Duration::from_secs(10),
		];

		for timeout in configs {
			let config = MoveDetectorConfig {
				timeout,
				..Default::default()
			};
			assert_eq!(config.timeout, timeout);
		}
	}

	#[test]
	fn test_move_detector_config_max_pending_events() {
		// Test various max pending event values
		let limits = vec![10, 100, 1000, 10000];

		for limit in limits {
			let config = MoveDetectorConfig {
				max_pending_events: limit,
				..Default::default()
			};
			assert_eq!(config.max_pending_events, limit);
		}
	}

	#[test]
	fn test_move_detector_config_content_hash_limits() {
		// Test various file size limits for content hashing
		let limits = vec![
			1024,        // 1KB
			1_048_576,   // 1MB
			10_485_760,  // 10MB
			104_857_600, // 100MB
		];

		for limit in limits {
			let config = MoveDetectorConfig {
				content_hash_max_file_size: limit,
				..Default::default()
			};
			assert_eq!(config.content_hash_max_file_size, limit);
		}
	}

	#[test]
	fn test_watcher_config_path_validation() {
		// Test with different path types
		let temp_dir = TempDir::new().unwrap();

		// Absolute path
		let config1 = WatcherConfig {
			path: temp_dir.path().to_path_buf(),
			recursive: true,
			move_detector_config: None,
		};
		assert!(config1.path.is_absolute());

		// Relative path
		let config2 = WatcherConfig {
			path: PathBuf::from("./test"),
			recursive: true,
			move_detector_config: None,
		};
		assert!(!config2.path.is_absolute());
	}

	#[test]
	fn test_move_detector_config_clone() {
		let original = MoveDetectorConfig {
			timeout: Duration::from_millis(1500),
			confidence_threshold: 0.8,
			max_pending_events: 500,
			..Default::default()
		};

		let cloned = original.clone();

		assert_eq!(original.timeout, cloned.timeout);
		assert_eq!(original.confidence_threshold, cloned.confidence_threshold);
		assert_eq!(original.max_pending_events, cloned.max_pending_events);
	}

	#[test]
	fn test_move_detector_config_debug() {
		let config = MoveDetectorConfig::default();
		let debug_output = format!("{:?}", config);

		// Should contain key configuration values
		assert!(debug_output.contains("timeout"));
		assert!(debug_output.contains("confidence_threshold"));
		assert!(debug_output.contains("max_pending_events"));
	}

	#[test]
	fn test_weight_configuration_edge_cases() {
		// Test with zero weights
		let config_zero_weights = MoveDetectorConfig {
			weight_size_match: 0.0,
			weight_time_factor: 0.0,
			weight_inode_match: 0.0,
			weight_content_hash: 0.0,
			weight_name_similarity: 0.0,
			..Default::default()
		};

		// Should not panic with zero weights
		assert_eq!(config_zero_weights.weight_size_match, 0.0);

		// Test with very high weights
		let config_high_weights = MoveDetectorConfig {
			weight_size_match: 10.0,
			weight_time_factor: 10.0,
			weight_inode_match: 10.0,
			weight_content_hash: 10.0,
			weight_name_similarity: 10.0,
			..Default::default()
		};

		// Should accept high weights (might reduce confidence for poor matches)
		assert_eq!(config_high_weights.weight_size_match, 10.0);
	}
}
