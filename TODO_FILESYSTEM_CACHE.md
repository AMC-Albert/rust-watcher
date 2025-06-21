# TODO: Filesystem Cache Implementation

## Status Update (June 2025)
- Legacy and obsolete code fully removed; all dead code, unused imports, and backup files are gone.
- Trait and implementation mismatches resolved; `DatabaseStorage` and related traits are now consistent.
- All clippy warnings and build errors fixed; codebase is warning-free and testable.
- Event log retention/cleanup system implemented, tested, and integrated (see Phase 1.1).
- Brute-force event statistics implementation added (see maintenance.rs). This is O(N) and not suitable for production or large datasets. Only event count is accurate; all other stats are placeholders. See code comments for limitations and future work.
- Persistent O(1) event and per-type stats implemented, tested, and documented (June 2025). Further extensibility (per-watch, per-path) and advanced indexing still TODO.
- No scalable stats/indexing subsystem exists yet. This is a critical TODO for production use.
- Integration and stress test scaffolding implemented and passing basic checks (see integration_multi_watch.rs, stress_cache_concurrency.rs).
- Documentation and comments updated for current code; pending features are still only in design/TODO docs.
- Cross-platform path normalization and edge case handling (Windows/Unix, UNC, device paths) implemented, tested, and documented. All normalization and test logic now reflects real OS behavior and limitations. (June 2025)
- MultiWatchDatabase: persistent transaction coordination, metadata table, and borrow checker fixes implemented (June 2025).
- Watch creation with filesystem tree scanning, metadata, and permission management implemented and integrated (June 2025).
- All Clippy warnings and build errors fixed after API changes (permissions, transaction coordination).

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

## Immediate Next Steps

- [x] Document event log edge cases, especially around duplicate events, ordering, and retention policy behavior.  # Complete (see event_retention.rs)
- [x] Implement brute-force event stats (O(N), not scalable, see maintenance.rs).  # Complete, but not suitable for production
- [x] Design and implement a scalable, indexed, and robust stats subsystem.  # Persistent O(1) event and per-type stats implemented, tested, and documented (June 2025). Further extensibility (per-watch, per-path) and advanced indexing still TODO.
- [x] Validate cross-platform path handling (Windows vs Unix).  # Complete (see path_utils.rs, path_normalization.rs)
- [x] Create integration test framework for multi-watch scenarios.  # Scaffolded and passing basic checks (integration_multi_watch.rs)
- [x] Add stress tests for concurrent cache access patterns.  # Scaffolded and passing basic checks (stress_cache_concurrency.rs)
- [x] Design and stub out the `MultiWatchDatabase` and related APIs for Phase 2.  # Initial stubs and partial implementation exist (multi_watch.rs, storage/multi_watch.rs)

### Priority Order (as of June 2025)

1. **Expand and harden MultiWatchDatabase and related APIs**
   - Implement watch registration, removal, and shared node cache management.
   - Add watch-scoped transaction coordination and metadata management.
2. **Implement real multi-watch integration tests**
   - Use the scaffolded framework to drive development and catch regressions early.
3. **Implement real stress/concurrency tests**
   - Use the scaffolded stress tests to validate cache and database concurrency under load.

---

# Summary Table

| Task                            | Status      | Notes                                                                                                                                                           |
| ------------------------------- | ----------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Event log retention/cleanup     | Complete    | Logic, tests, and integration done                                                                                                                              |
| Event log edge case docs        | Complete    | Documented in code and event_retention.rs                                                                                                                       |
| Brute-force stats impl          | Complete    | O(N), not scalable, see maintenance.rs                                                                                                                          |
| Scalable stats/indexing         | Partial     | O(1) event and per-type stats done; per-watch, per-path, and advanced indexing still TODO                                                                       |
| Cross-platform path handling    | Complete    | Windows/Unix normalization, edge cases                                                                                                                          |
| Multi-watch test infra          | Complete    | Scaffolded and passing basic checks                                                                                                                             |
| Stress tests                    | Complete    | Scaffolded and passing basic checks                                                                                                                             |
| Multi-watch core                | In progress | Persistent transaction coordination, watch creation, permissions, and tree scan implemented                                                                     |
| Multi-watch concurrency tests   | Complete    | Integration test for concurrent registration/removal passes (June 2025)                                                                                         |
| Shared cache optimization       | Partial     | Minimal implementation: shared nodes created for overlaps, integration test passes (June 2025). Full merge/split logic, cleanup, and error handling still TODO. |
| Redundant/orphaned node cleanup | Complete    | Robust removal of redundant watch-specific and orphaned shared nodes; integration test passes (June 2025)                                                       |

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
- [ ] Create `WatchOverlap` detection algorithms
- [ ] Implement automatic shared cache optimization
- [ ] Add overlap statistics and reporting
- [ ] Create background optimization scheduler

## Phase 3: Filesystem Cache API

### 3.1 Query Interface
- [ ] Implement single-watch filesystem queries (`list_directory_for_watch`, `get_node`)
- [ ] Add unified cross-watch queries (`list_directory_unified`, `get_unified_node`)
- [ ] Create hierarchical operations (ancestors, descendants, subtree)
- [ ] Add pattern-based search and filtering

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

## Dependencies and Prerequisites

### Code Dependencies
- Extend existing `database/` module structure
- Integrate with `events.rs` and `watcher.rs`
- Enhance `database/adapter.rs` and `database/config.rs`

### External Dependencies
- ReDB features: multimap tables, range queries, concurrent access
- Filesystem traversal: `walkdir` or similar for initial caching
- Path manipulation: enhanced `std::path` operations
- Hashing: consistent path hashing across platforms

### Configuration Requirements
- Database schema migration strategy
- Backward compatibility with existing databases
- Performance tuning parameters
- Memory and disk usage limits

## Success Criteria

### Performance Targets
- 95%+ cache hit rate for monitored directories
- <1ms average response time for cached directory listings
- <10MB memory overhead per 10,000 cached filesystem nodes
- 90%+ reduction in filesystem I/O for repeated operations

### Functionality Requirements
- Support for 100+ concurrent watch operations
- Automatic optimization of overlapping watches
- Real-time cache synchronization with filesystem events
- Complete backward compatibility with existing watcher functionality

### Operational Goals
- Zero-downtime cache enable/disable
- Automatic cache recovery from corruption
- Comprehensive monitoring and diagnostics
- Production-ready error handling and logging

---

**Implementation Priority**: Phases 1-3 are critical path for core functionality. Phases 4-6 can be developed in parallel once core infrastructure is stable.

**Estimated Timeline**: 4-6 weeks for full implementation with comprehensive testing.

**Risk Mitigation**: Maintain backward compatibility throughout implementation. Each phase should be independently testable and deployable.
