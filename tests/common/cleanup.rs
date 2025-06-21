//! Utility to clean up the test_artifacts directory after tests.

use std::fs;
use std::path::Path;

use super::TEST_ARTIFACTS_DIR;

/// Remove all files and directories under the test artifacts directory.
pub fn cleanup_test_artifacts() {
	let path = Path::new(TEST_ARTIFACTS_DIR);
	if path.exists() {
		// Remove all contents, but not the directory itself
		for entry in fs::read_dir(path)
			.expect("Failed to read test_artifacts dir")
			.flatten()
		{
			let entry_path = entry.path();
			if entry_path.is_dir() {
				fs::remove_dir_all(&entry_path).ok();
			} else {
				fs::remove_file(&entry_path).ok();
			}
		}
	}
}
