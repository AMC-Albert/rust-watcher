# Scalable Stats and Indexing Design

## Overview
This document tracks the design and implementation of scalable statistics and indexing in the filesystem cache, with explicit support for multi-watch correctness. The schema and stats struct work are now complete; the focus is on robust, incremental updates and correctness for all event types, including overlapping watches and shared nodes.

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

## Index Maintenance Logic (In Progress)

### On Event Insert (Create/Modify)
- [x] Increment global event and per-type counts in `STATS_TABLE`.
- [x] Increment event count in `WATCH_STATS` for the affected watch.
- [x] Increment event count in `PATH_STATS` for the affected path.
- [ ] If the event is associated with multiple watches, update all relevant `WATCH_STATS`.

### On Event Remove
- [x] Decrement global event and per-type counts.
- [x] Decrement event count in `WATCH_STATS` and `PATH_STATS`.
- [ ] For shared nodes, update all affected watches.

### On Event Move/Rename
- [ ] Decrement counters for the old path, increment for the new path in `PATH_STATS`.
- [ ] If the move crosses watches, update both source and destination `WATCH_STATS`.

### On Metadata Change
- [ ] Apply the same logic as above for metadata counters.

### Consistency and Repair
- [ ] On startup or if a counter is missing/corrupt, rescan only the relevant subset (per-watch or per-path) to repair, not the entire dataset.

## Multi-Watch Correctness (In Progress)
- [ ] All stats updates must be aware of shared nodes and overlapping watches.
- [ ] When a node is removed or moved, update every watch that references it.
- [ ] Tests must cover overlapping watches, shared node removal, and edge-case invalidation.

## Limitations and Risks
- Schema changes will require migration logic and careful versioning.
- Incremental update logic must be robust against partial failures and transaction rollbacks.
- Edge cases (e.g., watch overlap, node move between watches) are a source of subtle bugs and must be covered by integration tests.

## Next Steps
1. Wire updated stats helpers into all event mutation logic (insert, remove, move/rename), ensuring multi-watch and shared node correctness.
2. Expand integration tests for overlapping watches, shared node removal, move/rename, and edge cases.
3. Implement or stub repair tools for counter desynchronization.
4. Document known limitations and workarounds.
