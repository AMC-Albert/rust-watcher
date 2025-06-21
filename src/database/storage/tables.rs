//! Table definitions for ReDB storage
//!
//! This module contains all table definitions used across the storage implementation.
//! Centralizing table definitions here ensures consistency and makes schema evolution easier.

use crate::database::error::DatabaseResult;
use redb::{Database, MultimapTableDefinition, TableDefinition};
use std::sync::Arc;

// ===== Basic Event and Metadata Tables =====

/// Events table for storing filesystem events
pub const EVENTS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("events");

/// Metadata table for storing file metadata
pub const METADATA_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("metadata");

/// General-purpose indexes for events (size, time buckets, etc.)
pub const INDEXES_TABLE: MultimapTableDefinition<&[u8], &[u8]> =
	MultimapTableDefinition::new("indexes");

/// Multimap events table for append-only event log (key: path hash, value: serialized EventRecord)
pub const EVENTS_LOG_TABLE: MultimapTableDefinition<&[u8], &[u8]> =
	MultimapTableDefinition::new("events_log");

// ===== Filesystem Cache Tables =====

/// Primary filesystem cache table (path_hash -> FilesystemNode)
pub const FS_CACHE_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("fs_cache");

/// Directory hierarchy multimap (parent_path_hash -> [child_path_hashes])
pub const HIERARCHY_TABLE: MultimapTableDefinition<&[u8], &[u8]> =
	MultimapTableDefinition::new("hierarchy");

/// Path prefix index for efficient subtree operations (prefix -> [path_hashes])
pub const PATH_PREFIX_TABLE: MultimapTableDefinition<&[u8], &[u8]> =
	MultimapTableDefinition::new("path_prefix");

/// Depth-based index for level-order traversal (depth -> [path_hashes])
pub const DEPTH_INDEX_TABLE: MultimapTableDefinition<&[u8], &[u8]> =
	MultimapTableDefinition::new("depth_index");

// ===== Multi-Watch Tables =====

/// Multi-watch filesystem cache with watch scoping (watch_scoped_key -> FilesystemNode)
pub const MULTI_WATCH_FS_CACHE: TableDefinition<&[u8], &[u8]> =
	TableDefinition::new("multi_fs_cache");

/// Multi-watch hierarchy with watch scoping (watch_scoped_parent -> [watch_scoped_children])
pub const MULTI_WATCH_HIERARCHY: MultimapTableDefinition<&[u8], &[u8]> =
	MultimapTableDefinition::new("multi_hierarchy");

/// Shared nodes table (path_hash -> SharedNodeInfo)
pub const SHARED_NODES: TableDefinition<&[u8], &[u8]> = TableDefinition::new("shared_nodes");

/// Watch registry (watch_id -> WatchMetadata)
pub const WATCH_REGISTRY: TableDefinition<&[u8], &[u8]> = TableDefinition::new("watch_registry");

/// Path to watches mapping (path_hash -> [watch_ids])
pub const PATH_TO_WATCHES: MultimapTableDefinition<&[u8], &[u8]> =
	MultimapTableDefinition::new("path_to_watches");

// ===== Performance and Maintenance Tables =====

/// Database statistics and health metrics
///
/// Key format (for extensibility):
///   - b"event_count"                  => total event count (u64)
///   - b"metadata_count"               => total metadata count (u64)
///   - b"event_type:<type>"            => per-event-type count (u64)
///   - b"watch:<uuid>:event_count"     => per-watch event count (u64)
///   - b"path:<hash>:event_count"      => per-path event count (u64, future)
///   - ... (future stat types)
///
/// All stat keys must be updated transactionally with the relevant mutation.
pub const STATS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("stats");

/// Maintenance operations log and scheduling
pub const MAINTENANCE_LOG: TableDefinition<&[u8], &[u8]> = TableDefinition::new("maintenance_log");

/// Table groups for easier management
pub const BASIC_TABLES: &[&str] = &["events", "metadata", "indexes"];
pub const FILESYSTEM_CACHE_TABLES: &[&str] =
	&["fs_cache", "hierarchy", "path_prefix", "depth_index"];
pub const MULTI_WATCH_TABLES: &[&str] = &[
	"multi_fs_cache",
	"multi_hierarchy",
	"shared_nodes",
	"watch_registry",
	"path_to_watches",
];
pub const MAINTENANCE_TABLES: &[&str] = &["stats", "maintenance_log"];

/// All tables for initialization
pub const ALL_TABLES: &[&str] = &[
	"events",
	"metadata",
	"indexes",
	"fs_cache",
	"hierarchy",
	"path_prefix",
	"depth_index",
	"multi_fs_cache",
	"multi_hierarchy",
	"shared_nodes",
	"watch_registry",
	"path_to_watches",
	"stats",
	"maintenance_log",
];

/// Schema version for migration tracking
pub const SCHEMA_VERSION: u32 = 1;

/// Key for event count in STATS_TABLE (u64, little-endian bytes)
pub const EVENT_COUNT_KEY: &[u8] = b"event_count";
// This key is used to store the persistent event count for O(1) stats queries.
// It must be updated transactionally on every event insert/delete.

/// Key for metadata count in STATS_TABLE (u64, little-endian bytes)
pub const METADATA_COUNT_KEY: &[u8] = b"metadata_count";
// This key is used to store the persistent metadata count for O(1) stats queries.
// It must be updated transactionally on every metadata insert/delete.

/// Key for event sequence number in STATS_TABLE (u64, little-endian bytes)
pub const EVENT_SEQUENCE_KEY: &[u8] = b"event_sequence";
// This key is used to store the next event sequence number for strict append order.
// It must be incremented transactionally on every event insert.

/// Initialize all database tables
pub async fn initialize_tables(database: &Arc<Database>) -> DatabaseResult<()> {
	let write_txn = database.begin_write()?;
	{
		// Initialize basic tables
		let _events_table = write_txn.open_table(EVENTS_TABLE)?;
		let _metadata_table = write_txn.open_table(METADATA_TABLE)?;
		let _indexes_table = write_txn.open_multimap_table(INDEXES_TABLE)?;
		// Initialize append-only event log table (multimap)
		let _events_log_table = write_txn.open_multimap_table(EVENTS_LOG_TABLE)?;

		// Initialize filesystem cache tables
		let _fs_cache_table = write_txn.open_table(FS_CACHE_TABLE)?;
		let _hierarchy_table = write_txn.open_multimap_table(HIERARCHY_TABLE)?;
		let _path_prefix_table = write_txn.open_multimap_table(PATH_PREFIX_TABLE)?;
		let _depth_index_table = write_txn.open_multimap_table(DEPTH_INDEX_TABLE)?;

		// Initialize multi-watch tables
		let _multi_fs_cache_table = write_txn.open_table(MULTI_WATCH_FS_CACHE)?;
		let _multi_hierarchy_table = write_txn.open_multimap_table(MULTI_WATCH_HIERARCHY)?;
		let _shared_nodes_table = write_txn.open_table(SHARED_NODES)?;
		let _watch_registry_table = write_txn.open_table(WATCH_REGISTRY)?;
		let _path_to_watches_table = write_txn.open_multimap_table(PATH_TO_WATCHES)?;

		// Initialize maintenance tables
		let _stats_table = write_txn.open_table(STATS_TABLE)?;
		let _maintenance_log_table = write_txn.open_table(MAINTENANCE_LOG)?;
	}
	write_txn.commit()?;
	Ok(())
}
