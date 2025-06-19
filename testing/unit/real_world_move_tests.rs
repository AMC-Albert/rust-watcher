/// Real-world move detection tests that perform actual file operations
/// These tests verify that the move detector correctly handles real filesystem events
#[cfg(test)]
mod tests {
	use rust_watcher::{start, WatcherConfig};
	use tempfile::TempDir;
	use tokio::time::{sleep, timeout, Duration};

	/// Test that verifies Windows cut/paste operations are detected correctly
	/// This test performs actual file operations using std::fs::rename() to generate
	/// real filesystem events, ensuring the move detector works with real-world scenarios
	#[tokio::test]
	async fn test_real_world_cut_paste_detection() {
		// Create a temporary directory for testing
		let temp_dir = TempDir::new().expect("Failed to create temp directory");
		let temp_path = temp_dir.path().to_path_buf();

		// Create a subdirectory for the move destination
		let dest_dir = temp_path.join("subfolder");
		std::fs::create_dir(&dest_dir).expect("Failed to create destination directory");

		// Start the watcher
		let config = WatcherConfig {
			path: temp_path.clone(),
			recursive: true,
			move_detector_config: None, // Use default
		};

		let (handle, mut receiver) = start(config).expect("Failed to start watcher");

		// Give the watcher time to initialize
		sleep(Duration::from_millis(100)).await;

		// Create the test file AFTER starting the watcher so it gets cached
		let source_file = temp_path.join("test_file.txt");
		let dest_file = dest_dir.join("test_file.txt");
		std::fs::write(
			&source_file,
			"Hello, World! This is a test file for move detection.",
		)
		.expect("Failed to create test file");

		// Wait for the create event to be processed and cached
		sleep(Duration::from_millis(200)).await;

		// Perform the actual move operation using std::fs::rename()
		// This generates the same Remove + Create events as Windows File Explorer cut/paste
		std::fs::rename(&source_file, &dest_file).expect("Failed to move file");

		// Collect events with a timeout
		let mut move_detected = false;
		let mut events_received = 0;

		let timeout_result = timeout(Duration::from_secs(3), async {
			while let Some(event) = receiver.recv().await {
				events_received += 1;

				if let Some(move_data) = &event.move_data {
					// Verify this is the move we're looking for
					if move_data.source_path == source_file
						&& move_data.destination_path == dest_file
					{
						move_detected = true;

						// Verify the move has reasonable confidence
						assert!(
							move_data.confidence >= 0.5,
							"Move should have confidence >= 0.5, got: {}",
							move_data.confidence
						);

						// Verify paths are different (this was the bug we fixed)
						assert_ne!(
							move_data.source_path, move_data.destination_path,
							"Source and destination paths should be different"
						);

						break;
					}
				}
			}
		})
		.await;

		// Stop the watcher
		handle.stop().await.expect("Failed to stop watcher");

		// Verify results
		assert!(
			timeout_result.is_ok(),
			"Timed out waiting for move detection after receiving {} events",
			events_received
		);

		assert!(
			move_detected,
			"Move should have been detected after {} events",
			events_received
		);

		// Verify the file actually moved
		assert!(!source_file.exists(), "Source file should no longer exist");
		assert!(dest_file.exists(), "Destination file should exist");
	}

	/// Test rapid moves to ensure the detector handles quick successive operations
	#[tokio::test]
	async fn test_rapid_move_operations() {
		let temp_dir = TempDir::new().expect("Failed to create temp directory");
		let temp_path = temp_dir.path().to_path_buf();

		// Create multiple subdirectories
		let dest_dir1 = temp_path.join("folder1");
		let dest_dir2 = temp_path.join("folder2");
		std::fs::create_dir(&dest_dir1).expect("Failed to create dest dir 1");
		std::fs::create_dir(&dest_dir2).expect("Failed to create dest dir 2");

		let config = WatcherConfig {
			path: temp_path.clone(),
			recursive: true,
			move_detector_config: None,
		};

		let (handle, mut receiver) = start(config).expect("Failed to start watcher");
		sleep(Duration::from_millis(100)).await;

		// Create test file
		let file1 = temp_path.join("rapid_test.txt");
		let file2 = dest_dir1.join("rapid_test.txt");
		let file3 = dest_dir2.join("rapid_test.txt");

		std::fs::write(&file1, "Rapid move test content").expect("Failed to create test file");
		sleep(Duration::from_millis(200)).await;

		// Perform rapid moves: file1 -> file2 -> file3
		std::fs::rename(&file1, &file2).expect("Failed to perform first move");
		sleep(Duration::from_millis(50)).await; // Short delay
		std::fs::rename(&file2, &file3).expect("Failed to perform second move");

		// Collect events and verify at least one move is detected
		let mut moves_detected = 0;
		let timeout_result = timeout(Duration::from_secs(3), async {
			while let Some(event) = receiver.recv().await {
				if event.move_data.is_some() {
					moves_detected += 1;
					if moves_detected >= 1 {
						break; // We expect at least one move to be detected
					}
				}
			}
		})
		.await;

		handle.stop().await.expect("Failed to stop watcher");

		assert!(
			timeout_result.is_ok(),
			"Timed out waiting for move detection"
		);
		assert!(
			moves_detected >= 1,
			"At least one move should be detected in rapid operations, got: {}",
			moves_detected
		);

		// Verify final state
		assert!(!file1.exists(), "Original file should not exist");
		assert!(!file2.exists(), "Intermediate file should not exist");
		assert!(file3.exists(), "Final file should exist");
	}

	/// Test that same-path operations are NOT detected as moves
	/// This verifies our fix for the bug where Remove/Create at the same path was detected as a move
	#[tokio::test]
	async fn test_same_path_not_detected_as_move() {
		let temp_dir = TempDir::new().expect("Failed to create temp directory");
		let temp_path = temp_dir.path().to_path_buf();

		let config = WatcherConfig {
			path: temp_path.clone(),
			recursive: true,
			move_detector_config: None,
		};

		let (handle, mut receiver) = start(config).expect("Failed to start watcher");
		sleep(Duration::from_millis(100)).await;

		let test_file = temp_path.join("same_path_test.txt");

		// Create file
		std::fs::write(&test_file, "Original content").expect("Failed to create test file");
		sleep(Duration::from_millis(200)).await;

		// Delete and recreate at same path (should NOT be detected as move)
		std::fs::remove_file(&test_file).expect("Failed to remove file");
		sleep(Duration::from_millis(50)).await;
		std::fs::write(&test_file, "New content").expect("Failed to recreate file");

		// Collect events and verify no moves are detected for same-path operations
		let mut same_path_moves = 0;
		let timeout_result = timeout(Duration::from_secs(2), async {
			while let Some(event) = receiver.recv().await {
				if let Some(move_data) = &event.move_data {
					if move_data.source_path == move_data.destination_path {
						same_path_moves += 1;
					}
				}
			}
		})
		.await;

		handle.stop().await.expect("Failed to stop watcher");

		// We expect timeout since no same-path moves should be detected
		assert!(
			timeout_result.is_err(),
			"Should timeout since no same-path moves expected"
		);
		assert_eq!(
			same_path_moves, 0,
			"Same-path operations should NOT be detected as moves"
		);
	}

	/// Test the specific bug we discovered and fixed: Windows File Explorer cut/paste
	/// where Remove events have no size but Create events do have size
	#[tokio::test]
	async fn test_windows_file_explorer_cut_paste_bug_fix() {
		use rust_watcher::{EventType, FileSystemEvent, MoveDetector, MoveDetectorConfig};
		use std::path::PathBuf;
		use tokio::time::{sleep, Duration};

		let mut detector = MoveDetector::new(MoveDetectorConfig::default());

		// Simulate the exact scenario from Windows File Explorer cut/paste:
		// 1. Remove event with NO size (file already gone, can't get metadata)
		// 2. Create event WITH size (file exists at destination)

		let source_path = PathBuf::from("C:\\Users\\Albert\\Downloads\\test_file.txt");
		let dest_path =
			PathBuf::from("C:\\Users\\Albert\\Downloads\\BlenderPreviews\\test_file.txt");

		// Remove event - this is how Windows File Explorer generates it (no size)
		let remove_event = FileSystemEvent::new(
			EventType::Remove,
			source_path.clone(),
			false,
			None, // ← This was the key issue: Remove event has NO size
		);

		println!("Processing remove event (no size): {:?}", remove_event.path);
		let remove_result = detector.process_event(remove_event).await;
		assert_eq!(remove_result.len(), 1, "Remove event should be processed");

		// Small delay to simulate cut/paste timing
		sleep(Duration::from_millis(50)).await;

		// Create event - this is how Windows File Explorer generates it (with size)
		let create_event = FileSystemEvent::new(
			EventType::Create,
			dest_path.clone(),
			false,
			Some(12345), // ← Create event HAS size because file exists at destination
		);

		println!(
			"Processing create event (with size): {:?}",
			create_event.path
		);
		let create_result = detector.process_event(create_event).await;

		// Verify the move was detected despite the size mismatch
		assert_eq!(
			create_result.len(),
			1,
			"Create event should return one result"
		);

		if let Some(move_data) = &create_result[0].move_data {
			println!("✅ MOVE DETECTED!");
			println!("  Source: {:?}", move_data.source_path);
			println!("  Destination: {:?}", move_data.destination_path);
			println!("  Confidence: {:.3}", move_data.confidence);

			// Verify the paths are correct
			assert_eq!(move_data.source_path, source_path);
			assert_eq!(move_data.destination_path, dest_path);

			// Confidence should be reasonable (0.6 for size mismatch + other factors)
			assert!(
				move_data.confidence >= 0.4,
				"Move should have reasonable confidence despite size mismatch, got: {}",
				move_data.confidence
			);

			// Verify this isn't a same-path false positive
			assert_ne!(
				move_data.source_path, move_data.destination_path,
				"Source and destination should be different"
			);
		} else {
			panic!("Move should have been detected! This was the bug we fixed.");
		}
	}
}
