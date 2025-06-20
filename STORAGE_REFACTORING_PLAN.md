# Storage Module Refactoring Plan

## Problem Statement
The current `storage.rs` file is becoming monolithic and will become unmaintainable as we implement the full filesystem cache design. We need a modular, scalable architecture.

## Proposed Module Structure

```
src/database/storage/
├── mod.rs                    # Public API and trait definitions
├── core.rs                   # Core storage trait and utilities
├── tables.rs                 # All ReDB table definitions
├── event_storage.rs          # Event record operations
├── metadata_storage.rs       # Metadata record operations  
├── filesystem_cache.rs       # Filesystem node cache operations
├── multi_watch.rs            # Multi-watch database management
├── indexing.rs               # Secondary indexes and range queries
├── maintenance.rs            # Cleanup, compaction, statistics
└── transactions.rs           # Transaction management utilities
```

## Modular Design Principles

### 1. **Separation of Concerns**
- **Event Storage**: Basic event CRUD operations
- **Metadata Storage**: File metadata caching
- **Filesystem Cache**: Complete tree structure caching
- **Multi-Watch**: Cross-watch coordination and shared caches
- **Indexing**: Secondary indexes for efficient queries
- **Maintenance**: Background operations and cleanup

### 2. **Trait Composition**
Instead of one massive `DatabaseStorage` trait, use smaller focused traits:

```rust
// Core storage operations
pub trait EventStorage: Send + Sync {
    async fn store_event(&mut self, record: &EventRecord) -> DatabaseResult<()>;
    async fn get_events(&mut self, key: &StorageKey) -> DatabaseResult<Vec<EventRecord>>;
    // ... other event operations
}

// Filesystem cache operations
pub trait FilesystemCacheStorage: Send + Sync {
    async fn store_filesystem_node(&mut self, watch_id: &Uuid, node: &FilesystemNode) -> DatabaseResult<()>;
    async fn get_filesystem_node(&mut self, watch_id: &Uuid, path: &Path) -> DatabaseResult<Option<FilesystemNode>>;
    // ... other cache operations
}

// Multi-watch operations
pub trait MultiWatchStorage: Send + Sync {
    async fn register_watch(&mut self, metadata: &WatchMetadata) -> DatabaseResult<()>;
    async fn unregister_watch(&mut self, watch_id: &Uuid) -> DatabaseResult<()>;
    // ... other multi-watch operations
}

// Main storage trait combines all capabilities
pub trait DatabaseStorage: EventStorage + FilesystemCacheStorage + MultiWatchStorage + Send + Sync {
    async fn initialize(&mut self) -> DatabaseResult<()>;
    async fn get_stats(&self) -> DatabaseResult<DatabaseStats>;
    async fn compact(&mut self) -> DatabaseResult<()>;
    async fn close(self) -> DatabaseResult<()>;
}
```

### 3. **Implementation Strategy**
```rust
pub struct RedbStorage {
    database: Arc<Database>,
    config: DatabaseConfig,
    // Module-specific implementations
    event_storage: EventStorageImpl,
    filesystem_cache: FilesystemCacheImpl,
    multi_watch: MultiWatchImpl,
    indexing: IndexingImpl,
    maintenance: MaintenanceImpl,
}
```

## Implementation Phases

### Phase 1: Foundation Setup (Current Priority)
1. **Create module structure** - Set up the directory and mod.rs
2. **Extract table definitions** - Move all const TABLE definitions to tables.rs
3. **Create core traits** - Define the smaller, focused traits
4. **Transaction utilities** - Common transaction patterns and error handling

### Phase 2: Extract Existing Functionality
1. **Event storage module** - Move existing event operations
2. **Metadata storage module** - Move existing metadata operations
3. **Basic indexing** - Extract size and time range queries
4. **Maintenance operations** - Extract cleanup and stats

### Phase 3: Filesystem Cache Implementation
1. **Filesystem cache storage** - Implement the full cache as designed
2. **Multi-watch management** - Cross-watch coordination
3. **Advanced indexing** - Hierarchical and prefix-based queries
4. **Performance optimization** - Batch operations and caching

### Phase 4: Integration and Testing
1. **Adapter integration** - Update DatabaseAdapter to use new traits
2. **Comprehensive testing** - Test each module independently
3. **Performance validation** - Ensure no regressions
4. **Documentation** - Document the modular architecture

## Benefits of This Approach

### **Maintainability**
- Each module has a single responsibility
- Easier to understand, test, and modify individual components
- Clear interfaces between components

### **Scalability** 
- New storage capabilities can be added as new modules
- Different storage backends could implement the same traits
- Easy to optimize individual components

### **Testing**
- Each module can be unit tested independently
- Mock implementations for integration testing
- Focused test coverage per responsibility

### **Performance**
- Specialized implementations per concern
- Better compiler optimization opportunities
- Easier to identify and fix performance bottlenecks

## Migration Strategy

1. **Gradual migration** - Move functionality module by module
2. **Backward compatibility** - Keep existing API working during transition
3. **Feature flags** - Allow switching between old and new implementations
4. **Comprehensive testing** - Ensure no functionality is lost

## Next Steps

1. Create the module directory structure
2. Extract table definitions first (least risky)
3. Create trait definitions and basic module shells
4. Gradually move existing functionality
5. Implement new filesystem cache features in the modular structure
