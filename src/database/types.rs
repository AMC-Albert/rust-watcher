//! Type definitions for database storage

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// A filesystem event record stored in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
	/// Unique identifier for this event
	pub event_id: Uuid,

	/// Type of filesystem event
	pub event_type: String,

	/// Path of the file/directory
	pub path: PathBuf,

	/// When the event occurred
	pub timestamp: DateTime<Utc>,

	/// Whether this is a directory
	pub is_directory: bool,

	/// File size (if available and applicable)
	pub size: Option<u64>,

	/// File inode (Unix systems)
	pub inode: Option<u64>,

	/// Windows file ID
	pub windows_id: Option<u64>,

	/// Content hash for small files
	pub content_hash: Option<String>,

	/// Move detection confidence (for move events)
	pub confidence: Option<f32>,

	/// Detection method used (for move events)
	pub detection_method: Option<String>,

	/// Time-to-live for automatic cleanup
	pub expires_at: DateTime<Utc>,
}

impl EventRecord {
	/// Create a new event record with automatic expiration
	pub fn new(
		event_type: String,
		path: PathBuf,
		is_directory: bool,
		retention_duration: chrono::Duration,
	) -> Self {
		let now = Utc::now();
		Self {
			event_id: Uuid::new_v4(),
			event_type,
			path,
			timestamp: now,
			is_directory,
			size: None,
			inode: None,
			windows_id: None,
			content_hash: None,
			confidence: None,
			detection_method: None,
			expires_at: now + retention_duration,
		}
	}

	/// Check if this record has expired
	pub fn is_expired(&self) -> bool {
		Utc::now() > self.expires_at
	}

	/// Update the expiration time
	pub fn extend_expiration(&mut self, additional_duration: chrono::Duration) {
		self.expires_at += additional_duration;
	}
}

/// File metadata record for caching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataRecord {
	/// Path of the file/directory
	pub path: PathBuf,

	/// File size
	pub size: Option<u64>,

	/// File inode (Unix systems)
	pub inode: Option<u64>,

	/// Windows file ID
	pub windows_id: Option<u64>,

	/// Content hash for small files
	pub content_hash: Option<String>,

	/// When this metadata was cached
	pub cached_at: DateTime<Utc>,

	/// Whether this is a directory
	pub is_directory: bool,

	/// Last modified time
	pub modified_at: Option<DateTime<Utc>>,
}

impl MetadataRecord {
	/// Create a new metadata record
	pub fn new(path: PathBuf, is_directory: bool) -> Self {
		Self {
			path,
			size: None,
			inode: None,
			windows_id: None,
			content_hash: None,
			cached_at: Utc::now(),
			is_directory,
			modified_at: None,
		}
	}

	/// Check if this metadata cache entry is stale
	pub fn is_stale(&self, max_age: chrono::Duration) -> bool {
		Utc::now() - self.cached_at > max_age
	}
}

/// Storage key types for efficient database indexing
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StorageKey {
	/// Key by event ID
	EventId(Uuid),

	/// Key by path hash
	PathHash(u64),

	/// Key by size bucket (for efficient size-based lookups)
	SizeBucket(u64),

	/// Key by inode
	Inode(u64),

	/// Key by Windows file ID
	WindowsId(u64),

	/// Key by content hash
	ContentHash(String),

	/// Key by timestamp bucket (for time-based queries)
	TimeBucket(i64),

	/// Key by path prefix (for directory-based queries)
	PathPrefix(String),
}

impl StorageKey {
	/// Convert to bytes for database storage
	pub fn to_bytes(&self) -> Vec<u8> {
		bincode::serialize(self).unwrap_or_default()
	}

	/// Create from bytes
	pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode::Error> {
		bincode::deserialize(bytes)
	}

	/// Generate a size bucket key for efficient size-based grouping
	pub fn size_bucket(size: u64) -> Self {
		// Group files into size buckets: 0-1KB, 1-10KB, 10-100KB, etc.
		let bucket = if size == 0 {
			0
		} else {
			let log_size = (size as f64).log10() as u64;
			10_u64.pow(log_size as u32)
		};
		Self::SizeBucket(bucket)
	}

	/// Generate a time bucket key for efficient time-based queries
	pub fn time_bucket(timestamp: DateTime<Utc>, bucket_size_seconds: i64) -> Self {
		let timestamp_seconds = timestamp.timestamp();
		let bucket = (timestamp_seconds / bucket_size_seconds) * bucket_size_seconds;
		Self::TimeBucket(bucket)
	}

	/// Generate a path hash key
	pub fn path_hash(path: &Path) -> Self {
		use std::collections::hash_map::DefaultHasher;
		use std::hash::{Hash, Hasher};

		let mut hasher = DefaultHasher::new();
		path.hash(&mut hasher);
		Self::PathHash(hasher.finish())
	}

	/// Generate a path prefix key for directory queries
	pub fn path_prefix(path: &Path, max_depth: usize) -> Self {
		let components: Vec<_> = path
			.components()
			.take(max_depth)
			.map(|c| c.as_os_str().to_string_lossy())
			.collect();
		Self::PathPrefix(components.join("/"))
	}
}

/// Statistics about database usage
#[derive(Debug, Clone, Default)]
pub struct DatabaseStats {
	/// Total number of events stored
	pub total_events: u64,

	/// Total number of metadata records
	pub total_metadata: u64,

	/// Database file size in bytes
	pub database_size: u64,

	/// Number of read operations
	pub read_operations: u64,

	/// Number of write operations
	pub write_operations: u64,

	/// Number of delete operations
	pub delete_operations: u64,

	/// Cache hit rate (0.0 to 1.0)
	pub cache_hit_rate: f32,

	/// Average query time in milliseconds
	pub avg_query_time_ms: f32,

	/// Number of expired events cleaned up
	pub cleaned_up_events: u64,
}

impl DatabaseStats {
	/// Calculate efficiency metrics
	pub fn efficiency_score(&self) -> f32 {
		// Combine cache hit rate and query performance into a score
		let cache_score = self.cache_hit_rate;
		let query_score = if self.avg_query_time_ms > 0.0 {
			(100.0 / self.avg_query_time_ms).min(1.0)
		} else {
			1.0
		};
		(cache_score + query_score) / 2.0
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use chrono::Duration;

	#[test]
	fn test_event_record_creation() {
		let path = PathBuf::from("/test/file.txt");
		let retention = Duration::hours(1);
		let record = EventRecord::new("Create".to_string(), path.clone(), false, retention);

		assert_eq!(record.event_type, "Create");
		assert_eq!(record.path, path);
		assert!(!record.is_directory);
		assert!(!record.is_expired());
	}

	#[test]
	fn test_storage_key_size_buckets() {
		assert_eq!(StorageKey::size_bucket(0), StorageKey::SizeBucket(0));
		assert_eq!(StorageKey::size_bucket(500), StorageKey::SizeBucket(100));
		assert_eq!(StorageKey::size_bucket(5000), StorageKey::SizeBucket(1000));
		assert_eq!(
			StorageKey::size_bucket(50000),
			StorageKey::SizeBucket(10000)
		);
	}

	#[test]
	fn test_storage_key_serialization() {
		let key = StorageKey::EventId(Uuid::new_v4());
		let bytes = key.to_bytes();
		let recovered = StorageKey::from_bytes(&bytes).unwrap();
		assert_eq!(key, recovered);
	}

	#[test]
	fn test_metadata_record() {
		let path = PathBuf::from("/test/dir");
		let record = MetadataRecord::new(path.clone(), true);

		assert_eq!(record.path, path);
		assert!(record.is_directory);
		assert!(!record.is_stale(Duration::hours(1)));
	}

	#[test]
	fn test_database_stats() {
		let stats = DatabaseStats {
			cache_hit_rate: 0.8,
			avg_query_time_ms: 50.0,
			..Default::default()
		};

		let efficiency = stats.efficiency_score();
		assert!(efficiency > 0.0 && efficiency <= 1.0);
	}
}
