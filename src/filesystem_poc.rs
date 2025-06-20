//! Proof-of-concept filesystem scanning for cache initialization
//!
//! Tests basic functionality needed for the filesystem cache implementation

#![allow(dead_code)] // POC module, used primarily in tests

use std::path::{Path, PathBuf};
use std::time::SystemTime;
use walkdir::{DirEntry, WalkDir};

#[derive(Debug, Clone)]
pub struct FilesystemNode {
	pub path: PathBuf,
	pub is_directory: bool,
	pub size: Option<u64>,
	pub modified_time: Option<SystemTime>,
	pub depth: usize,
}

impl FilesystemNode {
	pub fn from_dir_entry(entry: &DirEntry) -> Result<Self, Box<dyn std::error::Error>> {
		let metadata = entry.metadata()?;

		Ok(FilesystemNode {
			path: entry.path().to_path_buf(),
			is_directory: metadata.is_dir(),
			size: if metadata.is_file() {
				Some(metadata.len())
			} else {
				None
			},
			modified_time: metadata.modified().ok(),
			depth: entry.depth(),
		})
	}
}

/// Scan a directory tree and return filesystem nodes
pub fn scan_directory_tree(root: &Path) -> Result<Vec<FilesystemNode>, Box<dyn std::error::Error>> {
	let mut nodes = Vec::new();

	for entry in WalkDir::new(root) {
		let entry = entry?;

		// Skip hidden files and directories for now
		if entry.file_name().to_string_lossy().starts_with('.') {
			continue;
		}

		let node = FilesystemNode::from_dir_entry(&entry)?;
		nodes.push(node);
	}

	Ok(nodes)
}

/// Get statistics about a directory tree scan
pub fn scan_statistics(nodes: &[FilesystemNode]) -> ScanStats {
	let file_count = nodes.iter().filter(|n| !n.is_directory).count();
	let directory_count = nodes.iter().filter(|n| n.is_directory).count();
	let max_depth = nodes.iter().map(|n| n.depth).max().unwrap_or(0);
	let total_size = nodes.iter().filter_map(|n| n.size).sum::<u64>();

	ScanStats {
		total_nodes: nodes.len(),
		file_count,
		directory_count,
		max_depth,
		total_size,
	}
}

#[derive(Debug)]
pub struct ScanStats {
	pub total_nodes: usize,
	pub file_count: usize,
	pub directory_count: usize,
	pub max_depth: usize,
	pub total_size: u64,
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::fs;
	use tempfile::TempDir;
	#[test]
	fn test_filesystem_scanning_basic() {
		let temp_dir = TempDir::new().expect("Failed to create temp directory");
		let root = temp_dir.path();

		// Create a simple directory structure
		fs::create_dir(root.join("subdir")).unwrap();
		fs::write(root.join("file1.txt"), "content1").unwrap();
		fs::write(root.join("subdir").join("file2.txt"), "content2").unwrap();

		let nodes = scan_directory_tree(root).expect("Failed to scan directory");

		let stats = scan_statistics(&nodes);

		// WalkDir includes: file1.txt, subdir, file2.txt = 3 total (excludes root by default)
		assert_eq!(stats.total_nodes, 3);
		assert_eq!(stats.file_count, 2);
		assert_eq!(stats.directory_count, 1); // just subdir
		assert_eq!(stats.max_depth, 2); // subdir=1, file2.txt=2
	}

	#[test]
	fn test_filesystem_node_creation() {
		let temp_dir = TempDir::new().expect("Failed to create temp directory");
		let file_path = temp_dir.path().join("test.txt");
		fs::write(&file_path, "test content").unwrap();

		let nodes = scan_directory_tree(temp_dir.path()).expect("Failed to scan");

		// Find the file node
		let file_node = nodes
			.iter()
			.find(|n| n.path == file_path)
			.expect("File node not found");

		assert!(!file_node.is_directory);
		assert_eq!(file_node.size, Some(12)); // "test content" = 12 bytes
		assert!(file_node.modified_time.is_some());
	}
	#[test]
	fn test_deep_directory_structure() {
		let temp_dir = TempDir::new().expect("Failed to create temp directory");
		let mut current_path = temp_dir.path().to_path_buf();

		// Create a deep directory structure
		for i in 0..10 {
			current_path = current_path.join(format!("level_{}", i));
			fs::create_dir_all(&current_path).unwrap();
		}

		// Add a file at the deepest level
		fs::write(current_path.join("deep_file.txt"), "deep content").unwrap();
		let nodes = scan_directory_tree(temp_dir.path()).expect("Failed to scan");
		let stats = scan_statistics(&nodes);

		assert!(stats.max_depth >= 10);
		assert_eq!(stats.file_count, 1);
		assert!(stats.directory_count >= 10); // Should be at least 10 levels + root
	}

	#[test]
	fn test_path_normalization() {
		let temp_dir = TempDir::new().expect("Failed to create temp directory");

		// Create files with various name patterns
		fs::write(temp_dir.path().join("normal.txt"), "content").unwrap();
		fs::write(temp_dir.path().join("with spaces.txt"), "content").unwrap();
		fs::write(temp_dir.path().join("unicode_文件.txt"), "content").unwrap();

		let nodes = scan_directory_tree(temp_dir.path()).expect("Failed to scan");

		// All files should be found and have valid paths
		let file_nodes: Vec<_> = nodes.iter().filter(|n| !n.is_directory).collect();

		assert_eq!(file_nodes.len(), 3);

		// Verify all paths are absolute and normalized
		for node in &file_nodes {
			assert!(node.path.is_absolute());
			assert!(node.path.exists());
		}
	}
}
