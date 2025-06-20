# TODO: Filesystem Cache Implementation

## Phase 0: Pre-Implementation Tasks (Critical)

### 0.1 Dependencies and Infrastructure
- [x] ~~Add `walkdir = "2"` to Cargo.toml for filesystem traversal~~ (already present)
- [x] ~~Add `chrono` for timestamp handling in filesystem cache~~ (already present)
- [ ] Verify ReDB performance characteristics with larger datasets (>100k files)
- [ ] Test multimap table scalability with deep directory hierarchies

### 0.2 Database Foundation Validation
- [x] ~~Fix existing database test failure in `test_database_integration`~~ (retention policy bug fixed)
- [x] ~~Validate current ReDB schema supports concurrent read/write operations~~ (concurrency tests pass)
- [x] ~~Test database recovery after unclean shutdown scenarios~~ (recovery test implemented and passing)
- [x] ~~Benchmark current database operations to establish baseline~~ (benchmarks created)

### 0.3 Architecture Validation
- [x] ~~Create proof-of-concept filesystem scanning with `walkdir`~~ (POC implemented with tests)
- [ ] Test path normalization edge cases (symlinks, junction points, UNC paths)
- [ ] Validate cross-platform path handling (Windows vs Unix)
- [ ] Test memory usage patterns with large directory trees (>1M files)

### 0.4 Testing Infrastructure
- [ ] Create test directory structures with known properties for validation
- [ ] Add performance benchmarks for filesystem operations
- [ ] Create integration test framework for multi-watch scenarios
- [ ] Add stress tests for concurrent cache access patterns

## Phase 1: Core Data Structures and Storage Layer

### 1.1 Extend Database Types
- [ ] Add `FilesystemNode`, `NodeType`, `NodeMetadata`, `CacheInfo`, `ComputedProperties` to `database/types.rs`
- [ ] Add `WatchScopedKey`, `FilesystemKey` enums for multi-watch key management
- [ ] Add `SharedNodeInfo`, `WatchMetadata`, `UnifiedNode` structures
- [ ] Update `StorageKey` enum with filesystem cache variants

### 1.2 Database Schema Extension
- [ ] Add filesystem cache table definitions to `database/storage.rs`
- [ ] Implement `MULTI_WATCH_FS_CACHE`, `MULTI_WATCH_HIERARCHY`, `SHARED_NODES` tables
- [ ] Add `WATCH_REGISTRY`, `PATH_TO_WATCHES` multimap tables
- [ ] Update database initialization to create new tables

### 1.3 Core Storage Operations
- [ ] Implement `FilesystemNode` serialization/deserialization
- [ ] Add filesystem cache storage methods to `DatabaseStorage` trait
- [ ] Implement watch-scoped key generation and path hashing
- [ ] Add batch insert operations for initial tree caching

## Phase 2: Multi-Watch Database Management

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
