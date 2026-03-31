//! Seed data functions for populating gateways in tests.

use swe_gateway::saf::file::UploadOptions;
use swe_gateway::saf::{DatabaseOutbound, FileOutbound};

use super::records;

/// Insert N numbered records into the given table.
///
/// Each record has fields: `id`, `name` ("item-{i}"), `payload`.
pub async fn insert_numbered_records(
    db: &(impl DatabaseOutbound + ?Sized),
    table: &str,
    count: usize,
    payload: &str,
) {
    for i in 0..count {
        db.insert(table, records::numbered_record(i, payload))
            .await
            .unwrap_or_else(|e| panic!("seed insert {i} failed: {e}"));
    }
}

/// Insert a batch of user records: (id, name) pairs.
pub async fn insert_users(
    db: &(impl DatabaseOutbound + ?Sized),
    table: &str,
    users: &[(&str, &str)],
) {
    for (id, name) in users {
        db.insert(table, records::record(id, name))
            .await
            .unwrap_or_else(|e| panic!("seed user {id} failed: {e}"));
    }
}

/// Insert categorized records for filtering tests.
///
/// Creates `count` records spread across `categories` round-robin.
pub async fn insert_categorized_records(
    db: &(impl DatabaseOutbound + ?Sized),
    table: &str,
    count: usize,
    categories: &[&str],
) {
    for i in 0..count {
        let cat = categories[i % categories.len()];
        db.insert(
            table,
            records::record_with_category(&i.to_string(), &format!("item-{i}"), cat),
        )
        .await
        .unwrap_or_else(|e| panic!("seed categorized {i} failed: {e}"));
    }
}

/// Write N small text files into the gateway root.
///
/// Files are named `file-0.txt` through `file-{count-1}.txt`.
pub async fn write_numbered_files(
    gw: &(impl FileOutbound + ?Sized),
    count: usize,
    content: &[u8],
) {
    for i in 0..count {
        gw.write(
            &format!("file-{i}.txt"),
            content.to_vec(),
            UploadOptions::overwrite(),
        )
        .await
        .unwrap_or_else(|e| panic!("seed file {i} failed: {e}"));
    }
}

/// Write files with specific names into the gateway.
pub async fn write_named_files(
    gw: &(impl FileOutbound + ?Sized),
    names: &[&str],
    content: &[u8],
) {
    for name in names {
        gw.write(name, content.to_vec(), UploadOptions::overwrite())
            .await
            .unwrap_or_else(|e| panic!("seed file {name} failed: {e}"));
    }
}
