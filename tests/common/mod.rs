// Common test utilities for integration tests
// This module provides shared functionality without being treated as a test file

use std::path::PathBuf;
use tempfile::TempDir;

/// Create a temporary directory for testing
pub fn setup_temp_dir() -> TempDir {
	TempDir::new().expect("Failed to create temp directory")
}

/// Create a test file with content
pub fn create_test_file(path: &std::path::Path, content: &str) -> std::io::Result<()> {
	std::fs::write(path, content)
}

/// Create multiple test files for testing
#[allow(dead_code)]
pub fn create_test_files(dir: &std::path::Path, count: usize) -> std::io::Result<Vec<PathBuf>> {
	let mut files = Vec::new();
	for i in 0..count {
		let file_path = dir.join(format!("test_file_{}.txt", i));
		create_test_file(&file_path, &format!("Content for file {}", i))?;
		files.push(file_path);
	}
	Ok(files)
}

/// Wait for a short duration to allow file system events to propagate
#[allow(dead_code)]
pub async fn wait_for_events() {
	tokio::time::sleep(std::time::Duration::from_millis(100)).await;
}

/// Create a short timeout for testing
#[allow(dead_code)]
pub async fn timeout_short() {
	tokio::time::sleep(std::time::Duration::from_millis(50)).await;
}

/// Create a longer timeout for testing
#[allow(dead_code)]
pub async fn timeout_long() {
	tokio::time::sleep(std::time::Duration::from_millis(500)).await;
}
