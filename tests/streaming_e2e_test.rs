//! End-to-end tests for BL-005: async streaming support.
//!
//! Validates that `DatabaseInbound::query_stream` and `FileInbound::list_stream`
//! deliver items incrementally and produce the same results as their `Vec`-based
//! counterparts.

use swe_gateway::prelude::*;
use swe_gateway::saf;
use swe_gateway::saf::database::QueryParams;
use swe_gateway::saf::file::{ListOptions, UploadOptions};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_record(id: &str, name: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut r = serde_json::Map::new();
    r.insert("id".into(), serde_json::json!(id));
    r.insert("name".into(), serde_json::json!(name));
    r
}

// ===========================================================================
// Database streaming tests
// ===========================================================================

#[tokio::test]
async fn test_query_stream_returns_all_items_one_by_one() {
    let db = saf::memory_database();

    db.insert("users", make_record("1", "Alice")).await.unwrap();
    db.insert("users", make_record("2", "Bob")).await.unwrap();
    db.insert("users", make_record("3", "Carol")).await.unwrap();

    let mut stream = db.query_stream("users", QueryParams::new()).await.unwrap();

    let mut count = 0usize;
    while let Some(item) = stream.next().await {
        let record = item.expect("each streamed item should be Ok");
        assert!(
            record.contains_key("id"),
            "streamed record should contain 'id' field"
        );
        count += 1;
    }

    assert_eq!(count, 3, "stream should yield exactly 3 records");
}

#[tokio::test]
async fn test_query_stream_empty_table_yields_no_items() {
    let db = saf::memory_database();

    let mut stream = db
        .query_stream("nonexistent", QueryParams::new())
        .await
        .unwrap();

    assert!(
        stream.next().await.is_none(),
        "stream from empty/missing table should yield None immediately"
    );
}

#[tokio::test]
async fn test_query_stream_with_filter_matches_vec_query() {
    let db = saf::memory_database();

    for i in 1..=6 {
        let mut r = make_record(&i.to_string(), &format!("user-{}", i));
        r.insert(
            "active".into(),
            serde_json::json!(i % 2 == 0),
        );
        db.insert("users", r).await.unwrap();
    }

    let params = QueryParams::new().filter("active", true);

    // Vec path
    let vec_results = db.query("users", params.clone()).await.unwrap();

    // Stream path
    let stream = db.query_stream("users", params).await.unwrap();
    let stream_results: Vec<_> = stream
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .map(|r| r.expect("stream item should be Ok"))
        .collect();

    assert_eq!(
        vec_results.len(),
        stream_results.len(),
        "stream and vec should return the same number of records"
    );

    // Verify the same IDs are present (order may differ for HashMap-backed storage)
    let mut vec_ids: Vec<String> = vec_results
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
    vec_ids.sort();

    let mut stream_ids: Vec<String> = stream_results
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
    stream_ids.sort();

    assert_eq!(vec_ids, stream_ids, "stream and vec should return the same record IDs");
}

#[tokio::test]
async fn test_query_stream_collects_to_same_result_as_vec() {
    let db = saf::memory_database();

    db.insert("items", make_record("a", "Apple")).await.unwrap();
    db.insert("items", make_record("b", "Banana")).await.unwrap();

    let params = QueryParams::new();

    let vec_result = db.query("items", params.clone()).await.unwrap();

    let stream = db.query_stream("items", params).await.unwrap();
    let stream_collected: Vec<_> = stream
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();

    // Same count
    assert_eq!(vec_result.len(), stream_collected.len());

    // Same IDs (sorted, since HashMap order is non-deterministic)
    let mut vec_ids: Vec<String> = vec_result
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
    vec_ids.sort();

    let mut stream_ids: Vec<String> = stream_collected
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
    stream_ids.sort();

    assert_eq!(vec_ids, stream_ids);
}

// ===========================================================================
// File streaming tests
// ===========================================================================

#[tokio::test]
async fn test_list_stream_returns_files_one_by_one() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let gateway = saf::local_file_gateway(temp_dir.path().to_str().unwrap());

    // Create several files
    for name in &["a.txt", "b.txt", "c.txt"] {
        gateway
            .write(name, b"data".to_vec(), UploadOptions::overwrite())
            .await
            .unwrap();
    }

    let mut stream = gateway
        .list_stream(ListOptions::default())
        .await
        .unwrap();

    let mut count = 0usize;
    while let Some(item) = stream.next().await {
        let file_info = item.expect("each streamed file info should be Ok");
        assert!(
            !file_info.path.is_empty(),
            "streamed FileInfo should have a non-empty path"
        );
        count += 1;
    }

    assert_eq!(count, 3, "stream should yield exactly 3 file entries");
}

#[tokio::test]
async fn test_list_stream_empty_directory_yields_no_items() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let gateway = saf::local_file_gateway(temp_dir.path().to_str().unwrap());

    let mut stream = gateway
        .list_stream(ListOptions::default())
        .await
        .unwrap();

    assert!(
        stream.next().await.is_none(),
        "stream from empty directory should yield None immediately"
    );
}

#[tokio::test]
async fn test_list_stream_collects_to_same_result_as_vec() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let gateway = saf::local_file_gateway(temp_dir.path().to_str().unwrap());

    for name in &["x.txt", "y.txt", "z.txt"] {
        gateway
            .write(name, b"content".to_vec(), UploadOptions::overwrite())
            .await
            .unwrap();
    }

    let options = ListOptions::default();

    // Vec path
    let list_result = gateway.list(options.clone()).await.unwrap();

    // Stream path
    let stream = gateway.list_stream(ListOptions::default()).await.unwrap();
    let stream_collected: Vec<_> = stream
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();

    assert_eq!(
        list_result.files.len(),
        stream_collected.len(),
        "stream and list should return the same number of files"
    );

    let mut list_paths: Vec<String> = list_result.files.iter().map(|f| f.path.clone()).collect();
    list_paths.sort();

    let mut stream_paths: Vec<String> =
        stream_collected.iter().map(|f| f.path.clone()).collect();
    stream_paths.sort();

    assert_eq!(
        list_paths, stream_paths,
        "stream and list should return the same file paths"
    );
}

// ===========================================================================
// GatewayStream type alias test
// ===========================================================================

#[tokio::test]
async fn test_gateway_stream_type_alias_is_usable() {
    // Verify the GatewayStream type alias compiles and works in consumer code.
    let db = saf::memory_database();
    db.insert("t", make_record("1", "test")).await.unwrap();

    let stream: GatewayStream<'_, swe_gateway::saf::database::Record> =
        db.query_stream("t", QueryParams::new()).await.unwrap();

    let items: Vec<_> = stream.collect::<Vec<_>>().await;
    assert_eq!(items.len(), 1);
    assert!(items[0].is_ok());
}
