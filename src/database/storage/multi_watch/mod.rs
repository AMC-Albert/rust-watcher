//! MultiWatch module: public API and re-exports
//
// This module coordinates multi-watch database management, overlap detection, and shared cache optimization.

pub mod implementation;
pub mod optimization;
pub mod types;

// Re-export main API for external use
pub use implementation::MultiWatchDatabase;
pub use optimization::*;
pub use types::*;
