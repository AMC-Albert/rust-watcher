mod error;
mod events;
mod move_detector;
mod watcher;

pub use error::{Result, WatcherError};
pub use events::{EventType, FileSystemEvent, MoveDetectionMethod, MoveEvent};
pub use move_detector::{MoveDetector, MoveDetectorConfig};
pub use watcher::{start, WatcherConfig, WatcherHandle};
