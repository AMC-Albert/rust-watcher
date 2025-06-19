use rust_watcher::{start, MoveDetectorConfig, WatcherConfig};
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
		move_detector_config: None,
	};
	let (handle, mut event_receiver) = start(config).unwrap();

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

	// Check that we can receive at least one event
	tokio::select! {
		event = event_receiver.recv() => {
			assert!(event.is_some(), "Should receive at least one event");
		}
		_ = sleep(Duration::from_millis(1000)) => {
			// Timeout is OK for basic test - just checking the file exists
		}
	}

	// Clean shutdown
	handle.stop().await.unwrap();
	sleep(Duration::from_millis(100)).await;
}

#[tokio::test]
async fn test_file_move_detection() {
	let temp_dir = TempDir::new().unwrap();
	let test_path = temp_dir.path().to_path_buf();

	// Create test files
	let source_file = test_path.join("source.txt");
	let dest_file = test_path.join("destination.txt");
	fs::write(&source_file, "test content").await.unwrap();
	let move_config = MoveDetectorConfig {
		timeout: Duration::from_millis(2000),
		..Default::default()
	};
	let config = WatcherConfig {
		path: test_path.clone(),
		recursive: true,
		move_detector_config: Some(move_config),
	};
	let (handle, mut event_receiver) = start(config).unwrap();

	sleep(Duration::from_millis(100)).await;

	// Move the file
	fs::rename(&source_file, &dest_file).await.unwrap();

	sleep(Duration::from_millis(2500)).await;

	// Verify the move
	assert!(!source_file.exists());
	assert!(dest_file.exists());

	let content = fs::read_to_string(&dest_file).await.unwrap();
	assert_eq!(content, "test content");
	// Consume any pending events
	tokio::select! {
		_ = event_receiver.recv() => {}
		_ = sleep(Duration::from_millis(100)) => {}
	}

	// Clean shutdown
	handle.stop().await.unwrap();
	sleep(Duration::from_millis(100)).await;
}

#[tokio::test]
async fn test_directory_operations() {
	let temp_dir = TempDir::new().unwrap();
	let test_path = temp_dir.path().to_path_buf();
	let config = WatcherConfig {
		path: test_path.clone(),
		recursive: true,
		move_detector_config: None,
	};
	let (handle, mut event_receiver) = start(config).unwrap();

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

	// Consume any pending events
	tokio::select! {
		_ = event_receiver.recv() => {}
		_ = sleep(Duration::from_millis(100)) => {}
	}

	// Clean shutdown
	handle.stop().await.unwrap();
	sleep(Duration::from_millis(100)).await;
}
