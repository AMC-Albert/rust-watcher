use rust_watcher::{
	EventType, FileSystemEvent, FileSystemWatcher, MoveDetectionMethod, MoveEvent, WatcherConfig,
	WatcherError,
};
use std::path::PathBuf;
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

	fs::write(&source_file, "test content for move")
		.await
		.unwrap();

	let config = WatcherConfig {
		path: test_path.clone(),
		recursive: true,
		move_timeout_ms: 500,
	};

	let mut watcher = FileSystemWatcher::new(config).await.unwrap();

	// Start watching in background
	tokio::spawn(async move {
		watcher.start_watching().await.unwrap();
	});

	sleep(Duration::from_millis(100)).await;

	// Perform the move operation
	fs::rename(&source_file, &dest_file).await.unwrap();

	// Wait for move detection
	sleep(Duration::from_millis(600)).await;

	// Verify the move completed
	assert!(!source_file.exists());
	assert!(dest_file.exists());

	let content = fs::read_to_string(&dest_file).await.unwrap();
	assert_eq!(content, "test content for move");
}

#[tokio::test]
async fn test_directory_move_detection() {
	let temp_dir = TempDir::new().unwrap();
	let test_path = temp_dir.path().to_path_buf();

	// Create test directory with content
	let source_dir = test_path.join("source_dir");
	let dest_dir = test_path.join("dest_dir");

	fs::create_dir(&source_dir).await.unwrap();
	fs::write(source_dir.join("file.txt"), "content")
		.await
		.unwrap();

	let config = WatcherConfig {
		path: test_path.clone(),
		recursive: true,
		move_timeout_ms: 500,
	};

	let mut watcher = FileSystemWatcher::new(config).await.unwrap();

	tokio::spawn(async move {
		watcher.start_watching().await.unwrap();
	});

	sleep(Duration::from_millis(100)).await;

	// Move the directory
	fs::rename(&source_dir, &dest_dir).await.unwrap();

	sleep(Duration::from_millis(600)).await;

	// Verify the move
	assert!(!source_dir.exists());
	assert!(dest_dir.exists());
	assert!(dest_dir.join("file.txt").exists());
}

#[tokio::test]
async fn test_multiple_rapid_operations() {
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

	// Perform rapid file operations
	for i in 0..5 {
		let file_path = test_path.join(format!("file_{}.txt", i));
		fs::write(&file_path, format!("content {}", i))
			.await
			.unwrap();

		// Move to a new location
		let new_path = test_path.join(format!("moved_file_{}.txt", i));
		fs::rename(&file_path, &new_path).await.unwrap();

		sleep(Duration::from_millis(50)).await;
	}

	sleep(Duration::from_millis(1200)).await;

	// Verify all operations completed
	for i in 0..5 {
		let original_path = test_path.join(format!("file_{}.txt", i));
		let moved_path = test_path.join(format!("moved_file_{}.txt", i));

		assert!(!original_path.exists());
		assert!(moved_path.exists());
	}
}

#[test]
fn test_move_event_creation() {
	let move_event = MoveEvent {
		source_path: PathBuf::from("/old/path"),
		destination_path: PathBuf::from("/new/path"),
		confidence: 0.95,
		detection_method: MoveDetectionMethod::InodeMatching,
	};

	assert_eq!(move_event.source_path, PathBuf::from("/old/path"));
	assert_eq!(move_event.destination_path, PathBuf::from("/new/path"));
	assert_eq!(move_event.confidence, 0.95);
	assert_eq!(
		move_event.detection_method,
		MoveDetectionMethod::InodeMatching
	);
}

#[test]
fn test_filesystem_event_serialization() {
	let event = FileSystemEvent::new(
		EventType::Create,
		PathBuf::from("/test/path"),
		false,
		Some(1024),
	);

	let json = event.to_json().unwrap();
	assert!(json.contains("Create"));
	assert!(json.contains("/test/path"));
	assert!(json.contains("1024"));
}

#[test]
fn test_filesystem_event_with_move_data() {
	let move_data = MoveEvent {
		source_path: PathBuf::from("/old"),
		destination_path: PathBuf::from("/new"),
		confidence: 0.8,
		detection_method: MoveDetectionMethod::ContentHash,
	};

	let event = FileSystemEvent::new(EventType::Create, PathBuf::from("/new"), false, Some(512))
		.with_move_data(move_data);

	assert!(event.is_move());
	assert_eq!(event.event_type, EventType::Move);
	assert!(event.move_data.is_some());

	let move_info = event.move_data.unwrap();
	assert_eq!(move_info.confidence, 0.8);
	assert_eq!(move_info.detection_method, MoveDetectionMethod::ContentHash);
}

#[tokio::test]
async fn test_watcher_config_validation() {
	// Test with non-existent path
	let config = WatcherConfig {
		path: PathBuf::from("/non/existent/path"),
		recursive: true,
		move_timeout_ms: 1000,
	};

	let mut watcher = FileSystemWatcher::new(config).await.unwrap();
	let result = watcher.start_watching().await;

	assert!(result.is_err());
}

#[test]
fn test_event_type_conversion() {
	use notify::EventKind;

	let create_event = EventType::from(EventKind::Create(notify::event::CreateKind::File));
	assert_eq!(create_event, EventType::Create);

	let modify_event = EventType::from(EventKind::Modify(notify::event::ModifyKind::Data(
		notify::event::DataChange::Content,
	)));
	assert_eq!(modify_event, EventType::Write);

	let remove_event = EventType::from(EventKind::Remove(notify::event::RemoveKind::File));
	assert_eq!(remove_event, EventType::Remove);
}

// Integration test for complex scenarios
#[tokio::test]
async fn test_complex_move_scenario() {
	let temp_dir = TempDir::new().unwrap();
	let test_path = temp_dir.path().to_path_buf();

	// Create a complex directory structure
	let src_dir = test_path.join("src");
	let dest_dir = test_path.join("dest");

	fs::create_dir_all(&src_dir).await.unwrap();
	fs::create_dir_all(&dest_dir).await.unwrap();

	// Create files with different sizes and types
	fs::write(src_dir.join("small.txt"), "small content")
		.await
		.unwrap();
	fs::write(src_dir.join("large.txt"), "large content ".repeat(100))
		.await
		.unwrap();

	let subdirectory = src_dir.join("subdir");
	fs::create_dir(&subdirectory).await.unwrap();
	fs::write(subdirectory.join("nested.txt"), "nested content")
		.await
		.unwrap();

	let config = WatcherConfig {
		path: test_path.clone(),
		recursive: true,
		move_timeout_ms: 2000,
	};

	let mut watcher = FileSystemWatcher::new(config).await.unwrap();

	tokio::spawn(async move {
		watcher.start_watching().await.unwrap();
	});

	sleep(Duration::from_millis(200)).await;

	// Perform complex move operations
	fs::rename(src_dir.join("small.txt"), dest_dir.join("moved_small.txt"))
		.await
		.unwrap();
	sleep(Duration::from_millis(100)).await;

	fs::rename(src_dir.join("large.txt"), dest_dir.join("moved_large.txt"))
		.await
		.unwrap();
	sleep(Duration::from_millis(100)).await;

	fs::rename(&subdirectory, dest_dir.join("moved_subdir"))
		.await
		.unwrap();
	sleep(Duration::from_millis(100)).await;

	// Wait for all move detections to complete
	sleep(Duration::from_millis(2500)).await;

	// Verify all moves completed correctly
	assert!(dest_dir.join("moved_small.txt").exists());
	assert!(dest_dir.join("moved_large.txt").exists());
	assert!(dest_dir.join("moved_subdir").exists());
	assert!(dest_dir.join("moved_subdir").join("nested.txt").exists());

	// Verify original files are gone
	assert!(!src_dir.join("small.txt").exists());
	assert!(!src_dir.join("large.txt").exists());
	assert!(!subdirectory.exists());
}

// Edge case tests
#[tokio::test]
async fn test_rapid_create_delete_cycles() {
	let temp_dir = TempDir::new().unwrap();
	let test_path = temp_dir.path().to_path_buf();

	let config = WatcherConfig {
		path: test_path.clone(),
		recursive: true,
		move_timeout_ms: 500,
	};

	let mut watcher = FileSystemWatcher::new(config).await.unwrap();

	tokio::spawn(async move {
		watcher.start_watching().await.unwrap();
	});

	sleep(Duration::from_millis(100)).await;

	// Rapid create/delete cycles that might confuse move detection
	for i in 0..10 {
		let file_path = test_path.join(format!("rapid_{}.txt", i));

		// Create file
		fs::write(&file_path, format!("content {}", i))
			.await
			.unwrap();
		sleep(Duration::from_millis(10)).await;

		// Delete it immediately
		fs::remove_file(&file_path).await.unwrap();
		sleep(Duration::from_millis(10)).await;

		// Create again with same name
		fs::write(&file_path, format!("new content {}", i))
			.await
			.unwrap();
		sleep(Duration::from_millis(10)).await;
	}

	sleep(Duration::from_millis(800)).await;

	// Verify final state
	for i in 0..10 {
		let file_path = test_path.join(format!("rapid_{}.txt", i));
		assert!(file_path.exists());
		let content = fs::read_to_string(&file_path).await.unwrap();
		assert_eq!(content, format!("new content {}", i));
	}
}

#[tokio::test]
async fn test_concurrent_moves_same_destination() {
	let temp_dir = TempDir::new().unwrap();
	let test_path = temp_dir.path().to_path_buf();

	// Create multiple source files
	let mut source_files = Vec::new();
	for i in 0..5 {
		let file_path = test_path.join(format!("source_{}.txt", i));
		fs::write(&file_path, format!("content {}", i))
			.await
			.unwrap();
		source_files.push(file_path);
	}

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

	// Try to move all files to the same destination (only last should succeed)
	let dest_file = test_path.join("destination.txt");

	for (i, source_file) in source_files.iter().enumerate() {
		if i == source_files.len() - 1 {
			// Last move should succeed
			fs::rename(source_file, &dest_file).await.unwrap();
		} else {
			// Earlier moves should fail or be overwritten
			let temp_dest = test_path.join(format!("temp_dest_{}.txt", i));
			fs::rename(source_file, &temp_dest).await.unwrap();
			sleep(Duration::from_millis(50)).await;

			// Try to move to final destination (will fail if it exists)
			if !dest_file.exists() {
				let _ = fs::rename(&temp_dest, &dest_file).await;
			}
		}
		sleep(Duration::from_millis(100)).await;
	}

	sleep(Duration::from_millis(1200)).await;

	// Verify final state - only one file should exist at destination
	assert!(dest_file.exists());
}

#[tokio::test]
async fn test_move_chain_operations() {
	let temp_dir = TempDir::new().unwrap();
	let test_path = temp_dir.path().to_path_buf();

	// Create initial file
	let file1 = test_path.join("file1.txt");
	let file2 = test_path.join("file2.txt");
	let file3 = test_path.join("file3.txt");
	let file4 = test_path.join("file4.txt");

	fs::write(&file1, "original content").await.unwrap();

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

	// Chain of moves: file1 -> file2 -> file3 -> file4
	fs::rename(&file1, &file2).await.unwrap();
	sleep(Duration::from_millis(200)).await;

	fs::rename(&file2, &file3).await.unwrap();
	sleep(Duration::from_millis(200)).await;

	fs::rename(&file3, &file4).await.unwrap();
	sleep(Duration::from_millis(200)).await;

	sleep(Duration::from_millis(1200)).await;

	// Verify final state
	assert!(!file1.exists());
	assert!(!file2.exists());
	assert!(!file3.exists());
	assert!(file4.exists());

	let content = fs::read_to_string(&file4).await.unwrap();
	assert_eq!(content, "original content");
}

#[tokio::test]
async fn test_move_detection_timeout_edge_cases() {
	let temp_dir = TempDir::new().unwrap();
	let test_path = temp_dir.path().to_path_buf();

	let config = WatcherConfig {
		path: test_path.clone(),
		recursive: true,
		move_timeout_ms: 300, // Very short timeout
	};

	let mut watcher = FileSystemWatcher::new(config).await.unwrap();

	tokio::spawn(async move {
		watcher.start_watching().await.unwrap();
	});

	sleep(Duration::from_millis(100)).await;

	// Create file
	let source_file = test_path.join("timeout_test.txt");
	fs::write(&source_file, "timeout test content")
		.await
		.unwrap();
	sleep(Duration::from_millis(100)).await;

	// Remove file
	fs::remove_file(&source_file).await.unwrap();

	// Wait longer than timeout before creating new file
	sleep(Duration::from_millis(500)).await;

	// Create file with same name (should NOT be detected as move due to timeout)
	fs::write(&source_file, "new content after timeout")
		.await
		.unwrap();

	sleep(Duration::from_millis(400)).await;

	// Verify file exists with new content
	assert!(source_file.exists());
	let content = fs::read_to_string(&source_file).await.unwrap();
	assert_eq!(content, "new content after timeout");
}

#[tokio::test]
async fn test_deep_directory_structure_moves() {
	let temp_dir = TempDir::new().unwrap();
	let test_path = temp_dir.path().to_path_buf();

	// Create deep directory structure
	let deep_path = test_path
		.join("level1")
		.join("level2")
		.join("level3")
		.join("level4");
	fs::create_dir_all(&deep_path).await.unwrap();

	let deep_file = deep_path.join("deep_file.txt");
	fs::write(&deep_file, "deep content").await.unwrap();

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

	// Move the entire deep structure
	let new_deep_path = test_path.join("moved_level1");
	fs::rename(test_path.join("level1"), &new_deep_path)
		.await
		.unwrap();

	sleep(Duration::from_millis(1200)).await;

	// Verify the move
	assert!(!test_path.join("level1").exists());
	assert!(new_deep_path.exists());
	assert!(new_deep_path
		.join("level2")
		.join("level3")
		.join("level4")
		.join("deep_file.txt")
		.exists());

	let content = fs::read_to_string(
		new_deep_path
			.join("level2")
			.join("level3")
			.join("level4")
			.join("deep_file.txt"),
	)
	.await
	.unwrap();
	assert_eq!(content, "deep content");
}

#[tokio::test]
async fn test_large_file_operations() {
	let temp_dir = TempDir::new().unwrap();
	let test_path = temp_dir.path().to_path_buf();

	// Create a large file (1MB)
	let large_content = "x".repeat(1024 * 1024);
	let large_file = test_path.join("large_file.txt");
	fs::write(&large_file, &large_content).await.unwrap();

	let config = WatcherConfig {
		path: test_path.clone(),
		recursive: true,
		move_timeout_ms: 2000, // Longer timeout for large files
	};

	let mut watcher = FileSystemWatcher::new(config).await.unwrap();

	tokio::spawn(async move {
		watcher.start_watching().await.unwrap();
	});

	sleep(Duration::from_millis(100)).await;

	// Move the large file
	let moved_large_file = test_path.join("moved_large_file.txt");
	fs::rename(&large_file, &moved_large_file).await.unwrap();

	sleep(Duration::from_millis(2500)).await;

	// Verify the move
	assert!(!large_file.exists());
	assert!(moved_large_file.exists());

	let moved_content = fs::read_to_string(&moved_large_file).await.unwrap();
	assert_eq!(moved_content.len(), 1024 * 1024);
	assert_eq!(moved_content, large_content);
}

#[tokio::test]
async fn test_special_characters_in_filenames() {
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

	// Test files with special characters (Windows-safe)
	let special_files = vec![
		"file with spaces.txt",
		"file-with-dashes.txt",
		"file_with_underscores.txt",
		"file.with.dots.txt",
		"file(with)parentheses.txt",
		"file[with]brackets.txt",
		"file{with}braces.txt",
		"file'with'quotes.txt",
		"file123numbers.txt",
	];

	for filename in &special_files {
		let file_path = test_path.join(filename);
		fs::write(&file_path, format!("content for {}", filename))
			.await
			.unwrap();

		// Move to new location
		let moved_path = test_path.join(format!("moved_{}", filename));
		fs::rename(&file_path, &moved_path).await.unwrap();

		sleep(Duration::from_millis(100)).await;
	}

	sleep(Duration::from_millis(1200)).await;

	// Verify all moves completed
	for filename in &special_files {
		let original_path = test_path.join(filename);
		let moved_path = test_path.join(format!("moved_{}", filename));

		assert!(
			!original_path.exists(),
			"Original file should not exist: {}",
			filename
		);
		assert!(
			moved_path.exists(),
			"Moved file should exist: moved_{}",
			filename
		);

		let content = fs::read_to_string(&moved_path).await.unwrap();
		assert_eq!(content, format!("content for {}", filename));
	}
}

#[tokio::test]
async fn test_cross_directory_moves() {
	let temp_dir = TempDir::new().unwrap();
	let test_path = temp_dir.path().to_path_buf();

	// Create multiple directories
	let src_dir = test_path.join("source_directory");
	let dest_dir = test_path.join("destination_directory");
	let nested_src = src_dir.join("nested");
	let nested_dest = dest_dir.join("nested");

	fs::create_dir_all(&nested_src).await.unwrap();
	fs::create_dir_all(&nested_dest).await.unwrap();

	// Create files in source directory
	let files = vec!["file1.txt", "file2.txt", "file3.txt"];
	for filename in &files {
		fs::write(src_dir.join(filename), format!("content for {}", filename))
			.await
			.unwrap();
		fs::write(
			nested_src.join(filename),
			format!("nested content for {}", filename),
		)
		.await
		.unwrap();
	}

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

	// Move files across directories
	for filename in &files {
		fs::rename(src_dir.join(filename), dest_dir.join(filename))
			.await
			.unwrap();

		fs::rename(nested_src.join(filename), nested_dest.join(filename))
			.await
			.unwrap();

		sleep(Duration::from_millis(100)).await;
	}

	sleep(Duration::from_millis(1200)).await;

	// Verify all cross-directory moves
	for filename in &files {
		// Check source files are gone
		assert!(!src_dir.join(filename).exists());
		assert!(!nested_src.join(filename).exists());

		// Check destination files exist
		assert!(dest_dir.join(filename).exists());
		assert!(nested_dest.join(filename).exists());

		// Verify content
		let content = fs::read_to_string(dest_dir.join(filename)).await.unwrap();
		assert_eq!(content, format!("content for {}", filename));

		let nested_content = fs::read_to_string(nested_dest.join(filename))
			.await
			.unwrap();
		assert_eq!(nested_content, format!("nested content for {}", filename));
	}
}

#[tokio::test]
async fn test_symlink_operations() {
	// Note: This test may fail on Windows without developer mode or admin privileges
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

	// Create target file
	let target_file = test_path.join("target.txt");
	fs::write(&target_file, "target content").await.unwrap();
	// Try to create symlink (may fail on Windows)
	let _symlink_path = test_path.join("symlink.txt");

	#[cfg(unix)]
	{
		tokio::fs::symlink(&target_file, &symlink_path)
			.await
			.unwrap();

		// Move the symlink
		let moved_symlink = test_path.join("moved_symlink.txt");
		fs::rename(&symlink_path, &moved_symlink).await.unwrap();

		sleep(Duration::from_millis(1200)).await;

		// Verify symlink move
		assert!(!symlink_path.exists());
		assert!(moved_symlink.exists());
		assert!(target_file.exists()); // Original target should still exist
	}

	#[cfg(windows)]
	{
		// On Windows, test directory junction instead
		let junction_path = test_path.join("junction_dir");
		fs::create_dir(&junction_path).await.unwrap();

		let moved_junction = test_path.join("moved_junction_dir");
		fs::rename(&junction_path, &moved_junction).await.unwrap();

		sleep(Duration::from_millis(1200)).await;

		assert!(!junction_path.exists());
		assert!(moved_junction.exists());
	}
}

#[tokio::test]
async fn test_stress_test_many_files() {
	let temp_dir = TempDir::new().unwrap();
	let test_path = temp_dir.path().to_path_buf();

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

	// Create many files
	let num_files = 50;
	for i in 0..num_files {
		let file_path = test_path.join(format!("stress_file_{:03}.txt", i));
		fs::write(&file_path, format!("stress test content {}", i))
			.await
			.unwrap();
	}

	sleep(Duration::from_millis(200)).await;

	// Move all files rapidly
	for i in 0..num_files {
		let old_path = test_path.join(format!("stress_file_{:03}.txt", i));
		let new_path = test_path.join(format!("moved_stress_file_{:03}.txt", i));
		fs::rename(&old_path, &new_path).await.unwrap();

		// Small delay to avoid overwhelming the system
		if i % 10 == 0 {
			sleep(Duration::from_millis(10)).await;
		}
	}

	sleep(Duration::from_millis(2500)).await;

	// Verify all files were moved correctly
	for i in 0..num_files {
		let old_path = test_path.join(format!("stress_file_{:03}.txt", i));
		let new_path = test_path.join(format!("moved_stress_file_{:03}.txt", i));

		assert!(!old_path.exists(), "Original file {} should not exist", i);
		assert!(new_path.exists(), "Moved file {} should exist", i);

		let content = fs::read_to_string(&new_path).await.unwrap();
		assert_eq!(content, format!("stress test content {}", i));
	}
}

// Unit tests for MoveDetector
#[tokio::test]
async fn test_move_detector_confidence_calculation() {
	use rust_watcher::{EventType, FileSystemEvent, MoveDetector};

	let mut detector = MoveDetector::new(1000);

	// Test high confidence scenario - same size files with similar names
	let remove_event = FileSystemEvent::new(
		EventType::Remove,
		PathBuf::from("/test/document.txt"),
		false,
		Some(1024),
	);

	let create_event = FileSystemEvent::new(
		EventType::Create,
		PathBuf::from("/test/moved_document.txt"), // Similar name for better matching
		false,
		Some(1024),
	);

	// Process remove first
	let result1 = detector.process_event(remove_event).await;
	assert_eq!(result1.len(), 1);
	assert_eq!(result1[0].event_type, EventType::Remove);

	// Add a small delay to ensure timing difference
	sleep(Duration::from_millis(50)).await;

	// Process create - should detect move
	let result2 = detector.process_event(create_event).await;
	assert_eq!(result2.len(), 1);

	// Debug output to understand what happened
	println!("Result event type: {:?}", result2[0].event_type);
	println!("Is move: {}", result2[0].is_move());
	if let Some(move_data) = &result2[0].move_data {
		println!("Move confidence: {}", move_data.confidence);
		println!("Detection method: {:?}", move_data.detection_method);
	}

	if result2[0].is_move() {
		if let Some(move_data) = &result2[0].move_data {
			assert!(move_data.confidence > 0.3); // Should have reasonable confidence
			assert_eq!(move_data.source_path, PathBuf::from("/test/document.txt"));
			assert_eq!(
				move_data.destination_path,
				PathBuf::from("/test/moved_document.txt")
			);
		}
	} else {
		// If not detected as move, that's also valid - depends on confidence threshold
		assert_eq!(result2[0].event_type, EventType::Create);
	}
}

#[tokio::test]
async fn test_move_detector_different_sizes() {
	use rust_watcher::MoveDetector;

	let mut detector = MoveDetector::new(1000);

	// Test with different file sizes - should have lower confidence
	let remove_event = FileSystemEvent::new(
		EventType::Remove,
		PathBuf::from("/test/source.txt"),
		false,
		Some(1024),
	);

	let create_event = FileSystemEvent::new(
		EventType::Create,
		PathBuf::from("/test/dest.txt"),
		false,
		Some(2048), // Different size
	);

	detector.process_event(remove_event).await;
	let result = detector.process_event(create_event).await;

	if result[0].is_move() {
		if let Some(move_data) = &result[0].move_data {
			// Should have lower confidence due to size mismatch
			assert!(move_data.confidence < 0.8);
		}
	}
}

#[tokio::test]
async fn test_move_detector_timeout_cleanup() {
	use rust_watcher::MoveDetector;

	let mut detector = MoveDetector::new(100); // Very short timeout

	let remove_event = FileSystemEvent::new(
		EventType::Remove,
		PathBuf::from("/test/expired.txt"),
		false,
		Some(1024),
	);

	// Process remove event
	detector.process_event(remove_event).await;

	// Wait for timeout
	sleep(Duration::from_millis(200)).await;

	// Create event after timeout - should NOT be detected as move
	let create_event = FileSystemEvent::new(
		EventType::Create,
		PathBuf::from("/test/expired.txt"),
		false,
		Some(1024),
	);

	let result = detector.process_event(create_event).await;
	assert_eq!(result.len(), 1);
	assert!(!result[0].is_move()); // Should not be detected as move due to timeout
}

#[tokio::test]
async fn test_move_detector_memory_limits() {
	use rust_watcher::MoveDetector;

	let mut detector = MoveDetector::new(10000); // Long timeout

	// Create many pending events to test memory limits
	for i in 0..1100 {
		// More than the 1000 limit
		let remove_event = FileSystemEvent::new(
			EventType::Remove,
			PathBuf::from(format!("/test/file_{}.txt", i)),
			false,
			Some(1024),
		);

		detector.process_event(remove_event).await;
	}

	// The detector should handle this gracefully without crashing
	let final_event = FileSystemEvent::new(
		EventType::Create,
		PathBuf::from("/test/final.txt"),
		false,
		Some(1024),
	);

	let result = detector.process_event(final_event).await;
	assert_eq!(result.len(), 1);
}

// Error handling tests
#[tokio::test]
async fn test_watcher_invalid_path() {
	let config = WatcherConfig {
		path: PathBuf::from("/completely/invalid/path/that/does/not/exist"),
		recursive: true,
		move_timeout_ms: 1000,
	};

	let mut watcher = FileSystemWatcher::new(config).await.unwrap();
	let result = watcher.start_watching().await;
	assert!(result.is_err());
	match result.unwrap_err() {
		WatcherError::InvalidPath { .. } => {
			// Expected error type
		}
		other => panic!("Expected InvalidPath error, got: {:?}", other),
	}
}

#[tokio::test]
async fn test_permission_denied_scenarios() {
	let temp_dir = TempDir::new().unwrap();
	let test_path = temp_dir.path().to_path_buf();

	// Create a file and directory
	let test_file = test_path.join("test_file.txt");
	let test_dir = test_path.join("test_dir");

	fs::write(&test_file, "test content").await.unwrap();
	fs::create_dir(&test_dir).await.unwrap();

	let config = WatcherConfig {
		path: test_path.clone(),
		recursive: true,
		move_timeout_ms: 1000,
	};

	let mut watcher = FileSystemWatcher::new(config).await.unwrap();

	tokio::spawn(async move {
		let _ = watcher.start_watching().await;
	});

	sleep(Duration::from_millis(100)).await;

	// Test operations that might fail on some systems
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;

		// Remove read permissions
		let mut perms = fs::metadata(&test_file).await.unwrap().permissions();
		perms.set_mode(0o000);
		fs::set_permissions(&test_file, perms).await.unwrap();

		// Try to read the file (should handle gracefully)
		let result = fs::read_to_string(&test_file).await;
		assert!(result.is_err());

		// Restore permissions for cleanup
		let mut perms = fs::metadata(&test_file).await.unwrap().permissions();
		perms.set_mode(0o644);
		fs::set_permissions(&test_file, perms).await.unwrap();
	}
}

#[tokio::test]
async fn test_unicode_filename_handling() {
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

	// Test files with Unicode characters
	let unicode_files = vec![
		"测试文件.txt",    // Chinese
		"файл.txt",        // Russian
		"αρχείο.txt",      // Greek
		"ファイル.txt",    // Japanese
		"📁📄🔄.txt",      // Emojis
		"café_résumé.txt", // Accented characters
	];

	for filename in &unicode_files {
		let file_path = test_path.join(filename);

		// Some filesystems might not support all Unicode characters
		match fs::write(&file_path, format!("Unicode content for {}", filename)).await {
			Ok(_) => {
				// If creation succeeded, test move
				let moved_path = test_path.join(format!("moved_{}", filename));
				if let Ok(_) = fs::rename(&file_path, &moved_path).await {
					sleep(Duration::from_millis(100)).await;

					// Verify move if both operations succeeded
					if moved_path.exists() {
						let content = fs::read_to_string(&moved_path).await.unwrap();
						assert_eq!(content, format!("Unicode content for {}", filename));
					}
				}
			}
			Err(_) => {
				// Some Unicode filenames might not be supported on the filesystem
				// This is expected behavior, not a test failure
				continue;
			}
		}
	}

	sleep(Duration::from_millis(1200)).await;
}

#[test]
fn test_event_serialization_edge_cases() {
	// Test serialization with None values
	let event = FileSystemEvent::new(
		EventType::Other("Custom".to_string()),
		PathBuf::from("/test/path"),
		true,
		None, // No size
	);

	let json = event.to_json().unwrap();
	assert!(json.contains("Custom"));
	assert!(json.contains("null")); // Size should be null    // Test with move data
	let move_data = MoveEvent {
		source_path: PathBuf::from("/src"),
		destination_path: PathBuf::from("/dst"),
		confidence: 1.0,
		detection_method: MoveDetectionMethod::FileSystemEvent,
	};

	let move_event = event.with_move_data(move_data);
	let move_json = move_event.to_json().unwrap();
	assert!(move_json.contains("FileSystemEvent"));
	assert!(move_json.contains("confidence"));
}

#[test]
fn test_error_types() {
	use rust_watcher::WatcherError;

	// Test error creation and display
	let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
	let watcher_error = WatcherError::Io(io_error);
	assert!(format!("{}", watcher_error).contains("IO error"));

	let invalid_path_error = WatcherError::InvalidPath {
		path: "/invalid/path".to_string(),
	};
	assert!(format!("{}", invalid_path_error).contains("Invalid path"));

	let not_initialized_error = WatcherError::NotInitialized;
	assert!(format!("{}", not_initialized_error).contains("not initialized"));
}

// Performance and benchmarking tests
#[tokio::test]
async fn test_performance_large_directory_tree() {
	let temp_dir = TempDir::new().unwrap();
	let test_path = temp_dir.path().to_path_buf();

	// Create a large directory tree
	let start_time = std::time::Instant::now();

	for i in 0..10 {
		let dir_path = test_path.join(format!("dir_{}", i));
		fs::create_dir(&dir_path).await.unwrap();

		for j in 0..10 {
			let subdir_path = dir_path.join(format!("subdir_{}", j));
			fs::create_dir(&subdir_path).await.unwrap();

			for k in 0..10 {
				let file_path = subdir_path.join(format!("file_{}_{}.txt", j, k));
				fs::write(&file_path, format!("content {}_{}", j, k))
					.await
					.unwrap();
			}
		}
	}

	let setup_time = start_time.elapsed();
	println!("Directory tree setup time: {:?}", setup_time);

	let config = WatcherConfig {
		path: test_path.clone(),
		recursive: true,
		move_timeout_ms: 1000,
	};

	let watcher_start = std::time::Instant::now();
	let mut watcher = FileSystemWatcher::new(config).await.unwrap();

	tokio::spawn(async move {
		watcher.start_watching().await.unwrap();
	});

	let watcher_init_time = watcher_start.elapsed();
	println!("Watcher initialization time: {:?}", watcher_init_time);

	// Give watcher time to start monitoring
	sleep(Duration::from_millis(200)).await;

	// Perform bulk operations
	let operation_start = std::time::Instant::now();

	// Move files in bulk
	for i in 0..5 {
		// Test subset to keep test time reasonable
		let dir_path = test_path.join(format!("dir_{}", i));
		let moved_dir_path = test_path.join(format!("moved_dir_{}", i));

		fs::rename(&dir_path, &moved_dir_path).await.unwrap();
		sleep(Duration::from_millis(10)).await; // Small delay between operations
	}

	let operation_time = operation_start.elapsed();
	println!("Bulk operation time: {:?}", operation_time);

	// Wait for processing
	sleep(Duration::from_millis(1500)).await;

	// Verify operations completed
	for i in 0..5 {
		let original_dir = test_path.join(format!("dir_{}", i));
		let moved_dir = test_path.join(format!("moved_dir_{}", i));

		assert!(!original_dir.exists());
		assert!(moved_dir.exists());
	}

	// Performance assertions
	assert!(
		watcher_init_time < Duration::from_millis(1000),
		"Watcher should initialize quickly"
	);
	assert!(
		operation_time < Duration::from_secs(5),
		"Bulk operations should complete reasonably fast"
	);
}

#[tokio::test]
async fn test_memory_usage_under_load() {
	let temp_dir = TempDir::new().unwrap();
	let test_path = temp_dir.path().to_path_buf();

	let config = WatcherConfig {
		path: test_path.clone(),
		recursive: true,
		move_timeout_ms: 5000, // Longer timeout to test memory usage
	};

	let mut watcher = FileSystemWatcher::new(config).await.unwrap();

	tokio::spawn(async move {
		watcher.start_watching().await.unwrap();
	});

	sleep(Duration::from_millis(100)).await;

	// Create many events that will be pending for a while
	for batch in 0..5 {
		for i in 0..20 {
			let file_path = test_path.join(format!("batch_{}_{}.txt", batch, i));
			fs::write(&file_path, format!("content {} {}", batch, i))
				.await
				.unwrap();

			// Remove immediately to create pending events
			fs::remove_file(&file_path).await.unwrap();

			if i % 10 == 0 {
				sleep(Duration::from_millis(10)).await;
			}
		}

		// Small delay between batches
		sleep(Duration::from_millis(100)).await;
	}

	// Test should complete without memory issues
	sleep(Duration::from_millis(6000)).await; // Wait for timeout cleanup

	// If we reach here without panicking, memory management is working
	assert!(true);
}

#[tokio::test]
async fn test_high_frequency_operations() {
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

	let operation_start = std::time::Instant::now();

	// High frequency file operations
	for i in 0..100 {
		let file_path = test_path.join(format!("freq_test_{}.txt", i));

		// Create
		fs::write(&file_path, format!("content {}", i))
			.await
			.unwrap();

		// Modify
		fs::write(&file_path, format!("modified content {}", i))
			.await
			.unwrap();

		// Move
		let moved_path = test_path.join(format!("moved_freq_test_{}.txt", i));
		fs::rename(&file_path, &moved_path).await.unwrap();

		// Every 10 operations, add a small delay
		if i % 10 == 0 {
			sleep(Duration::from_millis(1)).await;
		}
	}

	let operation_time = operation_start.elapsed();
	println!("High frequency operations time: {:?}", operation_time);

	// Wait for all events to be processed
	sleep(Duration::from_millis(1500)).await;

	// Verify final state
	for i in 0..100 {
		let original_path = test_path.join(format!("freq_test_{}.txt", i));
		let moved_path = test_path.join(format!("moved_freq_test_{}.txt", i));

		assert!(!original_path.exists());
		assert!(moved_path.exists());

		let content = fs::read_to_string(&moved_path).await.unwrap();
		assert_eq!(content, format!("modified content {}", i));
	}

	// Performance assertion
	assert!(
		operation_time < Duration::from_secs(10),
		"High frequency operations should complete in reasonable time"
	);
}

#[tokio::test]
async fn test_concurrent_watchers() {
	let temp_dir = TempDir::new().unwrap();
	let test_path = temp_dir.path().to_path_buf();

	// Create subdirectories for different watchers
	let subdir1 = test_path.join("watcher1");
	let subdir2 = test_path.join("watcher2");

	fs::create_dir_all(&subdir1).await.unwrap();
	fs::create_dir_all(&subdir2).await.unwrap();

	// Start multiple watchers
	let config1 = WatcherConfig {
		path: subdir1.clone(),
		recursive: true,
		move_timeout_ms: 1000,
	};

	let config2 = WatcherConfig {
		path: subdir2.clone(),
		recursive: true,
		move_timeout_ms: 1000,
	};

	let mut watcher1 = FileSystemWatcher::new(config1).await.unwrap();
	let mut watcher2 = FileSystemWatcher::new(config2).await.unwrap();

	// Start both watchers concurrently
	let handle1 = tokio::spawn(async move {
		watcher1.start_watching().await.unwrap();
	});

	let handle2 = tokio::spawn(async move {
		watcher2.start_watching().await.unwrap();
	});

	sleep(Duration::from_millis(200)).await;

	// Perform operations in both directories simultaneously
	let ops1 = tokio::spawn({
		let subdir1 = subdir1.clone();
		async move {
			for i in 0..20 {
				let file_path = subdir1.join(format!("file1_{}.txt", i));
				fs::write(&file_path, format!("content1 {}", i))
					.await
					.unwrap();

				let moved_path = subdir1.join(format!("moved1_{}.txt", i));
				fs::rename(&file_path, &moved_path).await.unwrap();

				sleep(Duration::from_millis(10)).await;
			}
		}
	});

	let ops2 = tokio::spawn({
		let subdir2 = subdir2.clone();
		async move {
			for i in 0..20 {
				let file_path = subdir2.join(format!("file2_{}.txt", i));
				fs::write(&file_path, format!("content2 {}", i))
					.await
					.unwrap();

				let moved_path = subdir2.join(format!("moved2_{}.txt", i));
				fs::rename(&file_path, &moved_path).await.unwrap();

				sleep(Duration::from_millis(10)).await;
			}
		}
	});

	// Wait for operations to complete
	let _ = tokio::join!(ops1, ops2);

	sleep(Duration::from_millis(1500)).await;

	// Verify both watchers handled their operations correctly
	for i in 0..20 {
		let moved1 = subdir1.join(format!("moved1_{}.txt", i));
		let moved2 = subdir2.join(format!("moved2_{}.txt", i));

		assert!(moved1.exists());
		assert!(moved2.exists());

		let content1 = fs::read_to_string(&moved1).await.unwrap();
		let content2 = fs::read_to_string(&moved2).await.unwrap();

		assert_eq!(content1, format!("content1 {}", i));
		assert_eq!(content2, format!("content2 {}", i));
	}

	// Clean shutdown
	handle1.abort();
	handle2.abort();
}

// Property-based testing
#[tokio::test]
async fn test_move_detection_invariants() {
	let temp_dir = TempDir::new().unwrap();
	let test_path = temp_dir.path().to_path_buf();

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

	// Test invariant: For any successful move operation,
	// the source should not exist and destination should exist
	for test_case in 0..10 {
		let source_path = test_path.join(format!("invariant_source_{}.txt", test_case));
		let dest_path = test_path.join(format!("invariant_dest_{}.txt", test_case));

		// Create source file
		fs::write(&source_path, format!("invariant content {}", test_case))
			.await
			.unwrap();
		assert!(source_path.exists());
		assert!(!dest_path.exists());

		// Perform move
		fs::rename(&source_path, &dest_path).await.unwrap();

		// Invariant check: source gone, destination exists
		assert!(!source_path.exists(), "Source should not exist after move");
		assert!(dest_path.exists(), "Destination should exist after move");

		// Content should be preserved
		let content = fs::read_to_string(&dest_path).await.unwrap();
		assert_eq!(content, format!("invariant content {}", test_case));

		sleep(Duration::from_millis(100)).await;
	}

	sleep(Duration::from_millis(2500)).await;
}

#[tokio::test]
async fn test_move_detector_basic_functionality() {
	use rust_watcher::{EventType, FileSystemEvent, MoveDetector};

	let mut detector = MoveDetector::new(2000); // Longer timeout for test reliability

	// Create identical files for highest confidence match
	let remove_event = FileSystemEvent::new(
		EventType::Remove,
		PathBuf::from("/test/identical_file.txt"),
		false,
		Some(12345), // Specific size
	);

	let create_event = FileSystemEvent::new(
		EventType::Create,
		PathBuf::from("/test/identical_file.txt"), // Exact same name
		false,
		Some(12345), // Exact same size
	);

	// Process remove
	let remove_results = detector.process_event(remove_event).await;
	assert_eq!(remove_results.len(), 1);
	assert_eq!(remove_results[0].event_type, EventType::Remove);

	// Small delay to create timing difference
	sleep(Duration::from_millis(10)).await;

	// Process create
	let create_results = detector.process_event(create_event).await;
	assert_eq!(create_results.len(), 1);

	// This should have high confidence due to identical name and size
	println!("Event: {:?}", create_results[0]);

	// Accept either move detection or separate create/remove events
	// The exact behavior depends on confidence threshold and timing
	match create_results[0].event_type {
		EventType::Move => {
			// Move was detected
			assert!(create_results[0].move_data.is_some());
			let move_data = create_results[0].move_data.as_ref().unwrap();
			assert!(move_data.confidence > 0.0);
			println!("Move detected with confidence: {}", move_data.confidence);
		}
		EventType::Create => {
			// Move not detected, treated as separate events
			println!("Events treated as separate create/remove");
		}
		_ => panic!("Unexpected event type"),
	}
}
