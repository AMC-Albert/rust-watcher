//! Path-to-watches mapping logic for filesystem cache
//
// This module contains helpers for maintaining the mapping from paths to watches.
//
// Limitations:
// - O(N) scan for some operations; not suitable for very large datasets.
// - TODO: Replace with indexed or batched queries for production use.

use crate::database::error::DatabaseResult;
use redb::Database;
use redb::WriteTransaction;
use uuid::Uuid;

pub struct WatchMappingHelpers;

impl WatchMappingHelpers {
	/// Insert a watch mapping for a given path hash.
	pub fn insert_watch_mapping(
		write_txn: &WriteTransaction, path_hash: u64, watch_id: &Uuid,
	) -> DatabaseResult<()> {
		let path_key = path_hash.to_le_bytes();
		let watch_bytes = &watch_id.as_bytes()[..];
		let mut path_watches_table =
			write_txn.open_multimap_table(crate::database::storage::tables::PATH_TO_WATCHES)?;
		path_watches_table.insert(path_key.as_slice(), watch_bytes)?;
		Ok(())
	}

	/// Enumerate all watches for a given path hash (read-only)
	pub fn get_watches_for_path(db: &Database, path_hash: u64) -> DatabaseResult<Vec<Uuid>> {
		let read_txn = db.begin_read()?;
		let path_watches_table =
			read_txn.open_multimap_table(crate::database::storage::tables::PATH_TO_WATCHES)?;
		let path_key = path_hash.to_le_bytes();
		let mut watches = Vec::new();
		if let Ok(iter) = path_watches_table.get(path_key.as_slice()) {
			for entry in iter {
				let entry = entry?;
				if entry.value().len() == 16 {
					if let Ok(uuid) = Uuid::from_slice(entry.value()) {
						watches.push(uuid);
					}
				}
			}
		}
		Ok(watches)
	}
	// TODO: Add more helpers as needed (removal, lookup, etc.)
}
