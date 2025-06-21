//! Filesystem cache module: public API and re-exports
//!
//! This module coordinates filesystem cache storage, queries, and synchronization.

pub mod implementation;
pub mod trait_def;
// pub mod query; // For Phase 3+
// pub mod sync;  // For Phase 3+

pub use implementation::*;
pub use trait_def::*;
// pub use query::*;
// pub use sync::*;
