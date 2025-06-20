# Filesystem Structure Caching Design

## High-Level Design Goals (2025 Revision)

- Efficiently monitor large filesystems for move and rename operations, with minimal latency and overhead.
- Maintain a fast, scalable, and consistent cache of the entire filesystem hierarchy, supporting instant subtree and prefix queries.
- Support append-only, event-log style storage for all file events, enabling full history and robust move/rename detection.
- Design for high write throughput and concurrent access, as real-world filesystems generate large volumes of events.
- Lay the groundwork for tracking complex, linked relationships between files and dependencies (e.g., symlinks, hard links, application-level references).
- Prioritize correctness and durability over premature optimization; edge-case features (deep dependency graphs, advanced analytics) should not compromise core reliability.
- All schema and storage primitives should be designed for extensibility, anticipating future needs for relationship tracking and advanced queries.

## Objective
Cache the entire watched filesystem structure in ReDB for maximum performance, enabling instant directory traversal, file lookups, and hierarchical queries without filesystem I/O.

## ReDB Features Leveraged

### Core Features
- **Zero-copy deserialization**: Direct access to stored data without copying
- **Concurrent read access**: Multiple readers without blocking
- **Range queries**: Efficient prefix-based lookups for directory hierarchies
- **Multimap tables**: One-to-many relationships for parent-child mappings
- **Durability controls**: Configurable sync policies for performance vs safety

### Performance Characteristics
- **Read-optimized**: B+ tree structure optimized for range scans
- **Memory efficient**: Pages loaded on-demand, not entire database
- **Lock-free reads**: Multiple concurrent readers without contention
- **Write batching**: Transaction-based writes for consistency

## Data Model

### Table Structure
```rust
// Primary filesystem cache table
const FS_CACHE_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("fs_cache");

// Directory hierarchy multimap (parent_path -> [child_paths])
const HIERARCHY_TABLE: MultimapTableDefinition<&[u8], &[u8]> = 
    MultimapTableDefinition::new("hierarchy");

// Reverse lookup multimap (child_path -> parent_path)
const PARENT_LOOKUP_TABLE: TableDefinition<&[u8], &[u8]> = 
    TableDefinition::new("parent_lookup");

// Path prefix index for efficient subtree operations
const PATH_PREFIX_TABLE: MultimapTableDefinition<&[u8], &[u8]> = 
    MultimapTableDefinition::new("path_prefix");
```

### Core Data Structures

#### FilesystemNode
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemNode {
    /// Canonical absolute path
    pub path: PathBuf,
    
    /// Node type and metadata
    pub node_type: NodeType,
    
    /// File system metadata
    pub metadata: NodeMetadata,
    
    /// Caching metadata
    pub cache_info: CacheInfo,
    
    /// Computed properties
    pub computed: ComputedProperties,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeType {
    File {
        size: u64,
        content_hash: Option<String>,
        mime_type: Option<String>,
    },
    Directory {
        child_count: u32,
        total_size: u64,
        max_depth: u16,
    },
    Symlink {
        target: PathBuf,
        resolved: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeMetadata {
    pub modified_time: SystemTime,
    pub created_time: Option<SystemTime>,
    pub accessed_time: Option<SystemTime>,
    pub permissions: u32,
    pub inode: Option<u64>,
    pub windows_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheInfo {
    pub cached_at: DateTime<Utc>,
    pub last_verified: DateTime<Utc>,
    pub cache_version: u32,
    pub needs_refresh: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputedProperties {
    pub depth_from_root: u16,
    pub path_hash: u64,
    pub parent_hash: Option<u64>,
    pub canonical_name: String,
}
```

### Storage Keys

#### Primary Key Strategy
```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FilesystemKey {
    /// Direct path lookup (most common)
    Path(PathBuf),
    
    /// Hash-based lookup for performance
    PathHash(u64),
    
    /// Inode lookup (Unix systems)
    Inode(u64),
    
    /// Windows file ID lookup
    WindowsId(u64),
    
    /// Parent directory lookup
    ParentPath(PathBuf),
    
    /// Prefix lookup for subtree operations
    PathPrefix(String),
    
    /// Depth-based lookup for tree traversal
    DepthLevel(u16),
}
```

## Indexing Strategy

### 1. Primary Path Index
- **Key**: Normalized canonical path bytes
- **Value**: Complete FilesystemNode
- **Use**: Direct O(log n) path lookups

### 2. Hierarchy Index (Multimap)
- **Key**: Parent path hash
- **Value**: Child path hash
- **Use**: O(log n) directory listing without filesystem I/O

### 3. Path Prefix Index (Multimap)
- **Key**: Path prefix (e.g., "/home/user")
- **Value**: Full paths starting with prefix
- **Use**: Efficient subtree operations and recursive directory listing

### 4. Reverse Parent Lookup
- **Key**: Child path hash
- **Value**: Parent path hash
- **Use**: Fast upward traversal and path resolution

### 5. Depth-Level Index (Multimap)
- **Key**: Depth level (u16)
- **Value**: Path hashes at that depth
- **Use**: Level-order tree traversal and depth-limited operations

## Query Patterns

### Directory Listing
```rust
// Get immediate children of directory
async fn list_directory(&self, parent_path: &Path) -> Result<Vec<FilesystemNode>> {
    let parent_hash = hash_path(parent_path);
    let hierarchy_table = self.read_txn.open_multimap_table(HIERARCHY_TABLE)?;
    let fs_cache_table = self.read_txn.open_table(FS_CACHE_TABLE)?;
    
    let child_hashes = hierarchy_table.get(parent_hash.to_be_bytes())?;
    
    // Zero-copy parallel lookup of child nodes
    let nodes = child_hashes
        .map(|hash| fs_cache_table.get(hash?.value()))
        .collect::<Result<Vec<_>>>()?;
    
    Ok(nodes)
}
```

### Recursive Directory Traversal
```rust
// Get entire subtree efficiently using prefix index
async fn get_subtree(&self, root_path: &Path) -> Result<Vec<FilesystemNode>> {
    let prefix = root_path.to_string_lossy();
    let prefix_table = self.read_txn.open_multimap_table(PATH_PREFIX_TABLE)?;
    let fs_cache_table = self.read_txn.open_table(FS_CACHE_TABLE)?;
    
    // Range query for all paths with this prefix
    let matching_paths = prefix_table.get(prefix.as_bytes())?;
    
    // Parallel lookup of all nodes in subtree
    let nodes = matching_paths
        .map(|path| fs_cache_table.get(path?.value()))
        .collect::<Result<Vec<_>>>()?;
        
    Ok(nodes)
}
```

### Path Resolution
```rust
// Fast parent chain lookup
async fn get_path_chain(&self, path: &Path) -> Result<Vec<FilesystemNode>> {
    let mut chain = Vec::new();
    let mut current_hash = hash_path(path);
    let parent_table = self.read_txn.open_table(PARENT_LOOKUP_TABLE)?;
    let fs_cache_table = self.read_txn.open_table(FS_CACHE_TABLE)?;
    
    while let Some(node_bytes) = fs_cache_table.get(current_hash.to_be_bytes())? {
        let node: FilesystemNode = bincode::deserialize(node_bytes.value())?;
        chain.push(node);
        
        if let Some(parent_bytes) = parent_table.get(current_hash.to_be_bytes())? {
            current_hash = u64::from_be_bytes(parent_bytes.value().try_into()?);
        } else {
            break;
        }
    }
    
    chain.reverse();
    Ok(chain)
}
```

## Performance Optimizations

### 1. Path Normalization
- Canonical path representation using platform-specific separators
- Hash-based keys for O(1) average lookup performance
- Path interning for reduced memory usage

### 2. Batch Operations
```rust
// Bulk directory scan and cache
async fn cache_directory_tree(&mut self, root: &Path) -> Result<CacheStats> {
    let mut batch = Vec::new();
    let mut hierarchy_batch = Vec::new();
    
    // Single filesystem traversal
    for entry in WalkDir::new(root).into_iter() {
        let entry = entry?;
        let node = FilesystemNode::from_path(entry.path())?;
        
        // Prepare batch insert
        let key = node.computed.path_hash.to_be_bytes();
        let value = bincode::serialize(&node)?;
        batch.push((key, value));
        
        // Prepare hierarchy relationships
        if let Some(parent_hash) = node.computed.parent_hash {
            hierarchy_batch.push((
                parent_hash.to_be_bytes(),
                node.computed.path_hash.to_be_bytes()
            ));
        }
    }
    
    // Single transaction for entire tree
    let write_txn = self.database.begin_write()?;
    {
        let mut fs_table = write_txn.open_table(FS_CACHE_TABLE)?;
        let mut hierarchy_table = write_txn.open_multimap_table(HIERARCHY_TABLE)?;
        
        // Bulk insert nodes
        for (key, value) in batch {
            fs_table.insert(key, value)?;
        }
        
        // Bulk insert hierarchy
        for (parent_key, child_key) in hierarchy_batch {
            hierarchy_table.insert(parent_key, child_key)?;
        }
    }
    write_txn.commit()?;
    
    Ok(CacheStats { nodes_cached: batch.len(), time_taken: start.elapsed() })
}
```

### 3. Incremental Updates
```rust
// Efficient cache invalidation and update
async fn update_node(&mut self, path: &Path, fs_metadata: &Metadata) -> Result<()> {
    let path_hash = hash_path(path);
    let write_txn = self.database.begin_write()?;
    
    {
        let mut fs_table = write_txn.open_table(FS_CACHE_TABLE)?;
        
        if let Some(existing_bytes) = fs_table.get(path_hash.to_be_bytes())? {
            let mut node: FilesystemNode = bincode::deserialize(existing_bytes.value())?;
            
            // Update only changed fields
            node.metadata = NodeMetadata::from_std_metadata(fs_metadata);
            node.cache_info.last_verified = Utc::now();
            node.cache_info.needs_refresh = false;
            
            // Atomic update
            let updated_bytes = bincode::serialize(&node)?;
            fs_table.insert(path_hash.to_be_bytes(), updated_bytes)?;
        }
    }
    
    write_txn.commit()?;
    Ok(())
}
```

### 4. Memory Management
- Use ReDB's zero-copy reads to minimize allocation
- Lazy loading of directory contents
- LRU eviction for least-recently-used subtrees
- Configurable cache size limits

## Cache Consistency

### 1. Filesystem Sync Strategy
```rust
pub enum SyncStrategy {
    /// Immediate: Update cache on every filesystem event
    Immediate,
    /// Batched: Collect events and batch update every N milliseconds
    Batched { interval_ms: u64, max_batch_size: usize },
    /// Lazy: Update cache only when accessed and stale
    Lazy { max_staleness: Duration },
    /// Periodic: Full rescan at regular intervals
    Periodic { rescan_interval: Duration },
}
```

### 2. Conflict Resolution
- Filesystem events take precedence over cached data
- Last-writer-wins for concurrent updates
- Versioned cache entries for optimistic locking
- Automatic re-scan on detection of cache inconsistency

### 3. Error Handling
- Graceful degradation to filesystem I/O on cache miss
- Background cache repair on corruption detection
- Atomic transactions prevent partial updates
- Cache warming strategies for critical paths

## Integration Points

### 1. File Watcher Integration
```rust
impl FilesystemCache {
    async fn handle_fs_event(&mut self, event: &FileSystemEvent) -> Result<()> {
        match event.event_type {
            EventType::Create => self.add_node(&event.path).await?,
            EventType::Remove => self.remove_node(&event.path).await?,
            EventType::Rename => self.rename_node(&event.path, &new_path).await?,
            EventType::Write => self.update_node(&event.path).await?,
            _ => {}
        }
        Ok(())
    }
}
```

### 2. Move Detection Enhancement
- Cache-based move detection using inode/file_id tracking
- Historical path mapping for improved move confidence
- Cross-directory move detection without filesystem scanning

### 3. Query API
```rust
impl FilesystemCache {
    // Fast directory operations
    async fn list_dir(&self, path: &Path) -> Result<Vec<FilesystemNode>>;
    async fn get_subtree(&self, path: &Path, max_depth: Option<u16>) -> Result<Vec<FilesystemNode>>;
    async fn find_by_pattern(&self, pattern: &str) -> Result<Vec<FilesystemNode>>;
    
    // Metadata operations
    async fn get_node(&self, path: &Path) -> Result<Option<FilesystemNode>>;
    async fn exists(&self, path: &Path) -> Result<bool>;
    async fn is_dir(&self, path: &Path) -> Result<bool>;
    
    // Tree operations
    async fn get_ancestors(&self, path: &Path) -> Result<Vec<FilesystemNode>>;
    async fn get_descendants(&self, path: &Path, max_depth: Option<u16>) -> Result<Vec<FilesystemNode>>;
    
    // Statistics
    async fn get_tree_stats(&self, path: &Path) -> Result<TreeStats>;
    async fn get_cache_stats(&self) -> Result<CacheStats>;
}
```

# Multi-Database Management for Overlapping Watch Operations

## Problem Analysis

### Scenarios to Support
1. **Separate Non-Overlapping Watches**: `/home/user/documents` and `/var/log`
2. **Nested Overlapping Watches**: `/home/user` and `/home/user/projects`
3. **Cross-Overlapping Watches**: `/home/user/shared` and `/opt/shared`
4. **Dynamic Watch Addition/Removal**: Runtime watch management
5. **Concurrent Multi-User Scenarios**: Different processes watching same areas

## ReDB Advanced Features Analysis

### 1. Single Database with Table Namespacing
**ReDB Feature**: Multiple table definitions in single database file
```rust
// Watch-specific table namespacing
const WATCH_1_FS_CACHE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("watch_1_fs_cache");
const WATCH_1_HIERARCHY: MultimapTableDefinition<&[u8], &[u8]> = MultimapTableDefinition::new("watch_1_hierarchy");
const WATCH_2_FS_CACHE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("watch_2_fs_cache");
const WATCH_2_HIERARCHY: MultimapTableDefinition<&[u8], &[u8]> = MultimapTableDefinition::new("watch_2_hierarchy");

// Shared global table for cross-watch queries
const GLOBAL_FS_CACHE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("global_fs_cache");
const GLOBAL_WATCH_REGISTRY: TableDefinition<&[u8], &[u8]> = TableDefinition::new("watch_registry");
```

**Advantages**:
- Single file handle and connection pool
- Atomic cross-watch transactions
- Shared filesystem nodes reduce duplication
- ACID guarantees across all watches

**Disadvantages**:
- Table name explosion with many watches
- Potential lock contention on single database
- Memory overhead for unused watch tables

### 2. Multiple Database Files
**ReDB Feature**: Separate database instances with concurrent access
```rust
// Per-watch database files
let watch_1_db = Database::create("/path/to/watch_1.db")?;
let watch_2_db = Database::create("/path/to/watch_2.db")?;

// Shared coordination database
let coordination_db = Database::create("/path/to/coordination.db")?;
```

**Advantages**:
- Complete isolation between watches
- Independent backup/recovery per watch
- Parallel I/O across different storage devices
- Easy watch removal (delete database file)

**Disadvantages**:
- No atomic cross-watch operations
- Duplicate data for overlapping paths
- Complex coordination for shared paths
- More file handles and memory usage

### 3. Hybrid: Shared Database with Watch Isolation
**ReDB Feature**: Single database with watch-scoped key prefixes
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchScopedKey {
    pub watch_id: Uuid,
    pub key_type: KeyType,
    pub path_hash: u64,
}

pub enum KeyType {
    FilesystemNode,
    HierarchyParent,
    HierarchyChild,
    PathPrefix,
    WatchMetadata,
}
```

## Recommended Architecture: Hybrid Approach

### Core Design Principles

#### 1. Single Database with Smart Key Scoping
```rust
pub struct MultiWatchDatabase {
    database: Database,
    watch_registry: Arc<RwLock<HashMap<Uuid, WatchMetadata>>>,
    shared_node_cache: Arc<RwLock<HashMap<u64, SharedNodeInfo>>>,

    // New fields for advanced features
    transaction_coordinator: Arc<MultiWatchTransactionCoordinator>,
    overlap_detector: Arc<RwLock<OverlapDetector>>,
}

#[derive(Debug, Clone)]
pub struct WatchMetadata {
    pub watch_id: Uuid,
    pub root_path: PathBuf,
    pub config: WatcherConfig,
    pub created_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
    pub node_count: u64,
}

#[derive(Debug, Clone)]
pub struct SharedNodeInfo {
    pub path_hash: u64,
    pub watching_ids: HashSet<Uuid>,
    pub canonical_node: FilesystemNode,
    pub last_updated: DateTime<Utc>,
}
```

#### 2. Efficient Key Design for Multi-Watch
```rust
// Primary table with watch-scoped keys
const MULTI_WATCH_FS_CACHE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("multi_fs_cache");
const MULTI_WATCH_HIERARCHY: MultimapTableDefinition<&[u8], &[u8]> = MultimapTableDefinition::new("multi_hierarchy");

// Global shared tables
const SHARED_NODES: TableDefinition<&[u8], &[u8]> = TableDefinition::new("shared_nodes");
const WATCH_REGISTRY: TableDefinition<&[u8], &[u8]> = TableDefinition::new("watch_registry");
const PATH_TO_WATCHES: MultimapTableDefinition<&[u8], &[u8]> = MultimapTableDefinition::new("path_to_watches");

impl WatchScopedKey {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(self.watch_id.as_bytes());
        bytes.extend_from_slice(&(self.key_type as u8).to_be_bytes());
        bytes.extend_from_slice(&self.path_hash.to_be_bytes());
        bytes
    }
    
    pub fn global_path_key(path_hash: u64) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[0xFF; 16]); // Reserved UUID for global keys
        bytes.extend_from_slice(&(KeyType::FilesystemNode as u8).to_be_bytes());
        bytes.extend_from_slice(&path_hash.to_be_bytes());
        bytes
    }
}
```

### 3. Advanced Query Patterns for Multi-Watch

#### Cross-Watch Path Resolution
```rust
impl MultiWatchDatabase {
    /// Find all watches monitoring a specific path
    async fn get_watches_for_path(&self, path: &Path) -> Result<Vec<Uuid>> {
        let path_hash = hash_path(path);
        let read_txn = self.database.begin_read()?;
        let path_to_watches = read_txn.open_multimap_table(PATH_TO_WATCHES)?;
        
        let watch_ids = path_to_watches
            .get(path_hash.to_be_bytes().as_slice())?
            .map(|entry| {
                let bytes = entry?.value();
                Ok(Uuid::from_slice(bytes)?)
            })
            .collect::<Result<Vec<_>>>()?;
            
        Ok(watch_ids)
    }
    
    /// Get unified view of path across all watches
    async fn get_unified_node(&self, path: &Path) -> Result<UnifiedNode> {
        let path_hash = hash_path(path);
        let read_txn = self.database.begin_read()?;
        let shared_nodes = read_txn.open_table(SHARED_NODES)?;
        
        if let Some(shared_bytes) = shared_nodes.get(path_hash.to_be_bytes().as_slice())? {
            let shared_info: SharedNodeInfo = bincode::deserialize(shared_bytes.value())?;
            return Ok(UnifiedNode {
                node: shared_info.canonical_node,
                watched_by: shared_info.watching_ids,
                last_updated: shared_info.last_updated,
            });
        }
        
        // Fallback: aggregate from individual watch caches
        self.aggregate_node_from_watches(path).await
    }
}
```

#### Efficient Range Queries with Watch Filtering
```rust
impl MultiWatchDatabase {
    /// List directory contents for specific watch
    async fn list_directory_for_watch(&self, watch_id: Uuid, parent_path: &Path) -> Result<Vec<FilesystemNode>> {
        let parent_hash = hash_path(parent_path);
        let hierarchy_key = WatchScopedKey {
            watch_id,
            key_type: KeyType::HierarchyParent,
            path_hash: parent_hash,
        };
        
        let read_txn = self.database.begin_read()?;
        let hierarchy_table = read_txn.open_multimap_table(MULTI_WATCH_HIERARCHY)?;
        let fs_cache_table = read_txn.open_table(MULTI_WATCH_FS_CACHE)?;
        
        let child_keys = hierarchy_table.get(hierarchy_key.to_bytes().as_slice())?;
        
        let nodes = child_keys
            .map(|key_entry| {
                let child_key = key_entry?.value();
                if let Some(node_bytes) = fs_cache_table.get(child_key)? {
                    let node: FilesystemNode = bincode::deserialize(node_bytes.value())?;
                    Ok(Some(node))
                } else {
                    Ok(None)
                }
            })
            .filter_map(|result| result.transpose())
            .collect::<Result<Vec<_>>>()?;
            
        Ok(nodes)
    }
    
    /// List directory contents unified across all relevant watches
    async fn list_directory_unified(&self, parent_path: &Path) -> Result<Vec<UnifiedNode>> {
        let relevant_watches = self.get_watches_for_path(parent_path).await?;
        let mut unified_nodes = HashMap::new();
        
        for watch_id in relevant_watches {
            let watch_nodes = self.list_directory_for_watch(watch_id, parent_path).await?;
            for node in watch_nodes {
                let path_hash = hash_path(&node.path);
                unified_nodes.entry(path_hash)
                    .and_modify(|unified: &mut UnifiedNode| {
                        unified.watched_by.insert(watch_id);
                        // Merge node data if needed (take most recent)
                        if node.cache_info.last_verified > unified.node.cache_info.last_verified {
                            unified.node = node.clone();
                        }
                    })
                    .or_insert(UnifiedNode {
                        node,
                        watched_by: [watch_id].into_iter().collect(),
                        last_updated: Utc::now(),
                    });
            }
        }
        
        Ok(unified_nodes.into_values().collect())
    }
}
```

### 4. Overlap Detection and Optimization

#### Automatic Overlap Detection
```rust
#[derive(Debug, Clone)]
pub struct WatchOverlap {
    pub watch_1: Uuid,
    pub watch_2: Uuid,
    pub overlap_type: OverlapType,
    pub shared_paths: Vec<PathBuf>,
    pub optimization_potential: f64,
}

#[derive(Debug, Clone)]
pub enum OverlapType {
    /// Watch 2 is completely contained within Watch 1
    NestedChild { parent: Uuid, child: Uuid },
    /// Watch 1 is completely contained within Watch 2
    NestedParent { parent: Uuid, child: Uuid },
    /// Watches have some overlapping paths
    Intersection { intersection_size: usize },
    /// Watches share a common parent directory
    SiblingOverlap { common_ancestor: PathBuf },
}

impl MultiWatchDatabase {
    /// Detect overlaps between watches for optimization
    async fn detect_overlaps(&self) -> Result<Vec<WatchOverlap>> {
        let read_txn = self.database.begin_read()?;
        let watch_registry = read_txn.open_table(WATCH_REGISTRY)?;
        
        let mut watches = Vec::new();
        for entry in watch_registry.iter()? {
            let (_, watch_bytes) = entry?;
            let watch_meta: WatchMetadata = bincode::deserialize(watch_bytes.value())?;
            watches.push(watch_meta);
        }
        
        let mut overlaps = Vec::new();
        for i in 0..watches.len() {
            for j in i+1..watches.len() {
                if let Some(overlap) = self.analyze_watch_overlap(&watches[i], &watches[j]).await? {
                    overlaps.push(overlap);
                }
            }
        }
        
        Ok(overlaps)
    }
    
    /// Optimize storage for detected overlaps
    async fn optimize_overlapping_watches(&mut self, overlaps: &[WatchOverlap]) -> Result<OptimizationStats> {
        let mut stats = OptimizationStats::default();
        
        for overlap in overlaps {
            match &overlap.overlap_type {
                OverlapType::NestedChild { parent, child } => {
                    // Child watch can reference parent's cache
                    stats.nodes_deduplicated += self.deduplicate_nested_watch(*parent, *child).await?;
                }
                OverlapType::Intersection { .. } => {
                    // Move shared paths to global cache
                    stats.nodes_shared += self.extract_shared_paths(&overlap.shared_paths).await?;
                }
                _ => {}
            }
        }
        
        Ok(stats)
    }
}
```

### 5. Transaction Management for Multi-Watch Operations

#### Watch-Aware Transaction Coordinator
```rust
pub struct MultiWatchTransactionCoordinator {
    database: Database,
    active_transactions: Arc<RwLock<HashMap<Uuid, TransactionInfo>>>,

    // New field for advanced transaction features
    global_transaction_id: Arc<RwLock<Uuid>>,
}

#[derive(Debug)]
pub struct TransactionInfo {
    pub watch_ids: HashSet<Uuid>,
    pub transaction_type: TransactionType,
    pub started_at: DateTime<Utc>,
    pub timeout: Duration,
}

#[derive(Debug, Clone)]
pub enum TransactionType {
    /// Read-only operation for single watch
    ReadOnly { watch_id: Uuid },
    /// Write operation for single watch
    SingleWatchWrite { watch_id: Uuid },
    /// Cross-watch operation (requires coordination)
    CrossWatchWrite { watch_ids: HashSet<Uuid> },
    /// Global optimization operation
    GlobalOptimization,
}

impl MultiWatchTransactionCoordinator {
    /// Smart transaction batching for overlapping watches
    async fn execute_batch_operation<F, R>(&self, 
        watch_ids: &[Uuid], 
        operation: F
    ) -> Result<R> 
    where
        F: FnOnce(&WriteTransaction) -> Result<R>,
    {
        // Sort watch IDs for consistent lock ordering (prevents deadlocks)
        let mut sorted_watches = watch_ids.to_vec();
        sorted_watches.sort();
        
        let write_txn = self.database.begin_write()?;
        
        // Register transaction for coordination
        let txn_id = Uuid::new_v4();
        let txn_info = TransactionInfo {
            watch_ids: sorted_watches.iter().cloned().collect(),
            transaction_type: if sorted_watches.len() == 1 {
                TransactionType::SingleWatchWrite { watch_id: sorted_watches[0] }
            } else {
                TransactionType::CrossWatchWrite { watch_ids: sorted_watches.iter().cloned().collect() }
            },
            started_at: Utc::now(),
            timeout: Duration::from_secs(30),
        };
        
        {
            let mut active = self.active_transactions.write().await;
            active.insert(txn_id, txn_info);
        }
        
        let result = operation(&write_txn);
        
        match result {
            Ok(value) => {
                write_txn.commit()?;
                Ok(value)
            }
            Err(e) => {
                // Transaction will be automatically rolled back on drop
                Err(e)
            }
        }?;
        
        // Clean up transaction registration
        {
            let mut active = self.active_transactions.write().await;
            active.remove(&txn_id);
        }
        
        Ok(result?)
    }
}
```

### 6. Performance Characteristics

#### Memory Usage Analysis
```rust
// Single database approach vs multiple databases
pub struct DatabaseMemoryProfile {
    // Per-watch overhead
    pub watch_metadata_size: usize,           // ~200 bytes per watch
    pub key_prefix_overhead: usize,           // ~16 bytes per key (UUID prefix)
    
    // Shared data benefits
    pub shared_node_deduplication: f64,      // 30-70% reduction for overlapping watches
    pub index_sharing_benefit: f64,          // 15-25% reduction in index memory
    
    // Global overhead
    pub coordination_structures: usize,       // ~1KB base + 100 bytes per watch
    pub transaction_coordination: usize,      // ~50 bytes per active transaction
}
```

#### Performance Benchmarks (Projected)
```
| Operation                  | Single DB  | Multiple DB | Improvement          |
| -------------------------- | ---------- | ----------- | -------------------- |
| Watch Registration         | O(log n)   | O(1)        | -                    |
| Path Lookup (Single Watch) | O(log n)   | O(log n)    | Same                 |
| Path Lookup (Cross-Watch)  | O(log n)   | O(k×log n)  | 90%+ faster          |
| Directory Listing          | O(log n+c) | O(log n+c)  | Same                 |
| Overlap Detection          | O(n²)      | N/A         | New capability       |
| Memory Usage (Overlapping) | ~60% less  | Baseline    | Significant          |
| Transaction Coordination   | ACID       | Best-effort | Stronger consistency |
```

## Configuration Strategy

```rust
#[derive(Debug, Clone)]
pub struct MultiWatchConfig {
    /// Database management strategy
    pub database_strategy: DatabaseStrategy,
    
    /// Overlap optimization settings
    pub auto_optimize_overlaps: bool,
    pub overlap_detection_interval: Duration,
    pub shared_cache_threshold: f64, // 0.0-1.0, minimum overlap to enable sharing
    
    /// Performance tuning
    pub max_concurrent_watches: usize,
    pub transaction_timeout: Duration,
    pub background_optimization_interval: Duration,
    
    /// Memory management
    pub per_watch_cache_limit: Option<usize>,
    pub global_cache_limit: Option<usize>,
    pub eviction_policy: EvictionPolicy,
}

#[derive(Debug, Clone)]
pub enum DatabaseStrategy {
    /// Single database with watch-scoped keys (recommended)
    SingleDatabaseScoped {
        enable_shared_cache: bool,
        enable_cross_watch_queries: bool,
    },
    /// Multiple database files (for isolation requirements)
    MultipleDatabases {
        coordination_database: bool,
        cross_database_queries: bool,
    },
    /// Hybrid approach with database pools
    DatabasePool {
        max_databases: usize,
        watches_per_database: usize,
    },
}
```

## Recommendation

**Use Single Database with Watch-Scoped Keys** for the following reasons:

1. **Superior Performance**: Eliminates duplicate data for overlapping watches
2. **ACID Transactions**: Atomic operations across multiple watches
3. **Memory Efficiency**: Shared caching reduces memory footprint by 30-70%
4. **Operational Simplicity**: Single database file, single backup, single recovery
5. **Advanced Features**: Cross-watch queries, overlap optimization, unified views
6. **ReDB Optimization**: Leverages ReDB's strength in range queries and concurrent reads

This approach provides the best balance of performance, consistency, and operational simplicity while fully leveraging ReDB's advanced features.

## Core Data Model Philosophy

- The event log is strictly append-only. Every filesystem event (move, rename, create, delete, etc.) is recorded as a new entry, preserving a complete, auditable history. No event is ever overwritten or deleted except by explicit retention/cleanup policies.
- The filesystem cache is dynamic and always reflects the current state of the filesystem. It is updated in-place as events occur: files and directories are added, removed, or modified to match the live system. Old cache entries are overwritten or deleted as needed.
- This separation ensures:
    - Fast, accurate lookups and subtree queries for the present state (via the cache).
    - Reliable, append-only history for analysis, recovery, and advanced features (via the event log).
- Both layers are required for robust, scalable monitoring and move/rename detection at scale.
