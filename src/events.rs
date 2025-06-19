use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EventType {
	Create,
	Write,
	Remove,
	Rename,
	Move,
	Chmod,
	Other(String),
}

impl From<notify::EventKind> for EventType {
	fn from(kind: notify::EventKind) -> Self {
		match kind {
			notify::EventKind::Create(_) => EventType::Create,
			notify::EventKind::Modify(_) => EventType::Write,
			notify::EventKind::Remove(_) => EventType::Remove,
			notify::EventKind::Access(_) => EventType::Other("Access".to_string()),
			notify::EventKind::Other => EventType::Other("Unknown".to_string()),
			_ => EventType::Other(format!("{:?}", kind)),
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSystemEvent {
	pub id: Uuid,
	pub event_type: EventType,
	pub path: PathBuf,
	pub timestamp: DateTime<Utc>,
	pub is_directory: bool,
	pub size: Option<u64>,
	pub move_data: Option<MoveEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MoveEvent {
	pub source_path: PathBuf,
	pub destination_path: PathBuf,
	pub confidence: f32, // 0.0 to 1.0, how confident we are this is a move
	pub detection_method: MoveDetectionMethod,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MoveDetectionMethod {
	/// Detected by filesystem events (most reliable)
	FileSystemEvent,
	/// Detected by inode matching (Unix-like systems)
	InodeMatching,
	/// Detected by content hash comparison
	ContentHash,
	/// Detected by name pattern and timing
	NameAndTiming,
	/// Detected by metadata comparison
	MetadataMatching,
}

impl FileSystemEvent {
	pub fn new(
		event_type: EventType,
		path: PathBuf,
		is_directory: bool,
		size: Option<u64>,
	) -> Self {
		Self {
			id: Uuid::new_v4(),
			event_type,
			path,
			timestamp: Utc::now(),
			is_directory,
			size,
			move_data: None,
		}
	}

	pub fn with_move_data(mut self, move_data: MoveEvent) -> Self {
		self.move_data = Some(move_data);
		self.event_type = EventType::Move;
		self
	}

	pub fn is_move(&self) -> bool {
		self.move_data.is_some() || self.event_type == EventType::Move
	}

	pub fn to_json(&self) -> serde_json::Result<String> {
		serde_json::to_string_pretty(self)
	}
}
