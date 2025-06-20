//! Test infrastructure for filesystem cache validation
//!
//! Provides utilities to create known directory structures for testing
//! filesystem cache implementations.

use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// A test directory structure with known properties for validation
#[derive(Debug)]
pub struct TestDirectoryStructure {
    pub temp_dir: TempDir,
    pub metadata: DirectoryMetadata,
}

/// Metadata about a test directory structure
#[derive(Debug, Clone)]
pub struct DirectoryMetadata {
    pub total_files: usize,
    pub total_directories: usize,
    pub max_depth: usize,
    pub total_size: u64,
    pub file_types: Vec<String>,
}

impl TestDirectoryStructure {
    /// Create a small test structure (suitable for unit tests)
    pub fn create_small() -> Result<Self, std::io::Error> {
        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();
        
        // Create simple structure:
        // root/
        //   file1.txt
        //   file2.log
        //   subdir/
        //     subfile.txt
        //     deeper/
        //       deep_file.dat
        
        fs::write(root.join("file1.txt"), "content1")?;
        fs::write(root.join("file2.log"), "log content")?;
        
        let subdir = root.join("subdir");
        fs::create_dir(&subdir)?;
        fs::write(subdir.join("subfile.txt"), "sub content")?;
        
        let deeper = subdir.join("deeper");
        fs::create_dir(&deeper)?;
        fs::write(deeper.join("deep_file.dat"), "deep content")?;
        
        let metadata = DirectoryMetadata {
            total_files: 4,
            total_directories: 3, // root, subdir, deeper
            max_depth: 2,         // root=0, subdir=1, deeper=2
            total_size: 8 + 11 + 11 + 12, // content lengths
            file_types: vec!["txt".to_string(), "log".to_string(), "dat".to_string()],
        };
        
        Ok(TestDirectoryStructure { temp_dir, metadata })
    }

    /// Create a medium test structure (good for integration tests)
    pub fn create_medium() -> Result<Self, std::io::Error> {
        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();
        
        let mut total_files = 0;
        let mut total_directories = 1; // root
        let mut total_size = 0u64;
        
        // Create multiple directories with various file types
        let subdirs = ["docs", "images", "code", "data"];
        
        for subdir_name in &subdirs {
            let subdir = root.join(subdir_name);
            fs::create_dir(&subdir)?;
            total_directories += 1;
            
            // Create files in each subdirectory
            for i in 0..10 {
                let filename = match *subdir_name {
                    "docs" => format!("document_{}.txt", i),
                    "images" => format!("image_{}.jpg", i),
                    "code" => format!("source_{}.rs", i),
                    "data" => format!("data_{}.json", i),
                    _ => format!("file_{}.dat", i),
                };
                
                let content = format!("Content for {} file {}", subdir_name, i);
                let file_path = subdir.join(&filename);
                fs::write(&file_path, &content)?;
                
                total_files += 1;
                total_size += content.len() as u64;
            }
        }
        
        let metadata = DirectoryMetadata {
            total_files,
            total_directories,
            max_depth: 1,
            total_size,
            file_types: vec!["txt".to_string(), "jpg".to_string(), "rs".to_string(), "json".to_string()],
        };
        
        Ok(TestDirectoryStructure { temp_dir, metadata })
    }

    /// Create a large test structure (for performance testing)
    pub fn create_large() -> Result<Self, std::io::Error> {
        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();
        
        let mut total_files = 0;
        let mut total_directories = 1; // root
        let mut total_size = 0u64;
        
        // Create a deep hierarchy
        for level1 in 0..20 {
            let level1_dir = root.join(format!("level1_{:02}", level1));
            fs::create_dir(&level1_dir)?;
            total_directories += 1;
            
            for level2 in 0..10 {
                let level2_dir = level1_dir.join(format!("level2_{:02}", level2));
                fs::create_dir(&level2_dir)?;
                total_directories += 1;
                
                // Add files at level2
                for file_num in 0..5 {
                    let filename = format!("file_{}_{}.txt", level2, file_num);
                    let content = format!("Content for level1={} level2={} file={}", level1, level2, file_num);
                    let file_path = level2_dir.join(&filename);
                    fs::write(&file_path, &content)?;
                    
                    total_files += 1;
                    total_size += content.len() as u64;
                }
            }
        }
        
        let metadata = DirectoryMetadata {
            total_files,
            total_directories,
            max_depth: 2,
            total_size,
            file_types: vec!["txt".to_string()],
        };
        
        Ok(TestDirectoryStructure { temp_dir, metadata })
    }

    /// Create a structure with specific edge cases for testing
    pub fn create_edge_cases() -> Result<Self, std::io::Error> {
        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();
        
        // Empty directory
        fs::create_dir(root.join("empty_dir"))?;
        
        // Directory with spaces and special characters
        let special_dir = root.join("dir with spaces & special chars");
        fs::create_dir(&special_dir)?;
        fs::write(special_dir.join("file in special dir.txt"), "content")?;
        
        // Unicode directory and files
        let unicode_dir = root.join("üñíçødé_dir");
        fs::create_dir(&unicode_dir)?;
        fs::write(unicode_dir.join("файл.txt"), "unicode content")?;
        fs::write(unicode_dir.join("🚀rocket.txt"), "emoji content")?;
        
        // Very long filename (but within reasonable limits)
        let long_name = "very_long_filename_that_tests_path_length_handling_but_stays_within_reasonable_limits.txt";
        fs::write(root.join(long_name), "long filename content")?;
        
        // Various file sizes
        fs::write(root.join("empty_file.txt"), "")?;
        fs::write(root.join("small_file.txt"), "small")?;
        fs::write(root.join("medium_file.txt"), "medium content ".repeat(100))?;
        fs::write(root.join("large_file.txt"), "large content ".repeat(1000))?;
        
        let metadata = DirectoryMetadata {
            total_files: 7, // Including files in subdirs
            total_directories: 4, // root + 3 subdirs
            max_depth: 1,
            total_size: 0 + 5 + 1400 + 14000 + 13 + 14 + 14, // Approximate
            file_types: vec!["txt".to_string()],
        };
        
        Ok(TestDirectoryStructure { temp_dir, metadata })
    }

    /// Get the root path of the test structure
    pub fn root(&self) -> &Path {
        self.temp_dir.path()
    }

    /// Validate that the filesystem cache correctly represents this structure
    pub fn validate_cache_representation<F>(&self, cache_query: F) -> bool 
    where
        F: Fn(&Path) -> (usize, usize, usize), // Returns (files, dirs, max_depth)
    {
        let (found_files, found_dirs, found_depth) = cache_query(self.root());
        
        found_files == self.metadata.total_files &&
        found_dirs == self.metadata.total_directories &&
        found_depth == self.metadata.max_depth
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem_poc::{scan_directory_tree, scan_statistics};

    #[test]
    fn test_small_structure_creation() {
        let structure = TestDirectoryStructure::create_small().expect("Failed to create small structure");
        
        // Verify the structure was created correctly
        assert!(structure.root().join("file1.txt").exists());
        assert!(structure.root().join("subdir").is_dir());
        assert!(structure.root().join("subdir").join("deeper").join("deep_file.dat").exists());
        
        // Validate using our filesystem scanning
        let nodes = scan_directory_tree(structure.root()).expect("Failed to scan");
        let stats = scan_statistics(&nodes);
        
        assert_eq!(stats.file_count, structure.metadata.total_files);
        assert_eq!(stats.directory_count, structure.metadata.total_directories);
        assert_eq!(stats.max_depth, structure.metadata.max_depth);
    }

    #[test]
    fn test_medium_structure_creation() {
        let structure = TestDirectoryStructure::create_medium().expect("Failed to create medium structure");
        
        // Verify some key files exist
        assert!(structure.root().join("docs").join("document_0.txt").exists());
        assert!(structure.root().join("images").join("image_5.jpg").exists());
        assert!(structure.root().join("code").join("source_9.rs").exists());
        
        // Validate using our filesystem scanning
        let nodes = scan_directory_tree(structure.root()).expect("Failed to scan");
        let stats = scan_statistics(&nodes);
        
        // Should match our expected metadata
        assert_eq!(stats.file_count, structure.metadata.total_files);
        assert_eq!(stats.directory_count, structure.metadata.total_directories);
    }

    #[test]
    fn test_edge_cases_structure() {
        let structure = TestDirectoryStructure::create_edge_cases().expect("Failed to create edge cases");
        
        // Verify edge case files exist
        assert!(structure.root().join("empty_dir").is_dir());
        assert!(structure.root().join("dir with spaces & special chars").is_dir());
        assert!(structure.root().join("üñíçødé_dir").join("файл.txt").exists());
        assert!(structure.root().join("empty_file.txt").exists());
        
        // Should be able to scan without errors
        let nodes = scan_directory_tree(structure.root()).expect("Failed to scan edge cases");
        assert!(nodes.len() > 0);
    }

    #[test]
    fn test_cache_validation_function() {
        let structure = TestDirectoryStructure::create_small().expect("Failed to create structure");
        
        // Mock cache query function that uses our filesystem scanning
        let cache_query = |root: &Path| {
            let nodes = scan_directory_tree(root).expect("Failed to scan");
            let stats = scan_statistics(&nodes);
            (stats.file_count, stats.directory_count, stats.max_depth)
        };
        
        // Validation should pass
        assert!(structure.validate_cache_representation(cache_query));
    }
}
