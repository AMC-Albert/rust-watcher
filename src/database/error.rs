//! Error types for database operations

use thiserror::Error;

#[derive(Error, Debug)]
pub enum DatabaseError {
	#[error("Database initialization failed: {0}")]
	InitializationFailed(String),

	#[error("Database connection failed: {0}")]
	ConnectionFailed(String),

	#[error("Serialization error: {0}")]
	Serialization(String),

	#[error("Deserialization error: {0}")]
	Deserialization(String),

	#[error("IO error: {0}")]
	IoError(#[from] std::io::Error),

	#[error("Database corruption detected: {0}")]
	CorruptionError(String),

	#[error("Transaction failed: {0}")]
	TransactionError(String),

	#[error("Key not found: {0}")]
	KeyNotFound(String),

	#[error("Database is read-only")]
	ReadOnlyError,

	#[error("Database size limit exceeded")]
	SizeLimitExceeded,

	#[error("Database operation timeout")]
	Timeout,

	#[error("Storage operation failed: {0}")]
	StorageError(String),

	#[error("redb database error: {0}")]
	RedbError(#[from] redb::Error),

	#[error("redb transaction error: {0}")]
	RedbTransactionError(#[from] redb::TransactionError),

	#[error("redb commit error: {0}")]
	RedbCommitError(#[from] redb::CommitError),

	#[error("redb table error: {0}")]
	RedbTableError(#[from] redb::TableError),

	#[error("redb storage error: {0}")]
	RedbStorageError(#[from] redb::StorageError),

	#[error("Invalid configuration: {0}")]
	InvalidConfiguration(String),
}

impl DatabaseError {
	/// Check if this error indicates that the operation should be retried
	pub fn is_retryable(&self) -> bool {
		matches!(
			self,
			DatabaseError::Timeout
				| DatabaseError::TransactionError(_)
				| DatabaseError::ConnectionFailed(_)
		)
	}

	/// Check if this error indicates data corruption
	pub fn is_corruption(&self) -> bool {
		matches!(self, DatabaseError::CorruptionError(_))
	}

	/// Check if this error is due to resource limitations
	pub fn is_resource_limit(&self) -> bool {
		matches!(
			self,
			DatabaseError::SizeLimitExceeded | DatabaseError::ReadOnlyError
		)
	}
}

impl From<redb::DatabaseError> for DatabaseError {
	fn from(e: redb::DatabaseError) -> Self {
		DatabaseError::RedbError(redb::Error::from(e))
	}
}

pub type DatabaseResult<T> = Result<T, DatabaseError>;

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_error_categorization() {
		let timeout_error = DatabaseError::Timeout;
		assert!(timeout_error.is_retryable());
		assert!(!timeout_error.is_corruption());
		assert!(!timeout_error.is_resource_limit());

		let corruption_error = DatabaseError::CorruptionError("test".to_string());
		assert!(!corruption_error.is_retryable());
		assert!(corruption_error.is_corruption());
		assert!(!corruption_error.is_resource_limit());

		let size_error = DatabaseError::SizeLimitExceeded;
		assert!(!size_error.is_retryable());
		assert!(!size_error.is_corruption());
		assert!(size_error.is_resource_limit());
	}

	#[test]
	fn test_error_display() {
		let error = DatabaseError::InitializationFailed("test failure".to_string());
		let display = format!("{}", error);
		assert!(display.contains("Database initialization failed"));
		assert!(display.contains("test failure"));
	}
}
