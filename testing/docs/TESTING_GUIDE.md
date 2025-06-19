# Comprehensive Rust Testing Guide for VS Code

## Rust's Built-in Testing Ecosystem

Rust has excellent built-in testing capabilities that are far superior to custom scripts:

### 1. **Unit Tests** (`#[test]`)
- Live right next to your code
- Run with `cargo test`
- Automatically discovered

### 2. **Integration Tests** (`tests/` directory)
- Test public API from external perspective
- Each file is a separate crate

### 3. **Benchmarks** (`benches/` directory)
- Performance testing with Criterion
- Statistical analysis and regression detection

### 4. **Documentation Tests** (`///` comments)
- Examples in docs are automatically tested
- Ensures documentation stays accurate

## VS Code Integration

### Built-in Commands (Ctrl+Shift+P)

#### Testing Commands
- **"Rust Analyzer: Run Test"** - Run test under cursor
- **"Tasks: Run Task"** → Choose test task
- **"Test: Run All Tests"** - If Test Explorer is enabled

#### Available Tasks

##### Build & Check
- `cargo check` - Fast syntax/type checking
- `cargo build` - Full compilation
- `cargo clippy` - Linting

##### Testing  
- `cargo test` - All tests (default)
- `cargo test -- unit tests only` - Library tests only
- `cargo test -- integration tests` - Integration tests only
- `cargo test -- specific test` - Run specific test pattern
- `cargo test -- with output` - Show println! output
- `cargo test -- single threaded` - For timing-sensitive tests

##### Benchmarking
- `cargo bench` - Run all benchmarks
- `cargo bench -- performance` - Run specific benchmark
- `cargo bench -- baseline` - Save performance baseline

##### Documentation
- `cargo doc` - Generate documentation
- `cargo test --doc` - Test documentation examples

## Testing in VS Code

### 1. **Quick Test Running**

#### Code Lens Integration
Rust-analyzer shows **▶ Run** and **🐛 Debug** buttons above test functions:

```rust
#[test]  // ← "▶ Run | 🐛 Debug" appears here
fn test_move_detection() {
    // test code
}

#[tokio::test]  // ← Works with async tests too
async fn test_async_functionality() {
    // async test code
}
```

#### Right-click Context Menu
- Right-click on any test function
- Select "Run Test" or "Debug Test"

### 2. **Running Test Suites**

#### All Tests
```bash
cargo test
# Or use VS Code task: Ctrl+Shift+P → "Tasks: Run Task" → "cargo test"
```

#### Unit Tests Only
```bash
cargo test --lib
# Tests in src/ files with #[test]
```

#### Integration Tests Only
```bash
cargo test --test '*'
# Tests in tests/ directory
```

#### Specific Test Pattern
```bash
cargo test move_detector
# Runs all tests matching "move_detector"
```

### 3. **Debugging Tests**

#### Debug Single Test
1. Set breakpoints in test or source code
2. Click **🐛 Debug** button above test
3. Or use Debug panel (F5) → "Debug specific test"

#### Debug with Arguments
```bash
# In terminal for custom debugging
RUST_LOG=debug cargo test test_name -- --nocapture
```

### 4. **Test Output and Logging**

#### Show Test Output
```bash
cargo test -- --nocapture
# Shows println!, dbg!, etc. output
```

#### Environment Variables
```bash
# Enable debug logging
RUST_LOG=debug cargo test

# Show backtraces on panic
RUST_BACKTRACE=1 cargo test

# Full backtrace
RUST_BACKTRACE=full cargo test
```

## Advanced Testing Features

### 1. **Test Organization**

#### Modules for Organization
```rust
#[cfg(test)]
mod tests {
    use super::*;

    mod move_detection_tests {
        use super::*;
        
        #[test]
        fn test_basic_move() { /* */ }
        
        #[test]
        fn test_complex_move() { /* */ }
    }

    mod error_handling_tests {
        use super::*;
        
        #[test]
        fn test_invalid_path() { /* */ }
    }
}
```

#### Test Attributes
```rust
#[test]
fn normal_test() { }

#[test]
#[ignore]  // Skip by default, run with --ignored
fn expensive_test() { }

#[test]
#[should_panic]  // Test should panic
fn test_panic_condition() { }

#[test]
#[should_panic(expected = "invalid path")]  // Specific panic message
fn test_specific_panic() { }
```

### 2. **Async Testing**
```rust
#[tokio::test]
async fn test_async_function() {
    let result = some_async_function().await;
    assert_eq!(result, expected_value);
}

#[tokio::test]
async fn test_with_timeout() {
    tokio::time::timeout(
        Duration::from_secs(5),
        long_running_operation()
    ).await.unwrap();
}
```

### 3. **Property-Based Testing**
```toml
[dev-dependencies]
proptest = "1.0"
```

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_move_preserves_content(
        content in ".*",
        filename in "[a-zA-Z0-9_-]{1,50}\\.txt"
    ) {
        // Test that file moves preserve content
        // regardless of content or filename
    }
}
```

### 4. **Benchmarking with Criterion**

#### Running Benchmarks
```bash
# All benchmarks
cargo bench

# Specific benchmark
cargo bench move_detector

# Save baseline for comparison
cargo bench -- --save-baseline main

# Compare against baseline
cargo bench -- --baseline main
```

#### Benchmark Results
- Results saved in `target/criterion/`
- HTML reports generated automatically
- Statistical analysis with confidence intervals
- Regression detection

## VS Code Testing Extensions

### Recommended Extensions

#### Essential
- **rust-analyzer** - Core Rust support with test integration
- **CodeLLDB** - Debugging support

#### Optional but Useful
- **Test Explorer UI** - Unified test interface
- **Coverage Gutters** - Code coverage visualization (with tarpaulin)
- **Error Lens** - Inline error display

### Configuration

#### .vscode/settings.json
```json
{
    "rust-analyzer.lens.enable": true,
    "rust-analyzer.lens.run": true,
    "rust-analyzer.lens.debug": true,
    "rust-analyzer.checkOnSave.command": "clippy",
    "rust-analyzer.cargo.buildScripts.enable": true
}
```

## Testing Workflow Examples

### 1. **TDD Workflow**
1. Write failing test
2. Click **▶ Run** - should fail
3. Write minimal code to pass
4. Click **▶ Run** - should pass
5. Refactor and re-run tests

### 2. **Debug Workflow**
1. Test fails unexpectedly
2. Set breakpoint in test or source
3. Click **🐛 Debug** on test
4. Step through code to find issue
5. Fix and re-run

### 3. **Performance Workflow**
1. Write benchmark
2. Run: `cargo bench -- --save-baseline before`
3. Make performance changes
4. Run: `cargo bench -- --baseline before`
5. Check regression/improvement

### 4. **Integration Testing**
1. Create `tests/integration_test.rs`
2. Test public API only
3. Use `cargo test --test integration_test`
4. Verify external behavior

## Troubleshooting

### Common Issues

#### Tests Not Running
- Check if `#[test]` attribute is present
- Verify test is in `#[cfg(test)]` module
- Make sure file is saved (auto-save enabled)

#### Debug Not Working
- Ensure CodeLLDB extension is installed
- Check launch.json configuration
- Verify debug symbols: use dev profile

#### Slow Tests
- Use `#[ignore]` for expensive tests
- Run with `--release` for faster execution
- Consider parallel test execution limits

#### Test Isolation
- Use `tempfile` crate for filesystem tests
- Each test gets isolated temporary directory
- Automatic cleanup prevents interference

### Performance Tips

#### Fast Feedback Loop
```bash
# Quick syntax check
cargo check

# Fast test subset
cargo test unit_tests

# Only changed tests (with watch mode)
cargo install cargo-watch
cargo watch -x test
```

#### Parallel Testing
```bash
# Control parallel execution
cargo test -- --test-threads=1  # Single threaded
cargo test -- --test-threads=4  # 4 threads
```

This comprehensive testing setup leverages Rust's excellent built-in capabilities and VS Code's powerful editor features for an optimal development experience!
