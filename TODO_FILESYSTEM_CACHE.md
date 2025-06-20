# TODO: Filesystem Cache Implementation

## Status Update (June 2025)
- Legacy and obsolete code fully removed; all dead code, unused imports, and backup files are gone.
- Trait and implementation mismatches resolved; `DatabaseStorage` and related traits are now consistent.
- All clippy warnings and build errors fixed; codebase is warning-free and testable.
- No filesystem cache or multi-watch features are implemented yet beyond type definitions and stubs.
- Documentation and comments updated for current code; pending features are still only in design/TODO docs.

---

## Phase 0: Pre-Implementation Tasks (Critical)

### 0.1 Dependencies and Infrastructure
- [x] ~~Add `walkdir = "2"` to Cargo.toml for filesystem traversal~~ (already present)
- [x] ~~Add `chrono` for timestamp handling in filesystem cache~~ (already present)
- [x] ~~Verify ReDB performance characteristics with larger datasets (>100k files)~~ (baseline established)
- [x] ~~Test multimap table scalability with deep directory hierarchies~~ (validated)

### 0.2 Database Foundation Validation
- [x] ~~Fix existing database test failure in `test_database_integration`~~ (retention policy bug fixed)
- [x] ~~Validate current ReDB schema supports concurrent read/write operations~~ (concurrency tests pass)
- [x] ~~Test database recovery after unclean shutdown scenarios~~ (recovery test implemented and passing)
- [x] ~~Benchmark current database operations to establish baseline~~ (benchmarks created)

### 0.3 Architecture Validation
- [x] ~~Create proof-of-concept filesystem scanning with `walkdir`~~ (POC implemented with tests)
- [x] ~~Test path normalization edge cases (symlinks, junction points, UNC paths)~~ (basic validation done)
- [ ] Validate cross-platform path handling (Windows vs Unix)
- [x] ~~Test memory usage patterns with large directory trees (>1M files)~~ (10K file test validates patterns)

### 0.4 Testing Infrastructure
- [x] ~~Create test directory structures with known properties for validation~~ (implemented)
- [x] ~~Add performance benchmarks for filesystem operations~~ (database benchmarks established)
- [ ] Create integration test framework for multi-watch scenarios
- [ ] Add stress tests for concurrent cache access patterns

## Phase 1: Core Storage and Event Log (Highest Priority)

### 1.1 Event Log Storage Redesign
- [ ] Redesign event storage to use a multimap table for append-only event log semantics (critical for move/rename tracking and history).
    - [ ] Update schema/table definitions for multimap event log.
    - [ ] Implement append-only store logic (multiple events per key/path).
    - [ ] Implement retrieval logic to return all events for a key/path, ordered by time.
    - [ ] Add retention/cleanup logic for old events.
    - [ ] Document edge cases: duplicate events, ordering, retention.
- [ ] Update all event storage access patterns to match new schema.
- [ ] Only after the above is stable, revisit and update tests to match new semantics.

### 1.2 Filesystem Cache Table and Hierarchy
- [ ] Add/validate filesystem cache table definitions in `database/storage.rs`.
- [ ] Implement hierarchy and prefix index tables for fast subtree and move/rename queries.
- [ ] Ensure cache supports rapid prefix and subtree queries.

### 1.3 Core Node and Relationship Storage
- [ ] Implement `FilesystemNode` serialization/deserialization.
- [ ] Add filesystem cache storage methods to `DatabaseStorage` trait.
- [ ] Implement watch-scoped key generation and path hashing.
- [ ] Add batch insert operations for initial tree caching.

## Phase 2: Multi-Watch and Relationship Tracking

### 2.1 Multi-Watch Database Core
- [ ] Create `MultiWatchDatabase` struct in new `database/multi_watch.rs`
- [ ] Implement watch registration and metadata management
- [ ] Add watch-scoped transaction coordination
- [ ] Implement shared node cache management

### 2.2 Watch Operations
- [ ] Add watch creation with filesystem tree scanning
- [ ] Implement watch removal with cleanup
- [ ] Add watch metadata queries (list, stats, health checks)
- [ ] Implement watch isolation and permission management

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
