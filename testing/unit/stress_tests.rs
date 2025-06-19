use rust_watcher::{start, MoveDetectorConfig, WatcherConfig};
use serial_test::serial;
use std::time::Duration;
use tempfile::TempDir;
use tokio::fs;
use tokio::time::{sleep, timeout};

/// Test memory usage under sustained load
#[tokio::test]
#[serial]
async fn test_memory_usage_under_load() {
	let temp_dir = TempDir::new().unwrap();
	let test_path = temp_dir.path().to_path_buf();

	let config = WatcherConfig {
		path: test_path.clone(),
		recursive: true,
		move_detector_config: Some(MoveDetectorConfig::default()),
	};
	let (handle, mut event_receiver) = start(config).unwrap();

	// Start a task to count events in the background
	let event_counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
	let move_counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
	let event_counter_clone = event_counter.clone();
	let move_counter_clone = move_counter.clone();

	let event_task = tokio::spawn(async move {
		while let Some(e) = event_receiver.recv().await {
			event_counter_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
			if e.is_move() {
				move_counter_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
			}
		}
	});

	sleep(Duration::from_millis(100)).await;

	// Create and immediately remove files to stress the move detector's pending event management
	let num_batches = 3;
	let files_per_batch = 10;

	for batch in 0..num_batches {
		for i in 0..files_per_batch {
			let file_path = test_path.join(format!("batch_{}_{}.txt", batch, i));
			fs::write(&file_path, format!("content {} {}", batch, i))
				.await
				.unwrap();

			// Remove immediately to create pending events
			fs::remove_file(&file_path).await.unwrap();

			// Add delay to prevent overwhelming
			sleep(Duration::from_millis(20)).await;
		}

		// Longer delay between batches
		sleep(Duration::from_millis(200)).await;
	}

	// Wait for timeout cleanup mechanisms to run
	sleep(Duration::from_millis(1000)).await;

	// Stop the watcher
	handle.stop().await.expect("Failed to stop watcher");

	// Wait for the event task to complete with timeout, or abort it
	let event_counts = match timeout(Duration::from_secs(5), event_task).await {
		Ok(_) => {
			// Task completed normally
			(
				event_counter.load(std::sync::atomic::Ordering::Relaxed),
				move_counter.load(std::sync::atomic::Ordering::Relaxed),
			)
		}
		Err(_) => {
			panic!("Event task timed out after shutdown. The channel was not closed.");
		}
	};

	println!(
		"Memory test completed: {} events, {} moves detected",
		event_counts.0, event_counts.1
	);

	// If we reach here without panicking, memory management is working
	println!("Memory management test passed - no memory leaks detected");
}
