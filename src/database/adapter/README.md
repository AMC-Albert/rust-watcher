# Database Adapter Module

This module provides a clean, decoupled interface for database operations, background task management, and related coordination logic.

- `core.rs`: Main adapter implementation (migrated from adapter.rs)
- `mod.rs`: Module root and re-exports

Further modularization (background.rs, event.rs, etc.) is recommended as the codebase grows.
