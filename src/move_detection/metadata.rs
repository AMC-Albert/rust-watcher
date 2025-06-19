use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::time::Instant;

/// Cached metadata for a file that we've seen before
#[derive(Debug, Clone)]
pub struct FileMetadata {
	pub size: Option<u64>,
	pub windows_id: Option<u64>,
	pub last_seen: Instant,
}

impl FileMetadata {
	pub fn new(size: Option<u64>, windows_id: Option<u64>) -> Self {
		Self {
			size,
			windows_id,
			last_seen: Instant::now(),
		}
	}
}

/// Cache for storing metadata of recently seen files
#[derive(Debug, Default)]
pub struct MetadataCache {
	cache: HashMap<PathBuf, FileMetadata>,
}

impl MetadataCache {
	pub fn new() -> Self {
		Self::default()
	}

	/// Insert or update metadata for a path
	pub fn insert(&mut self, path: PathBuf, metadata: FileMetadata) {
		self.cache.insert(path, metadata);
	}

	/// Get metadata for a path
	pub fn get(&self, path: &Path) -> Option<&FileMetadata> {
		self.cache.get(path)
	}

	/// Remove and return metadata for a path
	pub fn remove(&mut self, path: &Path) -> Option<FileMetadata> {
		self.cache.remove(path)
	}

	/// Check if metadata exists for a path
	pub fn contains(&self, path: &Path) -> bool {
		self.cache.contains_key(path)
	}

	/// Get all cached paths (useful for path type inference)
	pub fn paths(&self) -> impl Iterator<Item = &PathBuf> {
		self.cache.keys()
	}

	/// Clear old entries based on age
	pub fn cleanup_old_entries(&mut self, max_age: std::time::Duration) {
		let cutoff = Instant::now() - max_age;
		self.cache.retain(|_, metadata| metadata.last_seen > cutoff);
	}

	/// Get the number of cached entries
	pub fn len(&self) -> usize {
		self.cache.len()
	}

	/// Check if the cache is empty
	pub fn is_empty(&self) -> bool {
		self.cache.is_empty()
	}

	/// Clear all cached metadata
	pub fn clear(&mut self) {
		self.cache.clear();
	}
}
