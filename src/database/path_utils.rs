//! Cross-platform path normalization utilities
//!
//! This module provides functions to normalize and compare paths in a way that is robust across Windows and Unix systems.
//! It is critical for correct event and cache behavior, especially when dealing with symlinks, UNC paths, and case sensitivity.

use std::path::{Path, PathBuf};

/// Normalize a path for cross-platform equality and hashing.
///
/// - On Windows, this lowercases the path, converts all separators to '\\',
///   and normalizes UNC and device paths. Short (8.3) names are not expanded.
/// - On Unix, this returns the canonicalized path if possible, else the original.
/// - Trailing slashes and redundant separators are removed.
/// - Symlinks are not resolved by default (to avoid IO and permission issues).
///
/// # Limitations
/// - Does not resolve symlinks or expand short (8.3) names on Windows.
/// - UNC and device paths are normalized to a canonical form, but not validated.
/// - On macOS, case-insensitivity is not handled.
/// - For production, consider using a crate like `dunce` or `path-absolutize` for more robust handling.
pub fn normalize_path(path: &Path) -> PathBuf {
	#[cfg(windows)]
	{
		use std::path::Component;
		let mut s = path.to_string_lossy().replace('/', "\\").to_lowercase();

		// Handle device paths: \\?\UNC\server\share, \\server\share, \\.\C:\...
		const UNC_PREFIX: &str = "\\?\\UNC\\";
		const DEVICE_PREFIX: &str = "\\.\\";
		let mut norm = PathBuf::new();

		if s.starts_with(UNC_PREFIX) {
			// Convert \\? to UNC (e.g. \\? UNC server share -> \\ server share)
			s = format!("\\{}", &s[8..]);
		} else if s.starts_with(DEVICE_PREFIX) && s.len() > 4 && s.chars().nth(4) == Some(':') {
			// Convert device path (e.g. \\. C: path -> C: path)
			s = s[4..].to_string();
		}

		// Remove leading backslashes for UNC normalization, but preserve for root
		let mut leading_slashes = 0;
		for c in s.chars() {
			if c == '\\' {
				leading_slashes += 1;
			} else {
				break;
			}
		}
		if leading_slashes > 2 {
			s = s[(leading_slashes - 2)..].to_string(); // Reduce to two for UNC
		}

		let p = Path::new(&s);
		let components = p.components().peekable();
		let mut saw_prefix = false;
		for comp in components {
			match comp {
				Component::Prefix(prefix) => {
					saw_prefix = true;
					norm.push(prefix.as_os_str());
				}
				Component::RootDir => {
					if saw_prefix || norm.as_os_str().is_empty() {
						norm.push(Component::RootDir);
					}
				}
				Component::Normal(c) => norm.push(c),
				_ => {} // Ignore CurDir, ParentDir
			}
		}
		// Remove trailing separators
		while norm.as_os_str().to_string_lossy().ends_with("\\") && norm.parent().is_some() {
			norm.pop();
		}
		// UNC normalization: ensure UNC root is preserved (e.g. \\ server share)
		if s.starts_with("\\\\") && !norm.as_os_str().to_string_lossy().starts_with("\\\\") {
			let mut unc = PathBuf::from("\\\\");
			unc.push(norm);
			norm = unc;
		}
		// Defensive: never return empty path
		if norm.as_os_str().is_empty() {
			return PathBuf::from(".");
		}
		norm
	}
	#[cfg(not(windows))]
	{
		let mut norm = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
		// Remove trailing slashes
		while norm.as_os_str().to_string_lossy().ends_with('/') && norm.parent().is_some() {
			norm.pop();
		}
		norm
	}
}

pub fn paths_equal(a: &Path, b: &Path) -> bool {
	normalize_path(a) == normalize_path(b)
}
