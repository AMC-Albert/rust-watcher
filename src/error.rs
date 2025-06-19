use thiserror::Error;

#[derive(Error, Debug)]
pub enum WatcherError {
	#[error("IO error: {0}")]
	Io(#[from] std::io::Error),

	#[error("Notify error: {0}")]
	Notify(#[from] notify::Error),

	#[error("JSON serialization error: {0}")]
	Json(#[from] serde_json::Error),

	#[error("Channel send error")]
	ChannelSend,

	#[error("Invalid path: {path}")]
	InvalidPath { path: String },

	#[error("Watcher not initialized")]
	NotInitialized,
}

pub type Result<T> = std::result::Result<T, WatcherError>;
