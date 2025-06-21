//! Utility to clean up the test_artifacts directory after tests.

use std::fs;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use super::TEST_ARTIFACTS_DIR;

/// Remove all files and directories under the test artifacts directory, with retries for locked files.
pub fn cleanup_test_artifacts() {
	let path = Path::new(TEST_ARTIFACTS_DIR);
	if path.exists() {
		// Remove all contents, but not the directory itself
		for entry in fs::read_dir(path)
			.expect("Failed to read test_artifacts dir")
			.flatten()
		{
			let entry_path = entry.path();
			let mut success = false;
			for _ in 0..5 {
				let result = if entry_path.is_dir() {
					fs::remove_dir_all(&entry_path)
				} else {
					fs::remove_file(&entry_path)
				};
				if result.is_ok() {
					success = true;
					break;
				}
				// Wait a bit and retry (file may still be locked)
				sleep(Duration::from_millis(200));
			}
			if !success {
				eprintln!(
					"Warning: Could not delete {:?} after multiple attempts",
					entry_path
				);
			}
		}
	}
}
