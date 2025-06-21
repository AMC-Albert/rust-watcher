//! Stats and indexing logic for filesystem cache
//!
//! Contains per-watch and per-path stats update helpers for insert/remove operations.

use super::utils::{deserialize, serialize};
use crate::database::error::DatabaseResult;
use crate::database::storage::tables::{PATH_STATS, WATCH_STATS};
use crate::database::types::{PathStats, WatchStats};
use redb::{ReadableTable, WriteTransaction};
use uuid::Uuid;

/// Increment event count for watch and path
pub fn increment_stats(
	write_txn: &mut WriteTransaction, watch_id: &Uuid, path_hash: u64,
) -> DatabaseResult<()> {
	let mut watch_stats_table = write_txn.open_table(WATCH_STATS)?;
	let mut path_stats_table = write_txn.open_table(PATH_STATS)?;

	let watch_key = &watch_id.as_bytes()[..];
	let mut watch_stats = match watch_stats_table.get(watch_key)? {
		Some(bytes) => deserialize::<WatchStats>(bytes.value()).unwrap_or_default(),
		None => WatchStats::default(),
	};
	watch_stats.event_count += 1;
	watch_stats_table.insert(watch_key, serialize(&watch_stats)?.as_slice())?;

	let path_key_arr = path_hash.to_le_bytes();
	let path_key = &path_key_arr[..];
	let mut path_stats = match path_stats_table.get(path_key)? {
		Some(bytes) => deserialize::<PathStats>(bytes.value()).unwrap_or_default(),
		None => PathStats::default(),
	};
	path_stats.event_count += 1;
	path_stats_table.insert(path_key, serialize(&path_stats)?.as_slice())?;
	Ok(())
}

/// Decrement event count for watch and path
pub fn decrement_stats(
	write_txn: &mut WriteTransaction, watch_id: &Uuid, path_hash: u64,
) -> DatabaseResult<()> {
	let mut watch_stats_table = write_txn.open_table(WATCH_STATS)?;
	let mut path_stats_table = write_txn.open_table(PATH_STATS)?;

	let watch_key = &watch_id.as_bytes()[..];
	let watch_stats = {
		if let Some(bytes) = watch_stats_table.get(watch_key)? {
			let mut stats = deserialize::<WatchStats>(bytes.value()).unwrap_or_default();
			if stats.event_count > 0 {
				stats.event_count -= 1;
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
			Some(stats)
		} else {
			None
		}
	};
	if let Some(stats) = path_stats {
		path_stats_table.insert(path_key, serialize(&stats)?.as_slice())?;
	}
	Ok(())
}
