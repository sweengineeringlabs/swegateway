//! End-to-end tests for FileOutbound sub-trait.
//!
//! Exercises FileOutbound write operations through the SAF factory.
//! Results are verified using FileInbound (read, exists, metadata) on the
//! same combined gateway instance.

use swe_gateway::prelude::*;
use swe_gateway::saf::file::UploadOptions;
use swe_gateway::saf;

#[tokio::test]
async fn e2e_file_outbound_write_and_delete() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    // impl FileGateway implements both FileInbound and FileOutbound
    let gw = saf::local_file_gateway(temp_dir.path());

    // --- FileOutbound: write ---
    let content = b"Hello, outbound world!".to_vec();
    let info = gw
        .write("hello.txt", content.clone(), UploadOptions::overwrite())
        .await
        .unwrap();
    assert_eq!(info.path, "hello.txt");
    assert_eq!(info.size, content.len() as u64);

    // Verify via FileInbound
    let read_back = gw.read("hello.txt").await.unwrap();
    assert_eq!(read_back, content);
    assert!(gw.exists("hello.txt").await.unwrap());

    // Overwrite with new content
    let new_content = b"Updated content".to_vec();
    let updated_info = gw
        .write("hello.txt", new_content.clone(), UploadOptions::overwrite())
        .await
        .unwrap();
    assert_eq!(updated_info.size, new_content.len() as u64);
    assert_eq!(gw.read("hello.txt").await.unwrap(), new_content);

    // --- FileOutbound: delete ---
    gw.delete("hello.txt").await.unwrap();
    assert!(!gw.exists("hello.txt").await.unwrap());

    // Deleting non-existent file should return error
    let del_err = gw.delete("never_existed.txt").await;
    assert!(del_err.is_err());
}

#[tokio::test]
async fn e2e_file_outbound_copy_and_rename() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let gw = saf::local_file_gateway(temp_dir.path());

    // Seed a file
    let original = b"Original document content".to_vec();
    gw.write("original.txt", original.clone(), UploadOptions::overwrite())
        .await
        .unwrap();

    // --- FileOutbound: copy ---
    let copy_info = gw.copy("original.txt", "backup.txt").await.unwrap();
    assert_eq!(copy_info.path, "backup.txt");
    assert_eq!(copy_info.size, original.len() as u64);

    // Both must exist with identical content
    assert!(gw.exists("original.txt").await.unwrap());
    assert!(gw.exists("backup.txt").await.unwrap());
    assert_eq!(gw.read("backup.txt").await.unwrap(), original);

    // --- FileOutbound: rename ---
    let renamed_info = gw.rename("original.txt", "final.txt").await.unwrap();
    assert_eq!(renamed_info.path, "final.txt");

    // Source gone, destination present
    assert!(!gw.exists("original.txt").await.unwrap());
    assert!(gw.exists("final.txt").await.unwrap());
    assert_eq!(gw.read("final.txt").await.unwrap(), original);
}

#[tokio::test]
async fn e2e_file_outbound_create_directory_and_nested_writes() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let gw = saf::local_file_gateway(temp_dir.path());

    // --- FileOutbound: create_directory ---
    gw.create_directory("uploads").await.unwrap();
    assert!(gw.exists("uploads").await.unwrap());

    let dir_meta = gw.metadata("uploads").await.unwrap();
    assert!(dir_meta.is_directory);

    // Write nested files inside the directory
    gw.write(
        "uploads/image.png",
        b"\x89PNG fake image data".to_vec(),
        UploadOptions::overwrite(),
    )
    .await
    .unwrap();
    gw.write(
        "uploads/doc.pdf",
        b"%PDF fake pdf data".to_vec(),
        UploadOptions::overwrite(),
    )
    .await
    .unwrap();

    assert!(gw.exists("uploads/image.png").await.unwrap());
    assert!(gw.exists("uploads/doc.pdf").await.unwrap());

    let img_meta = gw.metadata("uploads/image.png").await.unwrap();
    assert!(!img_meta.is_directory);

    // --- FileOutbound: delete_directory (recursive) ---
    gw.delete_directory("uploads", true).await.unwrap();
    assert!(!gw.exists("uploads/image.png").await.unwrap());
    assert!(!gw.exists("uploads/doc.pdf").await.unwrap());
}
