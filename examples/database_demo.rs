//! Example demonstrating database functionality
//!
//! This example shows how to use the database adapter for persistent storage
//! of filesystem events and metadata.

use rust_watcher::database::{DatabaseAdapter, DatabaseConfig};
use rust_watcher::{EventType, FileSystemEvent};
use tempfile::TempDir;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	println!("Database Feature Demo");
	println!("====================");

	// Create a temporary directory for our database
	let temp_dir = TempDir::new()?;
	let db_path = temp_dir.path().join("demo.db");

	println!("Database path: {:?}", db_path);
	// Configure the database for small-scale monitoring
	let db_config = DatabaseConfig::with_path(db_path.clone());

	println!("Creating database adapter...");
	let adapter = DatabaseAdapter::new(db_config).await?;

	if adapter.is_enabled() {
		println!("✓ Database adapter created successfully");
		println!("✓ Database file: {:?}", adapter.database_path());
	} else {
		println!("✗ Database adapter is disabled");
		return Ok(());
	}

	// Create some sample events
	println!("\nStoring sample events...");

	let events = vec![
		FileSystemEvent {
			id: Uuid::new_v4(),
			event_type: EventType::Create,
			path: temp_dir.path().join("file1.txt"),
			timestamp: chrono::Utc::now(),
			is_directory: false,
			size: Some(1024),
			move_data: None,
		},
		FileSystemEvent {
			id: Uuid::new_v4(),
			event_type: EventType::Write,
			path: temp_dir.path().join("file2.txt"),
			timestamp: chrono::Utc::now(),
			is_directory: false,
			size: Some(2048),
			move_data: None,
		},
		FileSystemEvent {
			id: Uuid::new_v4(),
			event_type: EventType::Remove,
			path: temp_dir.path().join("file3.txt"),
			timestamp: chrono::Utc::now(),
			is_directory: false,
			size: None,
			move_data: None,
		},
	];

	for (i, event) in events.iter().enumerate() {
		adapter.store_event(event).await?;
		println!(
			"✓ Stored event {}: {:?} on {:?}",
			i + 1,
			event.event_type,
			event.path.file_name()
		);
	}
	// Query events back
	println!("\nQuerying stored events...");

	for event in &events {
		println!("Querying for path: {:?}", event.path);
		let retrieved = adapter.get_events_for_path(&event.path).await?;
		println!(
			"📄 Path: {:?} - Found {} events",
			event.path.file_name(),
			retrieved.len()
		);

		for record in retrieved {
			println!(
				"   - {} at {}",
				record.event_type,
				record.timestamp.format("%H:%M:%S")
			);
		}
	}

	// Query by size range
	println!("\nQuerying events by size (1000-2500 bytes)...");
	let size_events = adapter.find_events_by_size(1000, 2500).await?;
	println!("Found {} events in size range", size_events.len());

	for record in size_events {
		println!(
			"   - {} on {:?} ({}b)",
			record.event_type,
			record.path.file_name(),
			record.size.unwrap_or(0)
		);
	}

	// Query by time range
	println!("\nQuerying recent events (last hour)...");
	let now = chrono::Utc::now();
	let hour_ago = now - chrono::Duration::hours(1);
	let recent_events = adapter.find_events_by_time_range(hour_ago, now).await?;
	println!("Found {} recent events", recent_events.len());

	// Get database statistics
	println!("\nDatabase Statistics:");
	let stats = adapter.get_stats().await?;
	println!("   - Total events: {}", stats.total_events);
	println!("   - Total metadata: {}", stats.total_metadata);

	// Health check
	println!("\nHealth Check:");
	let healthy = adapter.health_check().await?;
	println!(
		"   - Database health: {}",
		if healthy {
			"✓ Healthy"
		} else {
			"✗ Unhealthy"
		}
	);

	// Cleanup demo
	println!("\nCleaning up old events...");
	let cleaned = adapter.cleanup_old_events().await?;
	println!("   - Cleaned {} old events", cleaned);

	// Compact database
	println!("\nCompacting database...");
	adapter.compact().await?;
	println!("   - ✓ Database compacted");

	println!("\n🎉 Database demo completed successfully!");
	println!("Database file created: {:?}", db_path);
	println!("You can inspect it with sqlite3 or similar tools.");

	Ok(())
}
