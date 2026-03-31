//! End-to-end tests for FileGateway.
//!
//! Exercises the full lifecycle through the combined FileGateway trait:
//! write -> read -> copy -> rename -> list -> delete -> verify.

use swe_gateway::prelude::*;
use swe_gateway::saf::file::UploadOptions;
use swe_gateway::saf;

#[tokio::test]
async fn e2e_file_write_read_delete_lifecycle() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let gw = saf::local_file_gateway(temp_dir.path());

    // Write a file
    let content = b"Hello, gateway!".to_vec();
    let info = gw
        .write("hello.txt", content.clone(), UploadOptions::overwrite())
        .await
        .unwrap();
    assert_eq!(info.path, "hello.txt");
    assert_eq!(info.size, 15);

    // Read it back
    let data = gw.read("hello.txt").await.unwrap();
    assert_eq!(data, content);

    // Check existence
    assert!(gw.exists("hello.txt").await.unwrap());
    assert!(!gw.exists("missing.txt").await.unwrap());

    // Get metadata
    let meta = gw.metadata("hello.txt").await.unwrap();
    assert_eq!(meta.size, 15);
    assert!(!meta.is_directory);

    // Delete
    gw.delete("hello.txt").await.unwrap();
    assert!(!gw.exists("hello.txt").await.unwrap());
}

#[tokio::test]
async fn e2e_file_copy_and_rename() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let gw = saf::local_file_gateway(temp_dir.path());

    let content = b"original content".to_vec();
    gw.write("source.txt", content.clone(), UploadOptions::overwrite())
        .await
        .unwrap();

    // Copy
    let copy_info = gw.copy("source.txt", "copy.txt").await.unwrap();
    assert_eq!(copy_info.path, "copy.txt");

    // Both should exist
    assert!(gw.exists("source.txt").await.unwrap());
    assert!(gw.exists("copy.txt").await.unwrap());

    // Copy content should match
    let copy_data = gw.read("copy.txt").await.unwrap();
    assert_eq!(copy_data, content);

    // Rename source to new name
    let renamed = gw.rename("source.txt", "renamed.txt").await.unwrap();
    assert_eq!(renamed.path, "renamed.txt");

    // Source gone, renamed exists
    assert!(!gw.exists("source.txt").await.unwrap());
    assert!(gw.exists("renamed.txt").await.unwrap());

    let renamed_data = gw.read("renamed.txt").await.unwrap();
    assert_eq!(renamed_data, content);
}

#[tokio::test]
async fn e2e_file_directory_operations() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let gw = saf::local_file_gateway(temp_dir.path());

    // Create directory
    gw.create_directory("subdir").await.unwrap();
    assert!(gw.exists("subdir").await.unwrap());

    // Write a file inside
    gw.write(
        "subdir/nested.txt",
        b"nested content".to_vec(),
        UploadOptions::overwrite(),
    )
    .await
    .unwrap();

    assert!(gw.exists("subdir/nested.txt").await.unwrap());

    // List root directory
    let listing = gw
        .list(swe_gateway::saf::file::ListOptions::default())
        .await
        .unwrap();
    // Should see the subdir
    assert!(!listing.files.is_empty() || !listing.prefixes.is_empty());

    // Delete directory recursively
    gw.delete_directory("subdir", true).await.unwrap();
    assert!(!gw.exists("subdir/nested.txt").await.unwrap());
}

#[tokio::test]
async fn e2e_file_overwrite_protection() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let gw = saf::local_file_gateway(temp_dir.path());

    gw.write("protected.txt", b"v1".to_vec(), UploadOptions::overwrite())
        .await
        .unwrap();

    // Writing again with overwrite=false should fail
    let no_overwrite = UploadOptions {
        overwrite: false,
        ..Default::default()
    };
    let result = gw
        .write("protected.txt", b"v2".to_vec(), no_overwrite)
        .await;
    assert!(result.is_err());

    // Content should still be v1
    let data = gw.read("protected.txt").await.unwrap();
    assert_eq!(data, b"v1");
}

#[tokio::test]
async fn e2e_file_presigned_urls() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let gw = saf::local_file_gateway(temp_dir.path());

    // Presigned read URL
    let read_url = gw.presigned_read_url("test.txt", 3600).await.unwrap();
    assert!(read_url.url.starts_with("file://"));
    assert_eq!(read_url.method, "GET");

    // Presigned upload URL
    let upload_url = gw.presigned_upload_url("upload.txt", 3600).await.unwrap();
    assert!(upload_url.url.starts_with("file://"));
    assert_eq!(upload_url.method, "PUT");
}

#[tokio::test]
async fn e2e_file_health_check() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let gw = saf::local_file_gateway(temp_dir.path());

    let health = gw.health_check().await.unwrap();
    assert_eq!(health.status, HealthStatus::Healthy);
}

#[tokio::test]
async fn e2e_file_read_nonexistent_returns_error() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let gw = saf::local_file_gateway(temp_dir.path());

    let result = gw.read("does_not_exist.txt").await;
    assert!(result.is_err());
}
