# Rust Watcher - Remaining Improvements

This document outlines the remaining tasks to further enhance the rust-watcher project. The core functionality is complete and production-ready, but these improvements would add additional robustness and features.

## 🔧 **Code Quality Improvements** (Medium Priority)

### Task 3.2: Improve Remove Event Heuristics ⭐ **COMPLETED**
**Priority**: Medium
**Estimated Time**: 2-3 hours
**Description**: Replace fragile file extension heuristics with smarter detection.

**Current State**:
- [x] Enhanced with intelligent path type inference
- [x] Uses cached metadata from recent events
- [x] Analyzes parent-child relationships in pending events

**Target State**:
- [x] Check against recent create events for context
- [x] Better heuristics for directory detection
- [x] Fallback strategies when uncertain

**Files Modified**:
- [x] `src/watcher.rs` - Updated `convert_notify_event()` method
- [x] `src/move_detector.rs` - Added path type inference methods

**Implementation Details**:
- [x] Check if path exists in cached metadata
- [x] Use parent directory structure analysis via pending events
- [x] Maintain recently seen paths cache
- [x] Improved logging for uncertain cases
- [x] Added `PathTypeHeuristics` struct for detailed analysis
- [x] Added `infer_path_type()` and `get_path_type_heuristics()` methods

**Acceptance Criteria**:
- [x] Reduced misclassification of files/directories
- [x] Better handling of extensionless files
- [x] Unit tests for various path scenarios

---

## 📊 **Enhanced Error Handling and Robustness** (Medium Priority)

### Task 4.1: Implement Resource Cleanup and Limits [x]
**Priority**: High  
**Status**: ✅ **COMPLETED**
**Estimated Time**: 2-3 hours
**Description**: Add comprehensive resource management to prevent memory leaks.

**Implementation Details**:
- [x] Configurable `max_pending_events` per type
- [x] LRU eviction when limits exceeded  
- [x] Memory usage monitoring and reporting
- [x] Graceful degradation strategies
- [x] **MAJOR REFACTOR**: Reorganized move detection into modular architecture
  - [x] `src/move_detection/config.rs` - Configuration management
  - [x] `src/move_detection/events.rs` - Event storage and management
  - [x] `src/move_detection/metadata.rs` - File metadata caching
  - [x] `src/move_detection/heuristics.rs` - Path inference and similarity
  - [x] `src/move_detection/matching.rs` - Move detection algorithms
  - [x] `src/move_detection/monitoring.rs` - Resource monitoring
  - [x] `src/move_detection/detector.rs` - Main orchestration
- [x] Resource monitoring structures (`ResourceStats`, `PendingEventsSummary`)

**Acceptance Criteria**:
- [x] Memory usage bounded under stress
- [x] No memory leaks in long-running tests  
- [x] Graceful handling of resource exhaustion
- [x] **BONUS**: Improved code organization and maintainability (1099 → ~280 lines per module)

---

### Task 4.2: Enhance Error Handling and Recovery ⭐⭐
**Priority**: High
**Estimated Time**: 3-4 hours
**Description**: Implement robust error handling and recovery mechanisms.

**Current State**:
- Basic error propagation
- No retry mechanisms
- Limited error context

**Target State**:
- Comprehensive error categorization
- Retry logic for transient failures
- Detailed error context and reporting

**Files to Modify**:
- [ ] `src/error.rs` - Expand error types
- [ ] `src/watcher.rs` - Add retry logic
- [ ] `src/move_detector.rs` - Better error handling

**New Error Categories**:
- [ ] `WatcherError::PermissionDenied`
- [ ] `WatcherError::ResourceExhausted`
- [ ] `WatcherError::FilesystemError`
- [ ] `WatcherError::ConfigurationError`

**Acceptance Criteria**:
- [ ] Clear error categorization
- [ ] Retry logic for recoverable errors
- [ ] Comprehensive error tests

---

## 🧪 **Testing and Documentation** (Medium Priority)

### Task 5.1: Expand Test Coverage ⭐⭐
**Priority**: High
**Estimated Time**: 4-6 hours
**Description**: Add comprehensive tests for new functionality and edge cases.

**Current State**:
- Basic tests exist
- Limited edge case coverage
- No configuration testing

**Target State**:
- 90%+ test coverage
- Comprehensive edge case testing
- Configuration validation tests

**New Test Files to Create**:
- [ ] `testing/unit/move_detector_tests.rs` - Dedicated move detection tests
- [ ] `testing/unit/config_tests.rs` - Configuration validation tests
- [ ] `testing/unit/error_handling_tests.rs` - Error scenario tests
- [ ] `testing/unit/platform_tests.rs` - Platform-specific tests

**Test Scenarios to Add**:
- [ ] Inode matching accuracy
- [ ] Content hash reliability
- [ ] Configuration edge cases
- [ ] Resource limit enforcement
- [ ] Error recovery scenarios
- [ ] Performance under load

**Acceptance Criteria**:
- [ ] Test coverage above 90%
- [ ] All new features have tests
- [ ] Edge cases thoroughly covered

---

### Task 5.2: Update Documentation ⭐⭐
**Priority**: High
**Estimated Time**: 3-4 hours
**Description**: Update all documentation to reflect new API and features.

**Files to Update**:
- [ ] `README.md` - New API examples and features
- [ ] `src/lib.rs` - Module documentation
- [ ] `src/watcher.rs` - API documentation
- [ ] `src/move_detector.rs` - Algorithm documentation
- [ ] `examples/advanced_monitor.rs` - Updated example

**Documentation Sections to Add**:
- [ ] Configuration guide
- [ ] Performance characteristics
- [ ] Platform differences
- [ ] Migration guide from old API
- [ ] Troubleshooting section

**Acceptance Criteria**:
- [ ] All public APIs documented
- [ ] Examples work as shown
- [ ] Migration guide complete

---

## 🎯 **Advanced Features** (Low Priority / Future)

### Task 6.1: Add Metrics and Monitoring ⭐
**Priority**: Low
**Estimated Time**: 3-4 hours
**Description**: Add optional metrics collection for monitoring and debugging.

**Features to Add**:
- [ ] Event processing rates
- [ ] Move detection accuracy metrics
- [ ] Memory usage tracking
- [ ] Performance histograms

### Task 6.2: Configuration File Support ⭐
**Priority**: Low
**Estimated Time**: 2-3 hours
**Description**: Support configuration files in addition to programmatic configuration.

**Features to Add**:
- [ ] TOML configuration file support
- [ ] JSON configuration support
- [ ] Environment variable overrides

### Task 6.3: Advanced Move Detection Algorithms ⭐
**Priority**: Low
**Estimated Time**: 6-8 hours
**Description**: Implement additional sophisticated move detection methods.

**Algorithms to Research**:
- [ ] Machine learning-based pattern recognition
- [ ] Directory structure analysis
- [ ] User behavior pattern analysis

---

## 📋 **Task Execution Guidelines**

### Before Starting Each Task:
1. [ ] Create a feature branch: `git checkout -b task-X.Y-description`
2. [ ] Read the current implementation thoroughly
3. [ ] Write failing tests first (TDD approach)
4. [ ] Document expected behavior

### During Implementation:
1. [ ] Follow existing code style and patterns
2. [ ] Add comprehensive logging for debugging
3. [ ] Handle all error cases explicitly
4. [ ] Write documentation as you code

### After Completing Each Task:
1. [ ] Run full test suite: `cargo test`
2. [ ] Run benchmarks: `cargo bench`
3. [ ] Test on multiple platforms if applicable
4. [ ] Update documentation
5. [ ] Create PR with detailed description

### Quality Gates:
- [ ] All tests pass
- [ ] No clippy warnings
- [ ] Code formatted with rustfmt
- [ ] Documentation updated
- [ ] Performance regression check

---

## 🎉 **Success Metrics**

### Technical Metrics:
- [ ] Test coverage > 90%
- [ ] Zero memory leaks in long-running tests
- [ ] Performance improvement in move detection
- [ ] Successful cross-platform operation

### API Quality Metrics:
- [ ] Non-blocking, composable API
- [ ] Comprehensive configuration options
- [ ] Clear error handling and reporting
- [ ] Excellent documentation with examples

### Robustness Metrics:
- [ ] Graceful handling of resource exhaustion
- [ ] Accurate move detection across different scenarios
- [ ] Stable operation under stress testing
- [ ] Professional-grade error handling

---

## 🐛 **Critical Bug Fixes** (Completed)

### Task: Fix Windows Cut/Paste Move Detection ⭐ **COMPLETED**
**Priority**: Critical
**Estimated Time**: 4-6 hours  
**Description**: Fixed critical bug where Windows File Explorer cut/paste operations were not being detected as moves.

**Problem Identified**:
- Windows `std::fs::rename()` generates correct Remove → Create events
- Move detector was incorrectly matching Remove events with **previous Create events for the same path**
- This resulted in moves being detected with identical source/destination paths
- Real moves were not detected because matching logic allowed same-path matches

**Root Cause**:
- Matching functions in `src/move_detection/matching.rs` lacked path comparison
- Remove events were matching Create events from initial file creation (same path) 
- No validation that source ≠ destination for valid moves

**Solution Implemented**:
- [x] Added path comparison filters in `find_best_match_in_candidates()` functions
- [x] Added same-path checks in inode and Windows ID matching logic  
- [x] Ensured moves are only detected when source ≠ destination paths
- [x] Created comprehensive real-world integration tests

**Files Modified**:
- [x] `src/move_detection/matching.rs` - Added path comparison filters
- [x] `testing/unit/real_world_move_tests.rs` - New comprehensive test suite

**Verification**:
- [x] `debug_full_pipeline.rs` - Real file operations now correctly detect moves
- [x] `debug_cut_paste_real.rs` - Simulated cut/paste works correctly
- [x] `test_real_world_cut_paste_detection` - Integration test with actual file ops
- [x] `test_same_path_not_detected_as_move` - Prevents regression of this bug
- [x] `test_rapid_move_operations` - Handles quick successive operations

**Test Results**:
- [x] 58/59 tests passing (1 pre-existing test failure unrelated to this fix)
- [x] Real Windows cut/paste operations now detected with 95%+ confidence
- [x] Move detection method: WindowsId for real operations, SizeAndTime for simulated

**Impact**:
- ✅ **Windows File Explorer cut/paste now works correctly**
- ✅ **High confidence detection (0.90-0.95)**  
- ✅ **Proper source → destination path tracking**
- ✅ **No false positives for same-path operations**

**FINAL VERIFICATION**:
- ✅ **All 60/60 tests passing** (including previously failing `test_bug_report_scenario`)
- ✅ **Real Windows File Explorer cut/paste operations detected** 
- ✅ **Programmatic moves continue to work correctly**
- ✅ **Comprehensive test coverage added to prevent regressions**
- ✅ **Clean codebase with debug files removed**

This critical bug fix ensures that rust-watcher works correctly with real-world user operations in Windows File Explorer, making it production-ready for Windows environments.
