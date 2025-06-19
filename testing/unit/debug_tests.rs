#[cfg(test)]
mod tests {
	use rust_watcher::{start, WatcherConfig};
	use std::time::Duration;
	use tempfile::TempDir;
	use tokio::fs;
	use tokio::time::{sleep, timeout};
	#[tokio::test]
	async fn test_minimal_watcher_debug() {
		println!("Starting minimal watcher test");

		let temp_dir = TempDir::new().unwrap();
		let test_path = temp_dir.path().to_path_buf();
		println!("Created temp dir: {:?}", test_path);

		let config = WatcherConfig {
			path: test_path.clone(),
			recursive: true,
			move_detector_config: None,
		};
		println!("Created config");

		let (handle, mut event_receiver) = start(config).unwrap();
		println!("Started watching");
		// Give the watcher more time to initialize
		sleep(Duration::from_millis(200)).await;

		// Create a simple event processing task
		let event_task = tokio::spawn(async move {
			let mut count = 0;
			while let Some(_event) = event_receiver.recv().await {
				count += 1;
				println!("Received event {}", count);
			}
			count
		});

		// Create a simple file
		let test_file = test_path.join("test.txt");
		println!("Creating test file");
		fs::write(&test_file, "test content").await.unwrap();

		// Wait longer for the event to propagate
		sleep(Duration::from_millis(1500)).await;

		// Stop the watcher
		println!("Stopping watcher");
		handle.stop().await.unwrap();

		// Wait for event task with timeout
		let event_count = match timeout(Duration::from_secs(3), event_task).await {
			Ok(Ok(count)) => count,
			Ok(Err(_)) => {
				println!("Event task panicked");
				0
			}
			Err(_) => {
				panic!("Event task timed out after shutdown. The channel was not closed.");
			}
		};

		println!("Test completed with {} events", event_count);
		assert!(event_count > 0, "Should have received at least one event");
	}
}
