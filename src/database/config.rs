//! Database configuration for different scale scenarios

use std::path::PathBuf;
use std::time::Duration;

/// Configuration for database-backed storage
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
	/// Path where the database file will be stored
	pub database_path: PathBuf,

	/// Maximum number of events to keep in memory before flushing to database
	pub memory_buffer_size: usize,

	/// Maximum size of the database file in bytes (0 = unlimited)
	pub max_database_size: u64,

	/// How often to flush pending events to database
	pub flush_interval: Duration,

	/// How long to keep events in the database before cleanup
	pub event_retention: Duration,

	/// Batch size for database operations
	pub write_batch_size: usize,

	/// Cache size for frequently accessed metadata
	pub read_cache_size: usize,

	/// Enable database compression (if supported)
	pub enable_compression: bool,
}

impl DatabaseConfig {
	/// Configuration for small directories (< 10K files)
	/// Uses minimal database overhead
	pub fn for_small_directories() -> Self {
		Self {
			database_path: std::env::temp_dir().join("rust_watcher_small"),
			memory_buffer_size: 1000,
			max_database_size: 100 * 1024 * 1024, // 100MB limit
			flush_interval: Duration::from_secs(30),
			event_retention: Duration::from_secs(300), // 5 minutes
			write_batch_size: 100,
			read_cache_size: 1024,
			enable_compression: false,
		}
	}

	/// Configuration for moderate directories (10K-100K files)
	pub fn for_moderate_directories() -> Self {
		Self {
			database_path: std::env::temp_dir().join("rust_watcher_moderate"),
			memory_buffer_size: 10_000,
			max_database_size: 1024 * 1024 * 1024, // 1GB limit
			flush_interval: Duration::from_secs(60),
			event_retention: Duration::from_secs(600), // 10 minutes
			write_batch_size: 1000,
			read_cache_size: 10_000,
			enable_compression: true,
		}
	}

	/// Configuration for large directories (100K-1M files)
	pub fn for_large_directories() -> Self {
		Self {
			database_path: std::env::temp_dir().join("rust_watcher_large"),
			memory_buffer_size: 50_000,
			max_database_size: 10 * 1024 * 1024 * 1024, // 10GB limit
			flush_interval: Duration::from_secs(120),
			event_retention: Duration::from_secs(1800), // 30 minutes
			write_batch_size: 5000,
			read_cache_size: 50_000,
			enable_compression: true,
		}
	}

	/// Configuration for massive directories (1M+ files)
	/// Optimized for maximum throughput and minimal memory usage
	pub fn for_massive_directories() -> Self {
		Self {
			database_path: std::env::temp_dir().join("rust_watcher_massive"),
			memory_buffer_size: 100_000,
			max_database_size: 0,                       // Unlimited
			flush_interval: Duration::from_secs(300),   // 5 minutes
			event_retention: Duration::from_secs(3600), // 1 hour
			write_batch_size: 10_000,
			read_cache_size: 100_000,
			enable_compression: true,
		}
	}

	/// Custom configuration with specified database path
	pub fn with_path(path: PathBuf) -> Self {
		let mut config = Self::for_moderate_directories();
		config.database_path = path;
		config
	}

	/// Validate configuration parameters
	pub fn validate(&self) -> Result<(), String> {
		if self.memory_buffer_size == 0 {
			return Err("Memory buffer size must be greater than 0".to_string());
		}

		if self.write_batch_size == 0 {
			return Err("Write batch size must be greater than 0".to_string());
		}

		if self.write_batch_size > self.memory_buffer_size {
			return Err("Write batch size cannot be larger than memory buffer size".to_string());
		}

		if self.flush_interval.is_zero() {
			return Err("Flush interval must be greater than 0".to_string());
		}

		if self.event_retention.is_zero() {
			return Err("Event retention must be greater than 0".to_string());
		}

		Ok(())
	}
}

impl Default for DatabaseConfig {
	fn default() -> Self {
		Self::for_moderate_directories()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_predefined_configs() {
		let small = DatabaseConfig::for_small_directories();
		let moderate = DatabaseConfig::for_moderate_directories();
		let large = DatabaseConfig::for_large_directories();
		let massive = DatabaseConfig::for_massive_directories();

		assert!(small.memory_buffer_size < moderate.memory_buffer_size);
		assert!(moderate.memory_buffer_size < large.memory_buffer_size);
		assert!(large.memory_buffer_size < massive.memory_buffer_size);

		// All configs should be valid
		assert!(small.validate().is_ok());
		assert!(moderate.validate().is_ok());
		assert!(large.validate().is_ok());
		assert!(massive.validate().is_ok());
	}

	#[test]
	fn test_config_validation() {
		let mut config = DatabaseConfig::default();

		// Valid config should pass
		assert!(config.validate().is_ok());

		// Invalid memory buffer size
		config.memory_buffer_size = 0;
		assert!(config.validate().is_err());
		config.memory_buffer_size = 1000;

		// Invalid write batch size
		config.write_batch_size = 0;
		assert!(config.validate().is_err());
		config.write_batch_size = 2000; // Larger than memory buffer
		assert!(config.validate().is_err());
	}

	#[test]
	fn test_custom_path() {
		let custom_path = PathBuf::from("/custom/database/path");
		let config = DatabaseConfig::with_path(custom_path.clone());
		assert_eq!(config.database_path, custom_path);
	}
}
