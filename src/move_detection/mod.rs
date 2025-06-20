//! Move detection module for the rust-watcher library
//!
//! This module provides comprehensive move detection capabilities by analyzing
//! filesystem events and inferring when files or directories have been moved
//! rather than removed and created separately.
//!
//! # Module Organization
//!
//! - [`config`] - Configuration structures and validation
//! - [`events`] - Event storage and management
//! - [`metadata`] - File metadata caching
//! - [`heuristics`] - Path type inference and similarity algorithms
//! - [`matching`] - Move detection algorithms and confidence calculations
//! - [`monitoring`] - Resource monitoring and statistics
//! - [`detector`] - Main MoveDetector implementation
//! - [`error`] - Move detection specific error types

pub mod config;
pub mod detector;
pub mod error;
pub mod events;
pub mod heuristics;
pub mod matching;
pub mod metadata;
pub mod monitoring;

// Re-export main types for convenience
pub use config::MoveDetectorConfig;
pub use detector::MoveDetector;
pub use error::MoveDetectionError;
