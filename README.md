# Rust Filesystem Watcher

A comprehensive filesystem watcher in Rust that monitors file and directory operations, with advanced capabilities for detecting and tracking rename/move operations with high confidence.

## Features

- **Cross-platform** filesystem monitoring using the `notify` crate
- **Advanced move detection** that can identify relationships between source and destination paths
- **Multiple detection methods** including:
  - Filesystem event correlation
  - Inode matching (Unix-like systems)
  - Content hash comparison
  - Metadata matching
  - Name similarity and timing analysis
- **Configurable timeout** for move detection
- **Recursive directory watching**
- **Structured event logging** with JSON output
- **Comprehensive test suite**

## Usage

### Command Line Interface

```bash
# Watch the current directory
cargo run -- --path .

# Watch a specific directory with verbose logging
cargo run -- --path /path/to/watch --verbose

# Non-recursive watching with custom timeout
cargo run -- --path /path/to/watch --recursive false --timeout 2000
```

### Programmatic Usage

```rust
use rust_watcher::{FileSystemWatcher, WatcherConfig};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = WatcherConfig {
        path: PathBuf::from("/path/to/watch"),
        recursive: true,
        move_timeout_ms: 1000,
    };
    
    let mut watcher = FileSystemWatcher::new(config).await?;
    watcher.start_watching().await?;
    
    Ok(())
}
```

## Event Types

The watcher detects and reports the following event types:

- **Create**: New files or directories
- **Write**: File content modifications
- **Remove**: File or directory deletions
- **Move**: File or directory moves/renames (with source-destination mapping)
- **Chmod**: Permission changes
- **Other**: Platform-specific events

## Move Detection

The system uses sophisticated algorithms to detect move operations:

### Detection Methods

1. **Filesystem Event Correlation**: Matches remove and create events within the timeout window
2. **Inode Matching**: Uses filesystem inodes to track the same file (Unix-like systems)
3. **Content Hash**: Compares file content hashes for small files
4. **Metadata Matching**: Compares file size, timestamps, and other metadata
5. **Name and Timing**: Analyzes filename similarity and event timing

### Confidence Scoring

Each detected move includes a confidence score (0.0 to 1.0) based on:
- File size matching
- Inode consistency
- Content hash matching
- Name similarity
- Timing proximity
- Metadata correlation

## Configuration

```rust
pub struct WatcherConfig {
    pub path: PathBuf,        // Path to watch
    pub recursive: bool,      // Whether to watch subdirectories
    pub move_timeout_ms: u64, // Timeout for move detection
}
```

## Examples

### Detecting File Moves

When a file is moved from `/old/path/file.txt` to `/new/path/file.txt`, the watcher will output:

```
🔄 MOVE DETECTED: /old/path/file.txt -> /new/path/file.txt (confidence: 0.95, method: InodeMatching)
```

### JSON Event Output

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "event_type": "Move",
  "path": "/new/path/file.txt",
  "timestamp": "2025-06-19T10:30:00Z",
  "is_directory": false,
  "size": 1024,
  "move_data": {
    "source_path": "/old/path/file.txt",
    "destination_path": "/new/path/file.txt",
    "confidence": 0.95,
    "detection_method": "InodeMatching"
  }
}
```

## Testing

Run the comprehensive test suite:

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_file_move_detection
```

### Test Coverage

The test suite covers:
- Basic file and directory operations
- Move detection accuracy
- Multiple rapid operations
- Complex directory structures
- Edge cases and error conditions
- Configuration validation
- Event serialization

## Platform Support

- **Linux**: Full support including inode tracking
- **macOS**: Full support including inode tracking  
- **Windows**: Supported with filesystem event correlation (no inode support)

## Performance Considerations

- **Memory Usage**: Configurable limits on pending events to prevent memory leaks
- **CPU Usage**: Efficient event processing with async/await
- **File Size**: Content hashing limited to files under 1MB by default
- **Timeout Management**: Automatic cleanup of expired pending events

## Dependencies

- `notify`: Cross-platform filesystem notification
- `tokio`: Async runtime
- `serde`: Serialization support
- `uuid`: Event identification
- `chrono`: Timestamp handling
- `tracing`: Structured logging

## License

This project is licensed under the MIT License.
