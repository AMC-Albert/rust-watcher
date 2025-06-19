# Testing Directory

This directory contains all test-related files and documentation for the Rust Filesystem Watcher project.

## Structure

```
testing/
├── README.md              # This file
├── unit/
│   └── mod.rs            # Comprehensive unit and integration tests
├── benchmarks/
│   └── performance.rs    # Criterion-based performance benchmarks
└── docs/
    └── TESTING_GUIDE.md  # Complete testing guide for VS Code + Rust
```

## Running Tests

### Unit and Integration Tests
```bash
# Run all tests
cargo test

# Run specific test
cargo test test_file_move_detection

# Run tests with output
cargo test -- --nocapture

# Run tests single-threaded (for timing-sensitive tests)
cargo test -- --test-threads=1
```

### Benchmarks
```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench move_detector
```

## VS Code Integration

The project includes comprehensive VS Code configuration for:
- **Debug configurations** (`.vscode/launch.json`)
- **Build and test tasks** (`.vscode/tasks.json`)
- **Rust-analyzer settings** (`.vscode/settings.json`)

Use `Ctrl+Shift+P` → "Tasks: Run Task" to access predefined tasks for testing and benchmarking.

## Test Coverage

The test suite includes:
- **35+ comprehensive tests** covering all functionality
- **Unit tests** for individual components
- **Integration tests** for end-to-end scenarios
- **Edge case testing** for error conditions
- **Performance tests** for timing-sensitive operations
- **Property-based tests** using quickcheck
- **Async test scenarios** with tokio-test

## Documentation

See `docs/TESTING_GUIDE.md` for detailed information about:
- Setting up the development environment
- Running tests in VS Code
- Debugging test failures
- Best practices for Rust testing
- Performance benchmarking guidelines
