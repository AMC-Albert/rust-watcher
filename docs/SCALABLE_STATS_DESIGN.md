# Scalable Stats and Indexing Design

## Overview
This document tracks the design and implementation of scalable statistics and indexing in the filesystem cache, with explicit support for multi-watch correctness. The schema and stats struct work are complete; the codebase is now modular, maintainable, and implements robust, incremental updates and correctness for all event types, including overlapping watches and shared nodes. All code and tests are clippy-clean and up to date with the new API. Recent work includes a functional glob-based search_nodes implementation, a working repair_stats_counters tool, and expanded integration test coverage.

## Schema (Complete)

### 1. Per-Watch Event/Metadata Counters
- **Table:** `WATCH_STATS` (implemented)
  - **Key:** `watch_id` (or composite key if needed)
  - **Value:** Struct with:
    - `event_count: u64`
    - `metadata_count: u64`
    - `per_type_counts: HashMap<String, u64>`

### 2. Per-Path Event/Metadata Counters
- **Table:** `PATH_STATS` (implemented)
  - **Key:** Canonicalized path (or path hash)
  - **Value:** Struct with:
    - `event_count: u64`
    - `metadata_count: u64`
    - `per_type_counts: HashMap<String, u64>`

### 3. Per-Type Event Counters (Global)
- **Table:** `STATS_TABLE` (implemented)
  - **Key:** `event_type:<type>`
  - **Value:** `u64`

## Index Maintenance Logic (Mostly Complete)

### On Event Insert (Create/Modify)
- [x] Increment global event and per-type counts in `STATS_TABLE`.
- [x] Increment event count in `WATCH_STATS` for the affected watch.
- [x] Increment event count in `PATH_STATS` for the affected path.
- [x] If the event is associated with multiple watches, update all relevant `WATCH_STATS`.

### On Event Remove
- [x] Decrement global event and per-type counts.
- [x] Decrement event count in `WATCH_STATS` and `PATH_STATS`.
- [x] For shared nodes, update all affected watches.

### On Event Move/Rename
- [x] Decrement counters for the old path, increment for the new path in `PATH_STATS`.
- [x] If the move crosses watches, update both source and destination `WATCH_STATS`.

### On Metadata Change
- [x] Apply the same logic as above for metadata counters.

### Consistency and Repair
- [x] Repair tool implemented: scans all nodes, recomputes stats, and updates tables. Limitation: event type is not stored on nodes, so all are counted as 'create'.
- [ ] For full event-type accuracy, event type must be stored or indexed with each node.

## Multi-Watch Correctness (Mostly Complete)
- [x] All stats updates must be aware of shared nodes and overlapping watches.
- [x] When a node is removed or moved, update every watch that references it.
- [x] Tests cover overlapping watches, shared node removal, move/rename, and edge-case invalidation.

## Search/Indexing
- [x] `search_nodes` implemented using globset for glob pattern matching. This is a naive O(N) scan, suitable for small/medium datasets and test coverage. Not optimized for large datasets.

## Limitations and Risks
- Schema changes will require migration logic and careful versioning.
- Incremental update logic must be robust against partial failures and transaction rollbacks.
- Edge cases (e.g., watch overlap, node move between watches) are a source of subtle bugs and must be covered by integration tests.
- `search_nodes` is not optimized for large datasets; performance will degrade with scale.
- Repair tooling assumes all nodes are 'create' events; event-type-accurate repair requires schema changes.

## Next Steps
1. Store or index event type with each node to enable fully accurate repair.
2. Optimize search/indexing for large datasets if needed.
3. Continue expanding integration tests for new edge cases as they are discovered.
4. Document known limitations and workarounds as new features are added.
