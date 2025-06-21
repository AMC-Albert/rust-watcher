//! Filesystem cache module public API and re-exports
//!
//! This module provides the trait and implementation for the filesystem cache storage.
//!
//! Limitations:
//! - Only ReDB backend is currently implemented.
//! - Naive search and traversal; not suitable for very large datasets.
//!
//! TODO: Add alternative backends and improve search performance.

mod implementation;
pub use implementation::RedbFilesystemCache;

pub mod stats;
pub mod synchronizer;
pub mod trait_def;
mod utils;

// Tests are in tests.rs
