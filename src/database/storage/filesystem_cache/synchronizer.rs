//! Synchronizes the filesystem cache with watcher events.
//!
//! This module provides a synchronizer that listens to filesystem events and updates the cache incrementally.
//!
//! Limitations:
//! - No transactional rollback; failures may leave cache in an inconsistent state.
//! - No batching; each event is processed individually.
//! - No cross-process coordination.

use crate::database::storage::FilesystemCacheStorage;
use crate::events::{EventType, FileSystemEvent};
use std::sync::Arc;
use uuid::Uuid;

/// Trait for synchronizing the filesystem cache with events.
#[async_trait::async_trait]
pub trait FilesystemCacheSynchronizer: Send + Sync {
	async fn handle_event(&mut self, watch_id: &Uuid, event: &FileSystemEvent);
}

/// Default implementation for a synchronizer that updates the cache incrementally.
pub struct DefaultFilesystemCacheSynchronizer<T: FilesystemCacheStorage> {
	pub cache: Arc<tokio::sync::Mutex<T>>,
}

#[async_trait::async_trait]
impl<T: FilesystemCacheStorage> FilesystemCacheSynchronizer
	for DefaultFilesystemCacheSynchronizer<T>
{
	async fn handle_event(&mut self, watch_id: &Uuid, event: &FileSystemEvent) {
		// This is a pragmatic, non-transactional implementation.
		// TODO: Add error handling/reporting, and consider transactional semantics.
		let mut cache = self.cache.lock().await;
		let event_type_str = format!("{:?}", event.event_type);
		match event.event_type {
			EventType::Create | EventType::Write | EventType::Chmod | EventType::Other(_) => {
				// Attempt to store or update the node in the cache.
				if let Some(ref node) = event_to_node(event) {
					if let Err(e) =
						cache.store_filesystem_node(watch_id, node, &event_type_str).await
					{
						// Log error, but do not panic. In production, consider error metrics.
						tracing::warn!("Cache update failed: {}", e);
					}
				} else {
					// No node info available; cannot update cache.
					tracing::debug!("No node info for event, skipping cache update: {:?}", event);
				}
			}
			EventType::Remove => {
				// Remove the node from the cache if possible.
				let path = &event.path;
				if let Err(e) = cache.remove_filesystem_node(watch_id, path, &event_type_str).await
				{
					tracing::warn!("Cache node removal failed: {}", e);
				}
			}
			EventType::Rename | EventType::Move | EventType::RenameFrom | EventType::RenameTo => {
				// Rename (move) the node in the cache if possible.
				if let Some(ref move_data) = event.move_data {
					let old_path = &move_data.source_path;
					let new_path = &move_data.destination_path;
					if let Err(e) = cache
						.rename_filesystem_node(watch_id, old_path, new_path, &event_type_str)
						.await
					{
						tracing::warn!("Cache node rename failed: {}", e);
					}
				} else {
					tracing::debug!(
						"Missing move_data for rename event, skipping cache rename: {:?}",
						event
					);
				}
			}
		}
	}
}

/// Converts a FileSystemEvent to a FilesystemNode if possible.
fn event_to_node(event: &FileSystemEvent) -> Option<crate::database::types::FilesystemNode> {
	use crate::database::types::{FilesystemNode, NodeType};
	use std::fs;
	let path = &event.path;
	let metadata = fs::metadata(path).ok();
	let event_type_str = format!("{:?}", event.event_type);
	if let Some(ref meta) = metadata {
		// Use FilesystemNode::new_with_event_type for best-effort construction
		Some(FilesystemNode::new_with_event_type(
			path.clone(),
			meta,
			Some(event_type_str),
		))
	} else {
		// Fallback: synthesize minimal node from event fields
		let node_type = if event.is_directory {
			NodeType::Directory { child_count: 0, total_size: 0, max_depth: 0 }
		} else {
			NodeType::File { size: event.size.unwrap_or(0), content_hash: None, mime_type: None }
		};
		Some(FilesystemNode {
			path: path.clone(),
			node_type,
			metadata: Default::default(),
			cache_info: Default::default(),
			computed: Default::default(),
			last_event_type: Some(event_type_str),
		})
	}
}
