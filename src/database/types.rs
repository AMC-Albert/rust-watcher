//! Type definitions for database storage

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use uuid::Uuid;

/// A filesystem event record stored in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
	/// Unique identifier for this event
	pub event_id: Uuid,

	/// Strictly increasing sequence number for append order
	pub sequence_number: u64,

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
		event_type: String, path: PathBuf, is_directory: bool,
		retention_duration: chrono::Duration, sequence_number: u64,
	) -> Self {
		let now = Utc::now();
		Self {
			event_id: Uuid::new_v4(),
			sequence_number,
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

/// A cached filesystem node with complete metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemNode {
	/// Canonical absolute path
	pub path: PathBuf,

	/// Node type and metadata
	pub node_type: NodeType,

	/// File system metadata
	pub metadata: NodeMetadata,

	/// Caching metadata
	pub cache_info: CacheInfo,

	/// Computed properties
	pub computed: ComputedProperties,

	/// Event type that created or last mutated this node (for repair/stats)
	pub last_event_type: Option<String>, // None for legacy nodes, Some for new/updated nodes
}

/// Type of filesystem node
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeType {
	File {
		size: u64,
		content_hash: Option<String>,
		mime_type: Option<String>,
	},
	Directory {
		child_count: u32,
		total_size: u64,
		max_depth: u16,
	},
	Symlink {
		target: PathBuf,
		resolved: Option<PathBuf>,
	},
}

/// Filesystem metadata from the OS
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeMetadata {
	pub modified_time: SystemTime,
	pub created_time: Option<SystemTime>,
	pub accessed_time: Option<SystemTime>,
	pub permissions: u32,
	pub inode: Option<u64>,
	pub windows_id: Option<u64>,
}

/// Cache-specific metadata
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CacheInfo {
	pub cached_at: DateTime<Utc>,
	pub last_verified: DateTime<Utc>,
	pub cache_version: u32,
	pub needs_refresh: bool,
}

/// Computed properties for performance
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ComputedProperties {
	pub depth_from_root: u16,
	pub path_hash: u64,
	pub parent_hash: Option<u64>,
	pub canonical_name: String,
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
///
/// NOTE: For scalable stats, total_events should be loaded from a persistent counter in STATS_TABLE (see EVENT_COUNT_KEY).
/// This enables O(1) stats queries. All event insert/delete operations must update the counter transactionally.
/// Edge case: If the counter drifts (e.g., after crash or migration), a full scan and resync is required.
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

	/// Per-event-type counts (e.g., {"create": 123, "delete": 45})
	pub per_type_counts: std::collections::HashMap<String, u64>,
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

/// Calculate a consistent hash for a path
pub fn calculate_path_hash(path: &Path) -> u64 {
	use std::collections::hash_map::DefaultHasher;
	use std::hash::{Hash, Hasher};

	let mut hasher = DefaultHasher::new();
	// Normalize path for consistent hashing across platforms
	path.to_string_lossy().to_lowercase().hash(&mut hasher);
	hasher.finish()
}

/// Update the existing StorageKey enum with filesystem cache variants
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExtendedStorageKey {
	/// Original event storage
	Event(String),

	/// Filesystem cache entries
	FilesystemNode(FilesystemKey),

	/// Watch metadata
	WatchMetadata(Uuid),

	/// Hierarchy relationships
	Hierarchy(u64), // parent hash

	/// Shared node references
	SharedNode(u64), // path hash
}

/// Filesystem storage key variants
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FilesystemKey {
	/// Direct path lookup (most common)
	Path(PathBuf),

	/// Hash-based lookup for performance
	PathHash(u64),

	/// Inode lookup (Unix systems)
	Inode(u64),

	/// Windows file ID lookup
	WindowsId(u64),

	/// Parent directory lookup
	ParentPath(PathBuf),

	/// Prefix lookup for subtree operations
	PathPrefix(String),

	/// Depth-based lookup for tree traversal
	DepthLevel(u16),
}

/// Shared node information across multiple watches
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedNodeInfo {
	pub node: FilesystemNode,
	pub watching_scopes: Vec<Uuid>,
	pub reference_count: u32,
	pub last_shared_update: DateTime<Utc>,
}

/// Watch permissions for per-watch access control
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WatchPermissions {
	pub can_read: bool,
	pub can_write: bool,
	pub can_delete: bool,
	pub can_manage: bool, // e.g., add/remove watches
}

/// Watch metadata for multi-watch management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchMetadata {
	pub watch_id: Uuid,
	pub root_path: PathBuf,
	pub created_at: DateTime<Utc>,
	pub last_scan: Option<DateTime<Utc>>,
	pub node_count: u64,
	pub is_active: bool,
	pub config_hash: u64,
	pub permissions: Option<WatchPermissions>, // Optional for backward compatibility
}

/// Unified node that can represent shared or watch-specific data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UnifiedNode {
	/// Node specific to a single watch
	WatchSpecific {
		watch_id: Uuid,
		node: FilesystemNode,
	},
	/// Node shared across multiple watches
	Shared { shared_info: SharedNodeInfo },
}

impl FilesystemNode {
	/// Create a new filesystem node from standard metadata
	pub fn new_with_event_type(
		path: PathBuf, metadata: &std::fs::Metadata, event_type: Option<String>,
	) -> Self {
		let now = Utc::now();
		let path_hash = calculate_path_hash(&path);
		let parent_hash = path.parent().map(calculate_path_hash);

		let node_type = if metadata.is_dir() {
			NodeType::Directory {
				child_count: 0, // Will be populated during scanning
				total_size: 0,
				max_depth: 0,
			}
		} else if metadata.file_type().is_symlink() {
			NodeType::Symlink {
				target: PathBuf::new(), // Will be resolved
				resolved: None,
			}
		} else {
			NodeType::File {
				size: metadata.len(),
				content_hash: None, // Computed on demand
				mime_type: None,
			}
		};

		Self {
			path: path.clone(),
			node_type,
			metadata: NodeMetadata::from_std_metadata(metadata),
			cache_info: CacheInfo {
				cached_at: now,
				last_verified: now,
				cache_version: 1,
				needs_refresh: false,
			},
			computed: ComputedProperties {
				depth_from_root: path.components().count() as u16,
				path_hash,
				parent_hash,
				canonical_name: path.file_name().unwrap_or_default().to_string_lossy().to_string(),
			},
			last_event_type: event_type,
		}
	}

	/// Backward-compatible constructor for legacy code
	pub fn new(path: PathBuf, metadata: &std::fs::Metadata) -> Self {
		Self::new_with_event_type(path, metadata, None)
	}

	/// Check if the node needs to be refreshed based on timestamp
	pub fn needs_refresh(&self, max_age: std::time::Duration) -> bool {
		let age = Utc::now().signed_duration_since(self.cache_info.last_verified);
		age.to_std().unwrap_or_default() > max_age || self.cache_info.needs_refresh
	}

	/// Mark the node as needing refresh
	pub fn mark_stale(&mut self) {
		self.cache_info.needs_refresh = true;
	}

	/// Update the verification timestamp
	pub fn mark_verified(&mut self) {
		self.cache_info.last_verified = Utc::now();
		self.cache_info.needs_refresh = false;
	}
}

impl NodeMetadata {
	/// Convert from standard library metadata
	pub fn from_std_metadata(metadata: &std::fs::Metadata) -> Self {
		Self {
			modified_time: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
			created_time: metadata.created().ok(),
			accessed_time: metadata.accessed().ok(),
			permissions: 0, // Platform-specific implementation needed
			inode: None,    // Platform-specific implementation needed
			windows_id: None,
		}
	}
}

impl Default for NodeMetadata {
	fn default() -> Self {
		Self {
			modified_time: std::time::SystemTime::UNIX_EPOCH,
			created_time: Some(std::time::SystemTime::UNIX_EPOCH),
			accessed_time: Some(std::time::SystemTime::UNIX_EPOCH),
			permissions: 0,
			inode: None,
			windows_id: None,
		}
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
		let record = EventRecord::new("Create".to_string(), path.clone(), false, retention, 1);

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
		let stats =
			DatabaseStats { cache_hit_rate: 0.8, avg_query_time_ms: 50.0, ..Default::default() };

		let efficiency = stats.efficiency_score();
		assert!(efficiency > 0.0 && efficiency <= 1.0);
	}
	#[test]
	fn test_filesystem_node_creation() {
		use std::fs;
		use std::time::Duration;
		use tempfile::TempDir;

		let temp_dir = TempDir::new().unwrap();
		let test_file = temp_dir.path().join("test.txt");
		fs::write(&test_file, "test content").unwrap();

		let metadata = fs::metadata(&test_file).unwrap();
		let node = FilesystemNode::new(test_file.clone(), &metadata);

		assert_eq!(node.path, test_file);
		assert!(matches!(node.node_type, NodeType::File { .. }));
		assert!(!node.needs_refresh(Duration::from_secs(3600)));
	}

	#[test]
	fn test_path_hash_consistency() {
		let path1 = PathBuf::from("/test/path");
		let path2 = PathBuf::from("/test/path");

		assert_eq!(calculate_path_hash(&path1), calculate_path_hash(&path2));
	}

	#[test]
	fn test_node_refresh_logic() {
		use std::fs;
		use std::time::Duration;
		use tempfile::TempDir;

		let temp_dir = TempDir::new().unwrap();
		let test_file = temp_dir.path().join("test.txt");
		fs::write(&test_file, "test content").unwrap();

		let metadata = fs::metadata(&test_file).unwrap();
		let mut node = FilesystemNode::new(test_file, &metadata);

		// Fresh node shouldn't need refresh
		assert!(!node.needs_refresh(Duration::from_secs(3600)));

		// Mark as stale
		node.mark_stale();
		assert!(node.needs_refresh(Duration::from_secs(3600)));

		// Mark as verified
		node.mark_verified();
		assert!(!node.needs_refresh(Duration::from_secs(3600)));
	}
}

/// Key type for scoping cache entries to a specific watch
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WatchScopedKey {
	pub watch_id: Uuid,
	pub path_hash: u64,
}

/// Helper to build a per-event-type stat key (e.g., b"event_type:create")
pub fn event_type_stat_key(event_type: &str) -> Vec<u8> {
	let mut key = b"event_type:".to_vec();
	key.extend_from_slice(event_type.as_bytes());
	key
}
