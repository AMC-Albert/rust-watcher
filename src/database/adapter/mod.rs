//! Database adapter module root.
//!
//! Re-exports core adapter, background management, and related submodules.

mod core;
pub use core::*;
mod background;
pub use background::*;
mod maintenance;
pub use maintenance::*;
// TODO: Add event.rs, etc. as needed for further modularization.
