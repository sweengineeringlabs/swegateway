//! End-to-end tests for FileInbound sub-trait.
//!
//! Exercises only the FileInbound read operations through the SAF factory.
//! Files are first written using FileOutbound (via the combined gateway),
//! then only FileInbound methods are exercised.

use swe_gateway::prelude::*;
use swe_gateway::saf::file::{ListOptions, UploadOptions};
use swe_gateway::saf;

#[tokio::test]
async fn e2e_file_inbound_read_exists_metadata() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    // The SAF builder returns impl FileGateway which implements both sub-traits.
    let gw = saf::local_file_gateway(temp_dir.path());

    // Seed data via FileOutbound
    let content = b"End-to-end file inbound test content".to_vec();
    gw.write("report.txt", content.clone(), UploadOptions::overwrite())
        .await
        .unwrap();
    gw.write("data.json", b"{\"key\": \"value\"}".to_vec(), UploadOptions::overwrite())
        .await
        .unwrap();

    // --- FileInbound: read ---
    let read_back = gw.read("report.txt").await.unwrap();
    assert_eq!(read_back, content);

    let json_back = gw.read("data.json").await.unwrap();
    assert_eq!(json_back, b"{\"key\": \"value\"}");

    // --- FileInbound: exists ---
    assert!(gw.exists("report.txt").await.unwrap());
    assert!(gw.exists("data.json").await.unwrap());
    assert!(!gw.exists("nonexistent.txt").await.unwrap());

    // --- FileInbound: metadata ---
    let meta = gw.metadata("report.txt").await.unwrap();
    assert_eq!(meta.path, "report.txt");
    assert_eq!(meta.size, content.len() as u64);
    assert!(!meta.is_directory);
}

#[tokio::test]
async fn e2e_file_inbound_list_and_health_check() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let gw = saf::local_file_gateway(temp_dir.path());

    // Seed a directory structure via FileOutbound
    gw.create_directory("archive").await.unwrap();
    gw.write("archive/jan.log", b"January logs".to_vec(), UploadOptions::overwrite())
        .await
        .unwrap();
    gw.write("archive/feb.log", b"February logs".to_vec(), UploadOptions::overwrite())
        .await
        .unwrap();
    gw.write("readme.txt", b"Top level readme".to_vec(), UploadOptions::overwrite())
        .await
        .unwrap();

    // --- FileInbound: list (default, root) ---
    let root_listing = gw.list(ListOptions::default()).await.unwrap();
    let total_entries = root_listing.files.len() + root_listing.prefixes.len();
    assert!(
        total_entries >= 1,
        "Root listing should contain at least the archive directory or readme"
    );

    // --- FileInbound: list with prefix filter ---
    let archive_listing = gw
        .list(ListOptions::with_prefix("archive"))
        .await
        .unwrap();
    assert!(
        !archive_listing.files.is_empty() || !archive_listing.prefixes.is_empty(),
        "Archive prefix listing should find entries"
    );

    // Verify the nested files are accessible via exists (FileInbound)
    assert!(gw.exists("archive/jan.log").await.unwrap());
    assert!(gw.exists("archive/feb.log").await.unwrap());

    // --- FileInbound: health_check ---
    let health = gw.health_check().await.unwrap();
    assert_eq!(health.status, HealthStatus::Healthy);
}

#[tokio::test]
async fn e2e_file_inbound_read_error_on_missing_and_metadata_directory() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let gw = saf::local_file_gateway(temp_dir.path());

    // Seed a directory via FileOutbound
    gw.create_directory("logs").await.unwrap();
    gw.write("logs/app.log", b"log content".to_vec(), UploadOptions::overwrite())
        .await
        .unwrap();

    // --- FileInbound: read on a missing path returns error ---
    let err = gw.read("does_not_exist.bin").await;
    assert!(err.is_err(), "Reading a non-existent file must return an error");

    // --- FileInbound: metadata on a directory ---
    let dir_meta = gw.metadata("logs").await.unwrap();
    assert!(dir_meta.is_directory, "Metadata for a directory should set is_directory=true");

    // --- FileInbound: exists on nested file ---
    assert!(gw.exists("logs/app.log").await.unwrap());
    assert!(!gw.exists("logs/missing.log").await.unwrap());
}
