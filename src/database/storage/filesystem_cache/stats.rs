//! Stats and indexing logic for filesystem cache
//!
//! Contains per-watch and per-path stats update helpers for insert/remove operations.

use super::utils::{deserialize, serialize};
use crate::database::error::DatabaseResult;
use crate::database::storage::tables::{PATH_STATS, STATS_TABLE, WATCH_STATS};
use redb::{ReadableTable, WriteTransaction};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PathStats {
	pub event_count: u64,
	pub per_type_counts: HashMap<String, u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WatchStats {
	pub event_count: u64,
	pub per_type_counts: HashMap<String, u64>,
}

/// Increment event count for watch, path, and global stats (per-type)
pub fn increment_stats(
	write_txn: &mut WriteTransaction, watch_id: &Uuid, path_hash: u64, event_type: &str,
) -> DatabaseResult<()> {
	let mut watch_stats_table = write_txn.open_table(WATCH_STATS)?;
	let mut path_stats_table = write_txn.open_table(PATH_STATS)?;
	let mut stats_table = write_txn.open_table(STATS_TABLE)?;

	let watch_key = &watch_id.as_bytes()[..];
	let mut watch_stats = match watch_stats_table.get(watch_key)? {
		Some(bytes) => deserialize::<WatchStats>(bytes.value()).unwrap_or_default(),
		None => WatchStats::default(),
	};
	watch_stats.event_count += 1;
	*watch_stats.per_type_counts.entry(event_type.to_string()).or_insert(0) += 1;
	watch_stats_table.insert(watch_key, serialize(&watch_stats)?.as_slice())?;

	let path_key_arr = path_hash.to_le_bytes();
	let path_key = &path_key_arr[..];
	let mut path_stats = match path_stats_table.get(path_key)? {
		Some(bytes) => deserialize::<PathStats>(bytes.value()).unwrap_or_default(),
		None => PathStats::default(),
	};
	path_stats.event_count += 1;
	*path_stats.per_type_counts.entry(event_type.to_string()).or_insert(0) += 1;
	path_stats_table.insert(path_key, serialize(&path_stats)?.as_slice())?;

	// Update global stats table for this event type
	let stat_key = crate::database::types::event_type_stat_key(event_type);
	let mut count = match stats_table.get(stat_key.as_slice())? {
		Some(bytes) => u64::from_le_bytes(bytes.value().try_into().unwrap_or([0u8; 8])),
		None => 0,
	};
	count += 1;
	stats_table.insert(stat_key.as_slice(), count.to_le_bytes().as_slice())?;
	Ok(())
}

/// Decrement event count for watch, path, and global stats (per-type)
pub fn decrement_stats(
	write_txn: &mut WriteTransaction, watch_id: &Uuid, path_hash: u64, event_type: &str,
) -> DatabaseResult<()> {
	let mut watch_stats_table = write_txn.open_table(WATCH_STATS)?;
	let mut path_stats_table = write_txn.open_table(PATH_STATS)?;
	let mut stats_table = write_txn.open_table(STATS_TABLE)?;

	let watch_key = &watch_id.as_bytes()[..];
	let watch_stats = {
		if let Some(bytes) = watch_stats_table.get(watch_key)? {
			let mut stats = deserialize::<WatchStats>(bytes.value()).unwrap_or_default();
			if stats.event_count > 0 {
				stats.event_count -= 1;
			}
			if let Some(count) = stats.per_type_counts.get_mut(event_type) {
				if *count > 0 {
					*count -= 1;
				}
			}
			Some(stats)
		} else {
			None
		}
	};
	if let Some(stats) = watch_stats {
		watch_stats_table.insert(watch_key, serialize(&stats)?.as_slice())?;
	}

	let path_key_arr = path_hash.to_le_bytes();
	let path_key = &path_key_arr[..];
	let path_stats = {
		if let Some(bytes) = path_stats_table.get(path_key)? {
			let mut stats = deserialize::<PathStats>(bytes.value()).unwrap_or_default();
			if stats.event_count > 0 {
				stats.event_count -= 1;
			}
			if let Some(count) = stats.per_type_counts.get_mut(event_type) {
				if *count > 0 {
					*count -= 1;
				}
			}
			Some(stats)
		} else {
			None
		}
	};
	if let Some(stats) = path_stats {
		path_stats_table.insert(path_key, serialize(&stats)?.as_slice())?;
	}

	// Update global stats table for this event type
	let stat_key = crate::database::types::event_type_stat_key(event_type);
	let mut count = match stats_table.get(stat_key.as_slice())? {
		Some(bytes) => u64::from_le_bytes(bytes.value().try_into().unwrap_or([0u8; 8])),
		None => 0,
	};
	count = count.saturating_sub(1);
	stats_table.insert(stat_key.as_slice(), count.to_le_bytes().as_slice())?;
	Ok(())
}
