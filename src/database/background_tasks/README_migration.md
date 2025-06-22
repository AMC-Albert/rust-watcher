# Migration: background_tasks module

The following files are now obsolete and should be deleted after migration:
- src/database/background_tasks.rs
- src/database/background_tasks_impl.rs

All code has been moved to:
- src/database/background_tasks/mod.rs
- src/database/background_tasks/manager.rs
- src/database/background_tasks/impls.rs

Update all imports to use `crate::database::background_tasks::{BackgroundTaskManager, BackgroundTask, ...}` and `impls::*` as needed.
