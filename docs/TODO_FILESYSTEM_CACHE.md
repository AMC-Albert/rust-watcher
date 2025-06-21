# TODO: Filesystem Cache Implementation

## Status Update (June 2025)
- Core event log and cache storage: complete and stable. All legacy migration, schema redesign, and scaffolding tasks are finished and no longer tracked here.
- Filesystem cache is modular, robust, and warning-free. All integration and stress tests pass except for explicitly unimplemented multi-watch scenarios (not a regression).
- Documentation and comments are up to date for all implemented features. Pending features are only in this TODO and design docs.
- Unified/cross-watch queries, hierarchical operations, and pattern-based search are implemented and tested.
- All build, test, and clippy checks are clean as of latest commit.
- See commit history for details of modularization, bugfixes, and groundwork for advanced features (June 2025).

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
| Pattern-based search/filtering  | Complete | File name glob patterns supported, robust and tested (June 2025)                                                                              |

---

## Remaining Work

### Phase 3: Filesystem Cache API (incomplete)
- [ ] Scalable stats/indexing: per-watch, per-path, and advanced indexing
- [ ] Cache synchronization: integrate with file watcher events, incremental updates, invalidation, and consistency verification
- [ ] Performance operations: bulk cache warming, background maintenance, statistics/monitoring, memory optimization
- [ ] Integration with existing systems: watcher integration, cache-aware monitoring, database adapter enhancements
- [ ] Advanced features: transaction coordination, memory management, performance monitoring
- [ ] Testing and validation: more unit/integration tests for new features, especially cache synchronization and multi-watch scenarios
