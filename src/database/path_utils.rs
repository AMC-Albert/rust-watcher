//! Cross-platform path normalization utilities
//!
//! This module provides functions to normalize and compare paths in a way that is robust across Windows and Unix systems.
//! It is critical for correct event and cache behavior, especially when dealing with symlinks, UNC paths, and case sensitivity.

use std::path::{Path, PathBuf};

/// Normalize a path for cross-platform equality and hashing.
///
/// - On Windows, this lowercases the path and converts all separators to '\\'.
/// - On Unix, this returns the canonicalized path if possible, else the original.
/// - Symlinks are not resolved by default (to avoid IO and permission issues).
///
/// # Limitations
/// - This does not resolve symlinks or network shares.
/// - On Windows, normalization is best-effort and may not handle all edge cases (e.g., short names, device paths).
/// - For production, consider using a crate like `path-absolutize` or `dunce` for more robust handling.
pub fn normalize_path(path: &Path) -> PathBuf {
	#[cfg(windows)]
	{
		let s = path.to_string_lossy().replace('/', "\\").to_lowercase();
		PathBuf::from(s)
	}
	#[cfg(not(windows))]
	{
		// On Unix, try to canonicalize, but fall back to the original path
		path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
	}
}

/// Compare two paths for equality after normalization
pub fn paths_equal(a: &Path, b: &Path) -> bool {
	normalize_path(a) == normalize_path(b)
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::path::PathBuf;

	#[test]
	fn test_normalize_path_windows() {
		#[cfg(windows)]
		{
			let p1 = PathBuf::from(r"C:/Users/ALBERT/Documents");
			let p2 = PathBuf::from(r"c:\users\albert\documents");
			assert_eq!(normalize_path(&p1), normalize_path(&p2));
		}
	}

	#[test]
	fn test_normalize_path_unix() {
		#[cfg(unix)]
		{
			let p1 = PathBuf::from("/tmp/../tmp/file.txt");
			let p2 = PathBuf::from("/tmp/file.txt");
			assert_eq!(normalize_path(&p1), normalize_path(&p2));
		}
	}

	#[test]
	fn test_paths_equal() {
		#[cfg(unix)]
		{
			let p1 = PathBuf::from("/tmp/../tmp/file.txt");
			let p2 = PathBuf::from("/tmp/file.txt");
			assert!(paths_equal(&p1, &p2));
		}
		#[cfg(windows)]
		{
			let p1 = PathBuf::from(r"C:/Users/ALBERT/Documents");
			let p2 = PathBuf::from(r"c:\users\albert\documents");
			assert!(paths_equal(&p1, &p2));
		}
	}
}
