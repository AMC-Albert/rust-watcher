//! Database module for handling massive directory watching scenarios
//!
//! This module provides persistent storage for filesystem events and metadata
//! using the `redb` embedded database. It's designed to handle directories
//! with hundreds of thousands or millions of files without memory exhaustion.

pub mod config;
pub mod error;
pub mod storage;
pub mod types;

pub use config::DatabaseConfig;
pub use error::{DatabaseError, DatabaseResult};
pub use storage::{DatabaseStorage, RedbStorage};
pub use types::{EventRecord, MetadataRecord, StorageKey};
