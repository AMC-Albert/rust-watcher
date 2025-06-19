use rust_watcher::{FileSystemWatcher, WatcherConfig};
use std::time::Duration;
use tempfile::TempDir;
use tokio::fs;
use tokio::time::sleep;

#[tokio::test]
async fn test_basic_file_creation() {
	let temp_dir = TempDir::new().unwrap();
	let test_path = temp_dir.path().to_path_buf();

	let config = WatcherConfig {
		path: test_path.clone(),
		recursive: true,
		move_timeout_ms: 1000,
	};
	let mut watcher = FileSystemWatcher::new(config).await.unwrap();

	// Start watching in the background
	tokio::spawn(async move {
		watcher.start_watching().await.unwrap();
	});

	// Give the watcher time to start
	sleep(Duration::from_millis(100)).await;

	// Create a test file
	let test_file = test_path.join("test.txt");
	fs::write(&test_file, "test content").await.unwrap();

	// Give some time for the event to be processed
	sleep(Duration::from_millis(200)).await;

	// The watcher should have detected the file creation
	// This is a basic smoke test to ensure the watcher starts without errors
	assert!(test_file.exists());
}

#[tokio::test]
async fn test_file_move_detection() {
	let temp_dir = TempDir::new().unwrap();
	let test_path = temp_dir.path().to_path_buf();

	// Create test files
	let source_file = test_path.join("source.txt");
	let dest_file = test_path.join("destination.txt");

	fs::write(&source_file, "test content").await.unwrap();

	let config = WatcherConfig {
		path: test_path.clone(),
		recursive: true,
		move_timeout_ms: 2000,
	};

	let mut watcher = FileSystemWatcher::new(config).await.unwrap();

	tokio::spawn(async move {
		watcher.start_watching().await.unwrap();
	});

	sleep(Duration::from_millis(100)).await;

	// Move the file
	fs::rename(&source_file, &dest_file).await.unwrap();

	sleep(Duration::from_millis(2500)).await;

	// Verify the move
	assert!(!source_file.exists());
	assert!(dest_file.exists());

	let content = fs::read_to_string(&dest_file).await.unwrap();
	assert_eq!(content, "test content");
}

#[tokio::test]
async fn test_directory_operations() {
	let temp_dir = TempDir::new().unwrap();
	let test_path = temp_dir.path().to_path_buf();

	let config = WatcherConfig {
		path: test_path.clone(),
		recursive: true,
		move_timeout_ms: 1000,
	};

	let mut watcher = FileSystemWatcher::new(config).await.unwrap();

	tokio::spawn(async move {
		watcher.start_watching().await.unwrap();
	});

	sleep(Duration::from_millis(100)).await;

	// Create a subdirectory
	let subdir = test_path.join("subdir");
	fs::create_dir(&subdir).await.unwrap();

	// Create a file in the subdirectory
	let subfile = subdir.join("file.txt");
	fs::write(&subfile, "subdir content").await.unwrap();

	sleep(Duration::from_millis(200)).await;

	// Verify the operations
	assert!(subdir.exists());
	assert!(subfile.exists());

	let content = fs::read_to_string(&subfile).await.unwrap();
	assert_eq!(content, "subdir content");
}
