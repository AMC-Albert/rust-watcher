// Integration tests for filesystem cache functionality
//
// These tests validate the correctness and performance of the filesystem cache layer:
// - Node insert and retrieval
// - Directory hierarchy queries
// - Batch insert
// - Prefix/subtree queries

#[path = "common/db_integration.rs"]
mod db_integration;
use db_integration::{create_and_store_node, setup_test_storage};
use rust_watcher::database::DatabaseStorage;

#[tokio::test]
async fn test_filesystem_node_insert_and_retrieve() {
	let (temp_dir, _db_path, mut storage, watch_id) =
		setup_test_storage("node_insert_and_retrieve").await;

	// Create a test node
	let node_path = temp_dir.path().join("foo.txt");
	let node = create_and_store_node(&mut storage, &watch_id, &node_path, "test").await;

	// Store and retrieve
	let retrieved = storage.get_filesystem_node(&watch_id, &node_path).await.expect("get");
	assert!(retrieved.is_some());
	let retrieved = retrieved.unwrap();
	assert_eq!(retrieved.path, node.path);
	assert_eq!(retrieved.node_type, node.node_type);
}

#[tokio::test]
async fn test_filesystem_hierarchy_and_list_directory() {
	let (temp_dir, _db_path, mut storage, watch_id) =
		setup_test_storage("hierarchy_and_list_directory").await;

	// Create parent and child nodes
	let parent_path = temp_dir.path().join("parent");
	let child_path = parent_path.join("child.txt");
	std::fs::create_dir(&parent_path).unwrap();
	let _parent_node = create_and_store_node(&mut storage, &watch_id, &parent_path, "test").await;
	let _child_node = create_and_store_node(&mut storage, &watch_id, &child_path, "test").await;

	// List directory
	let children = storage.list_directory_for_watch(&watch_id, &parent_path).await.expect("list");
	assert_eq!(children.len(), 1);
	assert_eq!(children[0].path, child_path);
}

#[tokio::test]
async fn test_get_node_api() {
	let (temp_dir, _db_path, mut storage, watch_id) = setup_test_storage("get_node_api").await;

	// Create and store a node
	let node_path = temp_dir.path().join("bar.txt");
	let node = create_and_store_node(&mut storage, &watch_id, &node_path, "test").await;

	// get_node should return the same as get_filesystem_node
	let retrieved = storage.get_node(&watch_id, &node_path).await.expect("get_node");
	assert!(retrieved.is_some());
	let retrieved = retrieved.unwrap();
	assert_eq!(retrieved.path, node.path);
	assert_eq!(retrieved.node_type, node.node_type);

	// get_node for a missing node should return None
	let missing_path = temp_dir.path().join("does_not_exist.txt");
	let missing = storage.get_node(&watch_id, &missing_path).await.expect("get_node missing");
	assert!(missing.is_none());
}

#[tokio::test]
async fn test_search_nodes_glob_patterns() {
	let (temp_dir, _db_path, mut storage, watch_id) =
		setup_test_storage("search_nodes_glob_patterns").await;

	// Create a set of files and directories
	let files = [
		temp_dir.path().join("alpha.txt"),
		temp_dir.path().join("beta.log"),
		temp_dir.path().join("gamma.txt"),
		temp_dir.path().join("delta.md"),
	];
	for file in &files {
		let _ = create_and_store_node(&mut storage, &watch_id, file, "test").await;
	}

	// Simple glob: *.txt
	let results = storage.search_nodes("*.txt").await.expect("search_nodes");
	let result_paths: Vec<_> = results
		.iter()
		.map(|n| n.path.file_name().unwrap().to_string_lossy().to_string())
		.collect();
	assert!(result_paths.contains(&"alpha.txt".to_string()));
	assert!(result_paths.contains(&"gamma.txt".to_string()));
	assert!(!result_paths.contains(&"beta.log".to_string()));
	assert!(!result_paths.contains(&"delta.md".to_string()));

	// Glob: *a.*
	let results = storage.search_nodes("*a.*").await.expect("search_nodes");
	let result_paths: Vec<_> = results
		.iter()
		.map(|n| n.path.file_name().unwrap().to_string_lossy().to_string())
		.collect();
	assert!(result_paths.contains(&"alpha.txt".to_string()));
	assert!(result_paths.contains(&"gamma.txt".to_string()));
	assert!(result_paths.contains(&"delta.md".to_string()));
	assert!(result_paths.contains(&"beta.log".to_string())); // Acceptable per glob semantics

	// No matches
	let results = storage.search_nodes("*.doesnotexist").await.expect("search_nodes");
	assert!(results.is_empty());
}

#[tokio::test]
async fn test_stats_and_metadata_event_types() {
	let (temp_dir, _db_path, mut storage, watch_id) =
		setup_test_storage("stats_and_metadata_event_types").await;

	// Create and store a node with a metadata event type
	let node_path = temp_dir.path().join("meta.txt");
	let _node = create_and_store_node(&mut storage, &watch_id, &node_path, "metadata").await;

	// Check that the node is present
	let retrieved = storage.get_filesystem_node(&watch_id, &node_path).await.expect("get");
	assert!(retrieved.is_some());

	// Remove the node with a metadata event type
	storage
		.remove_filesystem_node(&watch_id, &node_path, "metadata")
		.await
		.expect("remove");
	let missing = storage
		.get_filesystem_node(&watch_id, &node_path)
		.await
		.expect("get after remove");
	assert!(missing.is_none());
}

#[tokio::test]
async fn test_repair_stats_counters_stub() {
	let (_temp_dir, _db_path, mut storage, _watch_id) =
		setup_test_storage("repair_stats_counters_stub").await;
	// This should not panic or error, but will always return 0 for now
	let repaired = storage.repair_stats_counters(None, None).await.expect("repair_stats_counters");
	assert_eq!(repaired, 0);
}

#[tokio::test]
async fn test_hierarchy_ancestor_descendant_traversal() {
	use std::fs;
	let (temp_dir, _db_path, mut storage, watch_id) =
		setup_test_storage("hierarchy_ancestor_descendant_traversal").await;

	// Create a deep directory tree: root/level1/level2/leaf.txt
	let root = temp_dir.path().join("root");
	let level1 = root.join("level1");
	let level2 = level1.join("level2");
	let leaf = level2.join("leaf.txt");
	fs::create_dir(&root).unwrap();
	fs::create_dir(&level1).unwrap();
	fs::create_dir(&level2).unwrap();
	fs::write(&leaf, b"leaf").unwrap();

	// Canonicalize all paths before node creation
	let root = fs::canonicalize(&root).unwrap();
	let level1 = fs::canonicalize(&level1).unwrap();
	let level2 = fs::canonicalize(&level2).unwrap();
	let leaf = fs::canonicalize(&leaf).unwrap();

	let _root_node = create_and_store_node(&mut storage, &watch_id, &root, "test").await;
	let _level1_node = create_and_store_node(&mut storage, &watch_id, &level1, "test").await;
	let _level2_node = create_and_store_node(&mut storage, &watch_id, &level2, "test").await;
	let _leaf_node = create_and_store_node(&mut storage, &watch_id, &leaf, "test").await;

	// Test ancestor traversal from leaf
	let ancestors = storage.list_ancestors_modular(&leaf).await.expect("list_ancestors_modular");
	let ancestor_paths: Vec<_> = ancestors.iter().map(|n| n.path.clone()).collect();
	assert!(ancestor_paths.contains(&level2));
	assert!(ancestor_paths.contains(&level1));
	assert!(ancestor_paths.contains(&root));

	// Test descendant traversal from root
	let descendants =
		storage.list_descendants_modular(&root).await.expect("list_descendants_modular");
	let descendant_paths: Vec<_> = descendants.iter().map(|n| n.path.clone()).collect();
	assert!(descendant_paths.contains(&level1));
	assert!(descendant_paths.contains(&level2));
	assert!(descendant_paths.contains(&leaf));
	// Should not include root itself
	assert!(!descendant_paths.contains(&root));
}
