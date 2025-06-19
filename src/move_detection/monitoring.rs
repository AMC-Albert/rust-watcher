use crate::move_detection::events::PendingEventsStorage;
use crate::move_detection::metadata::MetadataCache;

/// Statistics about resource usage and performance
#[derive(Debug, Clone)]
pub struct ResourceStats {
	pub pending_removes: usize,
	pub pending_creates: usize,
	pub cached_metadata_entries: usize,
	pub memory_usage_estimate_bytes: usize,
	pub total_events_processed: u64,
	pub moves_detected: u64,
	pub confidence_sum: f64,
	pub average_confidence: f32,
}

impl ResourceStats {
	pub fn new() -> Self {
		Self {
			pending_removes: 0,
			pending_creates: 0,
			cached_metadata_entries: 0,
			memory_usage_estimate_bytes: 0,
			total_events_processed: 0,
			moves_detected: 0,
			confidence_sum: 0.0,
			average_confidence: 0.0,
		}
	}

	/// Calculate estimated memory usage based on data structures
	pub fn calculate_memory_estimate(
		pending_events: &PendingEventsStorage,
		metadata_cache: &MetadataCache,
	) -> usize {
		// Rough estimates based on typical struct sizes
		let pending_event_size = 200; // bytes per PendingEvent
		let metadata_size = 100; // bytes per FileMetadata entry
		let hashmap_overhead = 50; // bytes per HashMap entry

		let pending_removes = pending_events.count_removes();
		let pending_creates = pending_events.count_creates();
		let metadata_entries = metadata_cache.len();

		(pending_removes + pending_creates) * (pending_event_size + hashmap_overhead)
			+ metadata_entries * (metadata_size + hashmap_overhead)
	}

	/// Update statistics based on current state
	pub fn update(
		&mut self,
		pending_events: &PendingEventsStorage,
		metadata_cache: &MetadataCache,
	) {
		self.pending_removes = pending_events.count_removes();
		self.pending_creates = pending_events.count_creates();
		self.cached_metadata_entries = metadata_cache.len();
		self.memory_usage_estimate_bytes =
			Self::calculate_memory_estimate(pending_events, metadata_cache);

		if self.moves_detected > 0 {
			self.average_confidence = (self.confidence_sum / self.moves_detected as f64) as f32;
		}
	}

	/// Record a processed event
	pub fn record_event_processed(&mut self) {
		self.total_events_processed += 1;
	}

	/// Record a detected move with its confidence
	pub fn record_move_detected(&mut self, confidence: f32) {
		self.moves_detected += 1;
		self.confidence_sum += confidence as f64;
		self.average_confidence = (self.confidence_sum / self.moves_detected as f64) as f32;
	}

	/// Check if resource usage is concerning
	pub fn is_resource_usage_high(&self, max_pending: usize) -> bool {
		(self.pending_removes + self.pending_creates) > max_pending * 80 / 100 // 80% of limit
	}

	/// Get memory usage in MB for easier reading
	pub fn memory_usage_mb(&self) -> f32 {
		self.memory_usage_estimate_bytes as f32 / (1024.0 * 1024.0)
	}
}

impl Default for ResourceStats {
	fn default() -> Self {
		Self::new()
	}
}

/// Summary of pending events for debugging and monitoring
#[derive(Debug, Clone)]
pub struct PendingEventsSummary {
	pub removes_by_size_buckets: usize,
	pub removes_no_size: usize,
	pub removes_by_inode: usize,
	pub removes_by_windows_id: usize,
	pub creates_by_size_buckets: usize,
	pub creates_no_size: usize,
	pub creates_by_inode: usize,
	pub creates_by_windows_id: usize,
	pub has_pending_rename_from: bool,
}

impl PendingEventsSummary {
	pub fn from_storage(storage: &PendingEventsStorage) -> Self {
		Self {
			removes_by_size_buckets: storage.removes_by_size.len(),
			removes_no_size: storage.removes_no_size.len(),
			removes_by_inode: storage.removes_by_inode.len(),
			removes_by_windows_id: storage.removes_by_windows_id.len(),
			creates_by_size_buckets: storage.creates_by_size.len(),
			creates_no_size: storage.creates_no_size.len(),
			creates_by_inode: storage.creates_by_inode.len(),
			creates_by_windows_id: storage.creates_by_windows_id.len(),
			has_pending_rename_from: storage.pending_rename_from.is_some(),
		}
	}

	pub fn total_removes(&self) -> usize {
		// Note: This counts buckets, not individual events
		// For accurate counts, use PendingEventsStorage::count_removes()
		self.removes_by_size_buckets
			+ self.removes_no_size
			+ self.removes_by_inode
			+ self.removes_by_windows_id
	}

	pub fn total_creates(&self) -> usize {
		// Note: This counts buckets, not individual events
		// For accurate counts, use PendingEventsStorage::count_creates()
		self.creates_by_size_buckets
			+ self.creates_no_size
			+ self.creates_by_inode
			+ self.creates_by_windows_id
	}
}
