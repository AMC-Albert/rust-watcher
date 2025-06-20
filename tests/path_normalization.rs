//! Path normalization and cross-platform compatibility tests
//!
//! These tests validate that filesystem paths are handled correctly
//! across different platforms and edge cases.

use rust_watcher::filesystem_poc::scan_directory_tree;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Test basic path normalization for different platforms
#[cfg(test)]
mod path_normalization {
	use super::*;

	#[test]
	fn test_windows_path_handling() {
		// Test Windows-style paths
		let paths = vec![
			r"C:\Users\Test\Documents\file.txt",
			r"\\server\share\file.txt",               // UNC path
			r"C:\Program Files (x86)\App\config.ini", // Spaces and special chars
			r"C:\Users\Test\Documents\файл.txt",      // Unicode filename
		];

		for path_str in paths {
			let path = PathBuf::from(path_str);

			// Path should be valid and have components
			assert!(path.is_absolute() || path.starts_with(r"\\"));
			assert!(path.components().count() > 0);

			// Should be able to convert to string and back
			let as_string = path.to_string_lossy();
			let back_to_path = PathBuf::from(as_string.as_ref());
			assert_eq!(path, back_to_path);
		}
	}

	#[test]
	fn test_unix_path_handling() {
		// Test Unix-style paths
		let paths = vec![
			"/home/user/documents/file.txt",
			"/usr/local/bin/app",
			"/tmp/file with spaces.txt",
			"/home/user/документы/файл.txt", // Unicode
			"/proc/1/status",                // Special filesystem
		];

		for path_str in paths {
			let path = PathBuf::from(path_str);

			// Path should be absolute and have components
			assert!(path.is_absolute());
			assert!(path.components().count() > 0);

			// Should be able to convert to string and back
			let as_string = path.to_string_lossy();
			let back_to_path = PathBuf::from(as_string.as_ref());
			assert_eq!(path, back_to_path);
		}
	}

	#[test]
	fn test_relative_path_normalization() {
		let temp_dir = TempDir::new().expect("Failed to create temp dir");
		let base = temp_dir.path();

		// Create test structure with relative components
		let nested_dir = base.join("level1").join("level2").join("level3");
		fs::create_dir_all(&nested_dir).unwrap();
		fs::write(nested_dir.join("file.txt"), "content").unwrap();

		// Test that scanning normalizes paths correctly
		let nodes = scan_directory_tree(base).expect("Failed to scan");

		for node in &nodes {
			// All paths should be absolute
			assert!(
				node.path.is_absolute(),
				"Path not absolute: {:?}",
				node.path
			);

			// Should start with our base directory
			assert!(
				node.path.starts_with(base),
				"Path not under base: {:?}",
				node.path
			);

			// Should not contain . or .. components
			for component in node.path.components() {
				assert_ne!(component.as_os_str(), ".");
				assert_ne!(component.as_os_str(), "..");
			}
		}
	}

	#[test]
	fn test_unicode_filename_handling() {
		let temp_dir = TempDir::new().expect("Failed to create temp dir");

		// Create files with various Unicode characters
		let unicode_files = vec![
			"файл.txt",      // Cyrillic
			"文件.txt",      // Chinese
			"ファイル.txt",  // Japanese
			"🚀rocket.txt",  // Emoji
			"café_münü.txt", // Accented characters
			"ñoño_año.txt",  // Spanish characters
		];

		for filename in &unicode_files {
			let path = temp_dir.path().join(filename);
			fs::write(&path, "unicode content").unwrap();
		}

		// Scan and verify all Unicode files are found
		let nodes = scan_directory_tree(temp_dir.path()).expect("Failed to scan");
		let file_nodes: Vec<_> = nodes.iter().filter(|n| !n.is_directory).collect();

		assert_eq!(file_nodes.len(), unicode_files.len());

		for node in file_nodes {
			// Should be able to convert path to string
			let path_str = node.path.to_string_lossy();
			assert!(!path_str.is_empty());

			// File should exist
			assert!(node.path.exists());
		}
	}

	#[test]
	fn test_path_length_limits() {
		let temp_dir = TempDir::new().expect("Failed to create temp dir");

		// Create a path that approaches system limits
		let mut long_path = temp_dir.path().to_path_buf();

		// Add many nested directories
		for i in 0..50 {
			long_path = long_path.join(format!("very_long_directory_name_{:03}", i));
		}

		// Try to create as much as the filesystem allows
		let created_path = match fs::create_dir_all(&long_path) {
			Ok(_) => {
				fs::write(long_path.join("file.txt"), "content").unwrap();
				long_path
			}
			Err(_) => {
				// If we hit filesystem limits, create a shorter version
				let mut shorter_path = temp_dir.path().to_path_buf();
				for i in 0..20 {
					shorter_path = shorter_path.join(format!("dir_{}", i));
				}
				fs::create_dir_all(&shorter_path).unwrap();
				fs::write(shorter_path.join("file.txt"), "content").unwrap();
				shorter_path
			}
		};

		// Scanning should handle long paths without crashing
		let nodes = scan_directory_tree(temp_dir.path()).expect("Failed to scan long paths");

		// Should find the file we created
		let file_found = nodes
			.iter()
			.any(|n| !n.is_directory && n.path.starts_with(&created_path));
		assert!(file_found, "Long path file not found in scan");
	}

	#[test]
	fn test_special_characters_in_paths() {
		let temp_dir = TempDir::new().expect("Failed to create temp dir");

		// Test various special characters that might cause issues
		let special_names = vec![
			"file with spaces.txt",
			"file-with-dashes.txt",
			"file_with_underscores.txt",
			"file.with.dots.txt",
			"file(with)parentheses.txt",
			"file[with]brackets.txt",
			"file{with}braces.txt",
			"file'with'quotes.txt",
			"file&with&ampersands.txt",
			"file%with%percent.txt",
			"file+with+plus.txt",
			"file=with=equals.txt",
		];

		for name in &special_names {
			let path = temp_dir.path().join(name);
			if fs::write(&path, "content").is_ok() {
				// File creation succeeded, so scanning should handle it
				assert!(path.exists());
			}
		}

		// Scan should handle all created files
		let nodes = scan_directory_tree(temp_dir.path()).expect("Failed to scan special chars");

		// Should find some files (OS-dependent which special chars are allowed)
		let file_count = nodes.iter().filter(|n| !n.is_directory).count();
		assert!(file_count > 0, "No files found with special characters");
	}

	#[test]
	fn test_case_sensitivity() {
		// On Windows, paths are case-insensitive; on Unix, they are case-sensitive.
		let path1 = PathBuf::from("C:/Test/File.txt");
		let path2 = PathBuf::from("C:/test/file.TXT");
		#[cfg(windows)]
		assert_eq!(
			path1.to_string_lossy().to_lowercase(),
			path2.to_string_lossy().to_lowercase()
		);
		#[cfg(unix)]
		assert_ne!(path1, path2);
	}

	#[test]
	fn test_separator_normalization() {
		// Both separators should be treated equivalently on Windows
		let path1 = PathBuf::from(r"C:\Users\Test\file.txt");
		let path2 = PathBuf::from("C:/Users/Test/file.txt");
		#[cfg(windows)]
		assert_eq!(path1, path2);
		#[cfg(unix)]
		assert_ne!(path1, path2); // On Unix, backslash is a valid character
	}

	#[test]
	#[cfg(windows)]
	fn test_reserved_names_and_trailing_dot_space() {
		// Windows has reserved names and ignores trailing dots/spaces
		let reserved = ["CON", "PRN", "AUX", "NUL", "COM1", "LPT1"];
		for name in reserved.iter() {
			let path = PathBuf::from(name);
			// Creating these files should fail
			let result = std::fs::File::create(&path);
			assert!(
				result.is_err(),
				"Should not be able to create reserved name: {}",
				name
			);
		}
		// Trailing dot/space
		let path = PathBuf::from("trailingdot.txt.");
		let file = std::fs::File::create(&path);
		assert!(
			file.is_ok(),
			"Should be able to create file with trailing dot"
		);
		let meta = std::fs::metadata("trailingdot.txt");
		assert!(
			meta.is_ok(),
			"Windows should treat trailing dot as equivalent"
		);
	}
}

/// Test symlink and junction point handling
#[cfg(test)]
mod symlink_tests {
	use super::*;

	#[test]
	#[cfg(unix)]
	fn test_symlink_handling_unix() {
		use std::os::unix::fs::symlink;

		let temp_dir = TempDir::new().expect("Failed to create temp dir");

		// Create a regular file
		let target_file = temp_dir.path().join("target.txt");
		fs::write(&target_file, "target content").unwrap();

		// Create a symlink to it
		let symlink_path = temp_dir.path().join("link.txt");
		symlink(&target_file, &symlink_path).unwrap();

		// Create a directory and symlink to it
		let target_dir = temp_dir.path().join("target_dir");
		fs::create_dir(&target_dir).unwrap();
		fs::write(target_dir.join("inner.txt"), "inner content").unwrap();

		let dir_link = temp_dir.path().join("dir_link");
		symlink(&target_dir, &dir_link).unwrap();

		// Scan and verify symlink handling
		let nodes = scan_directory_tree(temp_dir.path()).expect("Failed to scan with symlinks");

		// Should find the original files/dirs
		let target_found = nodes.iter().any(|n| n.path == target_file);
		let target_dir_found = nodes.iter().any(|n| n.path == target_dir && n.is_directory);

		assert!(target_found, "Target file not found");
		assert!(target_dir_found, "Target directory not found");

		// Behavior with symlinks is implementation-dependent
		// walkdir by default follows symlinks, so we might see duplicates
		println!("Found {} nodes with symlinks", nodes.len());
	}

	#[test]
	#[cfg(windows)]
	fn test_junction_point_handling_windows() {
		// Junction points are Windows-specific
		// This test would require elevated privileges to create junction points
		// For now, we'll just test that scanning doesn't crash on existing ones

		let temp_dir = TempDir::new().expect("Failed to create temp dir");

		// Create a simple structure
		fs::create_dir(temp_dir.path().join("normal_dir")).unwrap();
		fs::write(temp_dir.path().join("normal_file.txt"), "content").unwrap();

		// Scanning should work even if there are no junction points
		let nodes = scan_directory_tree(temp_dir.path()).expect("Failed to scan");
		assert!(nodes.len() >= 2); // At least the dir and file we created
	}
}

/// Test cross-platform path conversion utilities
#[cfg(test)]
mod cross_platform {
	use super::*;

	#[test]
	fn test_path_separator_normalization() {
		// Test that we can handle both types of separators
		let mixed_paths = vec![
			"path/with/forward/slashes",
			r"path\with\back\slashes",
			r"path/with\mixed/slashes",
		];

		for path_str in mixed_paths {
			let path = PathBuf::from(path_str);

			// Should be able to get components regardless of separator style
			let components: Vec<_> = path.components().collect();
			assert!(components.len() >= 4);

			// Converting to string should use platform-appropriate separators
			let normalized = path.to_string_lossy();
			assert!(!normalized.is_empty());
		}
	}

	#[test]
	fn test_absolute_path_detection() {
		let test_cases = vec![
			("/absolute/unix/path", true),
			("relative/unix/path", false),
			("./relative/path", false),
			("../relative/path", false),
		];

		#[cfg(windows)]
		let windows_cases = vec![
			(r"C:\absolute\windows\path", true),
			(r"\\unc\path", true),
			(r"relative\windows\path", false),
			(r".\relative\path", false),
			(r"..\relative\path", false),
		];

		for (path_str, should_be_absolute) in test_cases {
			let path = PathBuf::from(path_str);
			assert_eq!(
				path.is_absolute(),
				should_be_absolute,
				"Path: {} expected absolute: {}",
				path_str,
				should_be_absolute
			);
		}

		#[cfg(windows)]
		for (path_str, should_be_absolute) in windows_cases {
			let path = PathBuf::from(path_str);
			assert_eq!(
				path.is_absolute(),
				should_be_absolute,
				"Windows path: {} expected absolute: {}",
				path_str,
				should_be_absolute
			);
		}
	}
}
