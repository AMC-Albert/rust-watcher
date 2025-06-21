use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EventType {
	Create,
	Write,
	Remove,
	RenameFrom, // Old name in rename operation
	RenameTo,   // New name in rename operation
	Rename,     // Generic rename (when direction unclear)
	Move,
	Chmod,
	Other(String),
}

impl From<notify::EventKind> for EventType {
	fn from(kind: notify::EventKind) -> Self {
		match kind {
			notify::EventKind::Create(_) => EventType::Create,
			notify::EventKind::Modify(modify_kind) => match modify_kind {
				notify::event::ModifyKind::Name(name_kind) => match name_kind {
					notify::event::RenameMode::From => EventType::RenameFrom,
					notify::event::RenameMode::To => EventType::RenameTo,
					_ => EventType::Rename,
				},
				_ => EventType::Write,
			},
			notify::EventKind::Remove(_) => EventType::Remove,
			notify::EventKind::Access(_) => EventType::Other("Access".to_string()),
			notify::EventKind::Other => EventType::Other("Unknown".to_string()),
			_ => EventType::Other(format!("{kind:?}")),
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
	Inode,
	/// Detected by inode matching (Unix-like systems) - backwards compatibility alias
	InodeMatching,
	/// Detected by Windows-specific file identifier
	WindowsId,
	/// Detected by content hash comparison
	ContentHash,
	/// Detected by name pattern and timing
	NameAndTiming,
	/// Detected by size and timing
	SizeAndTime,
	/// Detected by metadata comparison
	MetadataMatching,
	/// Detected by rename events
	Rename,
	/// Detected by heuristics when other methods uncertain
	Heuristics,
}

impl FileSystemEvent {
	pub fn new(
		event_type: EventType, path: PathBuf, is_directory: bool, size: Option<u64>,
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

#[cfg(test)]
mod tests {
	use super::*;
	use std::path::PathBuf;

	#[test]
	fn test_event_type_from_notify() {
		// Test conversion from notify events
		let create_kind = notify::EventKind::Create(notify::event::CreateKind::File);
		let event_type = EventType::from(create_kind);
		assert_eq!(event_type, EventType::Create);
	}

	#[test]
	fn test_move_detection_method_variants() {
		// Test that all MoveDetectionMethod variants can be created
		let methods = vec![
			MoveDetectionMethod::FileSystemEvent,
			MoveDetectionMethod::Inode,
			MoveDetectionMethod::InodeMatching,
			MoveDetectionMethod::WindowsId,
			MoveDetectionMethod::ContentHash,
			MoveDetectionMethod::NameAndTiming,
		];

		for method in methods {
			// Just test that they can be created and serialized
			let json = serde_json::to_string(&method).unwrap();
			assert!(!json.is_empty());
		}
	}

	#[test]
	fn test_filesystem_event_creation() {
		let event = FileSystemEvent {
			id: uuid::Uuid::new_v4(),
			event_type: EventType::Create,
			path: PathBuf::from("/test/file.txt"),
			timestamp: chrono::Utc::now(),
			is_directory: false,
			size: Some(100),
			move_data: None,
		};

		assert_eq!(event.event_type, EventType::Create);
		assert_eq!(event.path, PathBuf::from("/test/file.txt"));
		assert!(!event.is_directory);
		assert_eq!(event.size, Some(100));
		assert!(!event.is_move());
	}

	#[test]
	fn test_filesystem_event_with_move_data() {
		let move_event = MoveEvent {
			source_path: PathBuf::from("/source.txt"),
			destination_path: PathBuf::from("/dest.txt"),
			confidence: 0.95,
			detection_method: MoveDetectionMethod::FileSystemEvent,
		};

		let mut event = FileSystemEvent {
			id: uuid::Uuid::new_v4(),
			event_type: EventType::Create,
			path: PathBuf::from("/dest.txt"),
			timestamp: chrono::Utc::now(),
			is_directory: false,
			size: Some(100),
			move_data: None,
		};

		event = event.with_move_data(move_event);

		assert!(event.is_move());
		assert_eq!(event.event_type, EventType::Move);
		assert!(event.move_data.is_some());

		let move_data = event.move_data.unwrap();
		assert_eq!(move_data.source_path, PathBuf::from("/source.txt"));
		assert_eq!(move_data.destination_path, PathBuf::from("/dest.txt"));
		assert_eq!(move_data.confidence, 0.95);
	}

	#[test]
	fn test_event_serialization() {
		let event = FileSystemEvent {
			id: uuid::Uuid::new_v4(),
			event_type: EventType::Write,
			path: PathBuf::from("/test.txt"),
			timestamp: chrono::Utc::now(),
			is_directory: false,
			size: Some(50),
			move_data: None,
		};

		let json = event.to_json().unwrap();
		assert!(json.contains("Write"));
		assert!(json.contains("test.txt"));
		assert!(json.contains("50"));
	}
}
