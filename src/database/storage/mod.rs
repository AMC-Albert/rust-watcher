//! Storage module for database operations

pub mod core;
pub mod event_storage;
pub mod filesystem_cache;
pub mod indexing;
pub mod maintenance;
pub mod metadata_storage;
pub mod multi_watch;
pub mod tables;
pub mod transactions;

// Re-export the main traits and implementation
pub use core::{CoreTest, DatabaseStorage, RedbStorage};
pub use tables::*;

// Re-export specific trait capabilities for focused usage
pub use event_storage::EventStorage;
pub use filesystem_cache::FilesystemCacheStorage;
pub use indexing::IndexingStorage;
pub use maintenance::MaintenanceStorage;
pub use metadata_storage::MetadataStorage;
pub use multi_watch::MultiWatchStorage;
