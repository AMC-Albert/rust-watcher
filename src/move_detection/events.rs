use crate::events::FileSystemEvent;
use std::collections::HashMap;
use tokio::time::Instant;

/// A filesystem event that is pending matching for move detection
#[derive(Debug, Clone)]
pub struct PendingEvent {
	pub event: FileSystemEvent,
	pub timestamp: Instant,
	pub inode: Option<u64>,
	pub content_hash: Option<String>,
	/// Windows-specific identifier (creation_time_nanos << 32 | size_lower_32_bits)
	pub windows_id: Option<u64>,
}

impl PendingEvent {
	pub fn new(event: FileSystemEvent) -> Self {
		Self {
			event,
			timestamp: Instant::now(),
			inode: None,
			content_hash: None,
			windows_id: None,
		}
	}

	pub fn with_inode(mut self, inode: Option<u64>) -> Self {
		self.inode = inode;
		self
	}

	pub fn with_content_hash(mut self, hash: Option<String>) -> Self {
		self.content_hash = hash;
		self
	}

	pub fn with_windows_id(mut self, windows_id: Option<u64>) -> Self {
		self.windows_id = windows_id;
		self
	}
}

/// Storage for pending events organized for efficient lookups
#[derive(Debug, Default)]
pub struct PendingEventsStorage {
	// Bucketed pending removes for O(1) size-based lookups
	pub removes_by_size: HashMap<u64, Vec<PendingEvent>>,
	pub removes_no_size: Vec<PendingEvent>,
	pub removes_by_inode: HashMap<u64, PendingEvent>, // Unix only
	pub removes_by_windows_id: HashMap<u64, PendingEvent>, // Windows only

	// Bucketed pending creates for O(1) size-based lookups
	pub creates_by_size: HashMap<u64, Vec<PendingEvent>>,
	pub creates_no_size: Vec<PendingEvent>,
	pub creates_by_inode: HashMap<u64, PendingEvent>, // Unix only
	pub creates_by_windows_id: HashMap<u64, PendingEvent>, // Windows only

	// Rename event pairing (Windows sends Name(From) then Name(To))
	pub pending_rename_from: Option<(FileSystemEvent, Instant)>,
}

impl PendingEventsStorage {
	pub fn new() -> Self {
		Self::default()
	}

	/// Add a pending remove event to the appropriate bucket
	pub fn add_remove(&mut self, event: PendingEvent) {
		// Store by inode for Unix systems
		if let Some(inode) = event.inode {
			self.removes_by_inode.insert(inode, event.clone());
		}

		// Store by Windows ID for Windows systems
		if let Some(windows_id) = event.windows_id {
			self.removes_by_windows_id.insert(windows_id, event.clone());
		}

		// Store by size for quick size-based matching
		if let Some(size) = event.event.size {
			self.removes_by_size.entry(size).or_default().push(event);
		} else {
			self.removes_no_size.push(event);
		}
	}

	/// Add a pending create event to the appropriate bucket
	pub fn add_create(&mut self, event: PendingEvent) {
		// Store by inode for Unix systems
		if let Some(inode) = event.inode {
			self.creates_by_inode.insert(inode, event.clone());
		}

		// Store by Windows ID for Windows systems
		if let Some(windows_id) = event.windows_id {
			self.creates_by_windows_id.insert(windows_id, event.clone());
		}

		// Store by size for quick size-based matching
		if let Some(size) = event.event.size {
			self.creates_by_size.entry(size).or_default().push(event);
		} else {
			self.creates_no_size.push(event);
		}
	}

	/// Count total pending remove events
	pub fn count_removes(&self) -> usize {
		self.removes_by_size
			.values()
			.map(|v| v.len())
			.sum::<usize>()
			+ self.removes_no_size.len()
	}

	/// Count total pending create events
	pub fn count_creates(&self) -> usize {
		self.creates_by_size
			.values()
			.map(|v| v.len())
			.sum::<usize>()
			+ self.creates_no_size.len()
	}
	/// Remove a pending create event by its ID
	pub fn remove_create_by_id(&mut self, event_id: uuid::Uuid) -> bool {
		// Check inode-based storage
		if let Some(position) = self
			.creates_by_inode
			.iter()
			.find(|(_, event)| event.event.id == event_id)
			.map(|(inode, _)| *inode)
		{
			self.creates_by_inode.remove(&position);
			return true;
		}

		// Check Windows ID storage
		if let Some(position) = self
			.creates_by_windows_id
			.iter()
			.find(|(_, event)| event.event.id == event_id)
			.map(|(id, _)| *id)
		{
			self.creates_by_windows_id.remove(&position);
			return true;
		}

		// Check size-based storage
		for events in self.creates_by_size.values_mut() {
			if let Some(pos) = events.iter().position(|e| e.event.id == event_id) {
				events.remove(pos);
				return true;
			}
		}

		// Check no-size storage
		if let Some(pos) = self
			.creates_no_size
			.iter()
			.position(|e| e.event.id == event_id)
		{
			self.creates_no_size.remove(pos);
			return true;
		}

		false
	}

	/// Clear all pending events
	pub fn clear(&mut self) {
		self.removes_by_size.clear();
		self.removes_no_size.clear();
		self.removes_by_inode.clear();
		self.removes_by_windows_id.clear();
		self.creates_by_size.clear();
		self.creates_no_size.clear();
		self.creates_by_inode.clear();
		self.creates_by_windows_id.clear();
		self.pending_rename_from = None;
	}
}
