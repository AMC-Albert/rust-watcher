# TODO: Filesystem Cache Implementation

## Status Update (June 2025)
- Filesystem cache refactored to modular structure: trait definitions, implementation, and module root separated for maintainability.
- All legacy, monolithic, and backup files removed. `.bak` file deleted after migration.
- Trait and implementation mismatches resolved; all APIs now match current project and redb versions.
- Directory hierarchy and child listing logic restored to match original, passing all integration tests.
- All clippy warnings and build errors fixed; codebase is warning-free and testable.
- All tests pass except for explicitly unimplemented multi-watch integration tests (not a regression).
- Documentation and comments updated for current code; pending features are still only in design/TODO docs.
- Phase 3 groundwork: trait stubs and documentation for unified/cross-watch queries, hierarchical operations, and pattern search added. Unused parameter warnings resolved (June 2025).
- `list_directory_unified` now performs a true cross-watch aggregation and deduplication.
- `get_unified_node` implemented: prefers shared node, falls back to per-watch.
- Hierarchical operations (`list_ancestors`, `list_descendants`) implemented and tested; defensive against cycles and missing parents.
- All build, test, and clippy checks are clean as of latest commit.
- See commit history for details of the modularization, bugfixes, and groundwork for Phase 3 (June 2025).

---

## Phase 1: Core Storage and Event Log (Highest Priority)

### 1.1 Event Log Storage Redesign
- [x] Redesign event storage to use a multimap table for append-only event log semantics (critical for move/rename tracking and history).
    - [x] Update schema/table definitions for multimap event log.
    - [x] Implement append-only store logic (multiple events per key/path).
    - [x] Implement retrieval logic to return all events for a key/path, ordered by time.  # (ordering is now handled in tests)
    - [x] Add retention/cleanup logic for old events.  # Implemented and tested June 2025
    - [x] Document edge cases: duplicate events, ordering, retention.  # Complete (see event_retention.rs)
- [x] Update all event storage access patterns to match new schema.
- [x] Only after the above is stable, revisit and update tests to match new semantics.

### 1.2 Filesystem Cache Table and Hierarchy
- [x] Add/validate filesystem cache table definitions in `database/storage.rs`.
- [x] Implement hierarchy and prefix index tables for fast subtree and move/rename queries.
- [x] Ensure cache supports rapid prefix and subtree queries.

### 1.3 Core Node and Relationship Storage
- [x] Implement `FilesystemNode` serialization/deserialization.
- [x] Add filesystem cache storage methods to `DatabaseStorage` trait.
- [x] Implement watch-scoped key generation and path hashing.
- [x] Add batch insert operations for initial tree caching.

## Immediate Next Steps (all complete)

- [x] Document event log edge cases, especially around duplicate events, ordering, and retention policy behavior.  # Complete (see event_retention.rs)
- [x] Implement brute-force event stats (O(N), not scalable, see maintenance.rs).  # Complete, but not suitable for production
- [x] Design and implement a scalable, indexed, and robust stats subsystem.  # Persistent O(1) event and per-type stats implemented, tested, and documented (June 2025). Further extensibility (per-watch, per-path) and advanced indexing still TODO.
- [x] Validate cross-platform path handling (Windows vs Unix).  # Complete (see path_utils.rs, path_normalization.rs)
- [x] Create integration test framework for multi-watch scenarios.  # Scaffolded and passing basic checks (integration_multi_watch.rs)
- [x] Add stress tests for concurrent cache access patterns.  # Scaffolded and passing basic checks (stress_cache_concurrency.rs)
- [x] Design and stub out the `MultiWatchDatabase` and related APIs for Phase 2.  # Initial stubs and partial implementation exist (multi_watch.rs, storage/multi_watch.rs)

### Priority Order (as of June 2025)

- [x] Expand and harden MultiWatchDatabase and related APIs
    - [x] Implement watch registration, removal, and shared node cache management.
    - [x] Add watch-scoped transaction coordination and metadata management.
- [x] Implement real multi-watch integration tests
    - [x] Use the scaffolded framework to drive development and catch regressions early.
- [x] Implement real stress/concurrency tests
    - [x] Use the scaffolded stress tests to validate cache and database concurrency under load.

---

# Summary Table

| Task                            | Status   | Notes                                                                                                                                         |
| ------------------------------- | -------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| Event log retention/cleanup     | Complete | Logic, tests, and integration done                                                                                                            |
| Event log edge case docs        | Complete | Documented in code and event_retention.rs                                                                                                     |
| Brute-force stats impl          | Complete | O(N), not scalable, see maintenance.rs                                                                                                        |
| Scalable stats/indexing         | Partial  | O(1) event and per-type stats done; per-watch, per-path, and advanced indexing still TODO                                                     |
| Cross-platform path handling    | Complete | Windows/Unix normalization, edge cases                                                                                                        |
| Multi-watch test infra          | Complete | Scaffolded and passing basic checks                                                                                                           |
| Stress tests                    | Complete | Scaffolded and passing basic checks                                                                                                           |
| Multi-watch core                | Complete | Persistent transaction coordination, watch creation, permissions, and tree scan implemented                                                   |
| Multi-watch concurrency tests   | Complete | Integration test for concurrent registration/removal passes (June 2025)                                                                       |
| Shared cache optimization       | Complete | Overlap detection, shared node merge, cleanup, robust error handling, and background scheduler implemented. Remaining limitations documented. |
| Redundant/orphaned node cleanup | Complete | Robust removal of redundant watch-specific and orphaned shared nodes; integration test passes (June 2025)                                     |
| Unified/cross-watch queries     | Complete | `list_directory_unified` and `get_unified_node` implemented and tested                                                                        |
| Hierarchical operations         | Complete | `list_ancestors` and `list_descendants` implemented and tested                                                                                |

---

## Phase 2: Multi-Watch and Relationship Tracking

### 2.1 Multi-Watch Database Core
- [x] Create `MultiWatchDatabase` struct in new `database/multi_watch.rs`  # Partial implementation exists
- [x] Implement watch registration and metadata management
- [x] Add watch-scoped transaction coordination and persistent transaction metadata
- [x] Implement shared node cache management

### 2.2 Watch Operations
- [x] Add watch creation with filesystem tree scanning
- [x] Implement watch removal with cleanup
- [x] Add watch metadata queries (list, stats, health checks)
- [x] Implement watch isolation and permission management

### 2.3 Overlap Detection and Optimization
- [x] Create `WatchOverlap` detection algorithms  # Implemented in multi_watch.rs (see detect_overlap, WatchOverlap)
- [x] Implement automatic shared cache optimization  # Implemented: optimize_shared_cache, merge_nodes_to_shared, cleanup_redundant_and_orphaned_nodes
- [x] Add overlap statistics and reporting  # Implemented: compute_overlap_statistics, logs in optimize_shared_cache
- [x] Improve robustness, error handling, and atomicity of merge/split logic  # Complete: input validation, error handling, and transactional safety added (June 2025)
- [x] Production-grade background optimization scheduler  # Complete: background task with interval and shutdown support (June 2025)

## Phase 3: Filesystem Cache API

### 3.1 Query Interface
- [x] Implement single-watch filesystem queries (`list_directory_for_watch`)
- [ ] Add single-node query (`get_node`)  # TODO: Not yet implemented
- [x] Add unified cross-watch queries (`list_directory_unified`, `get_unified_node`)
- [x] Create hierarchical operations (ancestors, descendants, subtree)
- [ ] Add pattern-based search and filtering  # Partial: stub and partial impl exist, not robust

### 3.2 Cache Synchronization
- [ ] Integrate with existing file watcher events
- [ ] Implement incremental cache updates on filesystem changes
- [ ] Add cache invalidation strategies
- [ ] Create cache consistency verification

### 3.3 Performance Operations
- [ ] Add bulk cache warming operations
- [ ] Implement background cache maintenance
- [ ] Create cache statistics and monitoring
- [ ] Add memory usage optimization

## Phase 4: Integration with Existing Systems

### 4.1 Watcher Integration
- [ ] Update `WatcherConfig` to include filesystem cache options
- [ ] Modify `start()` function to optionally enable filesystem caching
- [ ] Integrate cache with move detection for improved accuracy
- [ ] Add cache-aware directory monitoring

### 4.2 Database Adapter Enhancement
- [ ] Extend `DatabaseAdapter` to support filesystem cache
- [ ] Add cache configuration options to `DatabaseConfig`
- [ ] Implement cache health checks and diagnostics
- [ ] Add cache backup and restore operations

### 4.3 Configuration and CLI
- [ ] Add filesystem cache configuration options
- [ ] Create cache management CLI commands
- [ ] Add cache statistics and monitoring endpoints
- [ ] Implement cache export/import functionality

## Phase 5: Advanced Features and Optimization

### 5.1 Transaction Coordination
- [ ] Implement `MultiWatchTransactionCoordinator`
- [ ] Add deadlock detection and prevention
- [ ] Create transaction timeout and retry mechanisms
- [ ] Add distributed lock coordination for shared resources

### 5.2 Memory Management
- [ ] Implement LRU eviction policies for cache management
- [ ] Add configurable memory limits and pressure handling
- [ ] Create background cache compaction
- [ ] Add cache preloading strategies for hot paths

### 5.3 Performance Monitoring
- [ ] Create detailed performance metrics collection
- [ ] Add cache hit/miss ratio tracking
- [ ] Implement query performance profiling
- [ ] Add database size and growth monitoring

## Phase 6: Testing and Validation

### 6.1 Unit Tests
- [ ] Test all filesystem cache data structures
- [ ] Test multi-watch database operations
- [ ] Test overlap detection and optimization algorithms
- [ ] Test transaction coordination and consistency

### 6.2 Integration Tests
- [ ] Test integration with file watcher events
- [ ] Test cache synchronization under heavy load
- [ ] Test overlapping watch scenarios
- [ ] Test cache recovery and error handling

### 6.3 Performance Tests
- [ ] Benchmark cache vs filesystem I/O performance
- [ ] Test memory usage with large directory trees
- [ ] Benchmark multi-watch query performance
- [ ] Test cache scalability with increasing watch count

### 6.4 End-to-End Validation
- [ ] Test complete filesystem monitoring with cache enabled
- [ ] Validate move detection accuracy improvements
- [ ] Test cache consistency during filesystem stress
- [ ] Validate production-ready performance characteristics

---

## Immediate Next Steps (as of June 2025)

1. **Finish Phase 3 API**: Complete all single-watch and cross-watch query methods, ensure hierarchical and pattern-based queries are robust and tested.
2. **Cache Synchronization**: Integrate cache with watcher event stream for real-time updates and invalidation.
3. **Expand Tests**: Add/expand tests for all new API features, especially for edge cases and concurrency.
4. **Begin Phase 4 Integration**: Start wiring cache options into `WatcherConfig` and CLI, and add health/diagnostic endpoints.
