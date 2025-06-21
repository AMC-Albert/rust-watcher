//! Types for multi-watch database management
//
// Contains overlap enums, transaction structs, and shared node types.

use chrono::{DateTime, Utc};
use std::path::PathBuf;
use uuid::Uuid;

/// Represents the overlap relationship between two watches
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchOverlap {
	/// No overlap between the two watches
	None,
	/// One watch is a strict ancestor of the other
	Ancestor { ancestor: Uuid, descendant: Uuid },
	/// The two watches have a common subtree (partial overlap)
	Partial {
		watch_a: Uuid,
		watch_b: Uuid,
		common_prefix: PathBuf,
	},
	/// The two watches are identical (same root)
	Identical(Uuid),
}

/// Transaction status for coordination
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TransactionStatus {
	InProgress,
	Committed,
	Aborted,
}

/// Metadata for a watch-scoped transaction
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WatchTransaction {
	pub transaction_id: Uuid,
	pub watch_id: Uuid,
	pub started_at: DateTime<Utc>,
	pub status: TransactionStatus,
}

// Re-export or alias types that are defined in crate::database::types for shared node info, etc.
pub use crate::database::types::SharedNodeInfo;
pub use crate::database::types::UnifiedNode;
pub use crate::database::types::WatchMetadata;
pub use crate::database::types::WatchPermissions;
