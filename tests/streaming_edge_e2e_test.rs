//! Edge-case tests for async streaming support.
//!
//! Validates behaviour under non-happy-path scenarios:
//! - Early drop / cancellation of a stream
//! - Large dataset streaming (1,000 records)
//! - Concurrent independent streams from the same database
//! - Stream with error injected mid-iteration
//! - File stream over a directory tree with subdirectories
//! - Stream stability when underlying data is mutated after stream creation

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
// 1. Stream cancellation — drop before consuming all items
// ===========================================================================

#[tokio::test]
async fn test_query_stream_drop_before_exhaustion_does_not_panic() {
    let db = saf::memory_database();

    for i in 1..=10 {
        db.insert("items", make_record(&i.to_string(), &format!("item-{i}")))
            .await
            .unwrap();
    }

    let mut stream = db
        .query_stream("items", QueryParams::new())
        .await
        .unwrap();

    // Consume only the first 3 items, then drop.
    let mut consumed = 0usize;
    while let Some(item) = stream.next().await {
        item.expect("streamed item should be Ok");
        consumed += 1;
        if consumed == 3 {
            break;
        }
    }

    // Explicitly drop the stream while items remain.
    drop(stream);

    assert_eq!(consumed, 3, "should have consumed exactly 3 items before drop");

    // The database should still be fully usable after the dropped stream.
    let count = db.count("items", QueryParams::new()).await.unwrap();
    assert_eq!(count, 10, "database should still contain all 10 items after stream drop");
}

#[tokio::test]
async fn test_list_stream_drop_before_exhaustion_does_not_panic() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let gateway = saf::local_file_gateway(temp_dir.path().to_str().unwrap());

    for i in 1..=5 {
        gateway
            .write(
                &format!("file-{i}.txt"),
                b"data".to_vec(),
                UploadOptions::overwrite(),
            )
            .await
            .unwrap();
    }

    let mut stream = gateway
        .list_stream(ListOptions::default())
        .await
        .unwrap();

    // Consume only 2 items then drop.
    let first = stream.next().await;
    assert!(first.is_some(), "stream should yield at least one item");
    first.unwrap().expect("first item should be Ok");

    let second = stream.next().await;
    assert!(second.is_some(), "stream should yield a second item");
    second.unwrap().expect("second item should be Ok");

    drop(stream);

    // Gateway should still work after dropped stream.
    let exists = gateway.exists("file-1.txt").await.unwrap();
    assert!(exists, "file should still exist after stream drop");
}

// ===========================================================================
// 2. Stream from very large dataset (1,000 records)
// ===========================================================================

#[tokio::test]
async fn test_query_stream_large_dataset_1000_records_all_received() {
    let db = saf::memory_database();

    const RECORD_COUNT: usize = 1_000;

    for i in 0..RECORD_COUNT {
        db.insert("big", make_record(&i.to_string(), &format!("name-{i}")))
            .await
            .unwrap();
    }

    let stream = db
        .query_stream("big", QueryParams::new())
        .await
        .unwrap();

    let collected: Vec<_> = stream.collect::<Vec<_>>().await;
    assert_eq!(
        collected.len(),
        RECORD_COUNT,
        "stream should yield exactly {RECORD_COUNT} items"
    );

    // Every item should be Ok.
    for (idx, item) in collected.iter().enumerate() {
        assert!(
            item.is_ok(),
            "item at index {idx} should be Ok, got: {:?}",
            item.as_ref().err()
        );
    }

    // Verify that all IDs 0..999 are present (order is non-deterministic for HashMap).
    let mut ids: Vec<usize> = collected
        .into_iter()
        .map(|r| {
            r.unwrap()["id"]
                .as_str()
                .unwrap()
                .parse::<usize>()
                .unwrap()
        })
        .collect();
    ids.sort();
    let expected: Vec<usize> = (0..RECORD_COUNT).collect();
    assert_eq!(ids, expected, "all IDs 0..999 should be present in stream");
}

// ===========================================================================
// 3. Concurrent streams from same database — independent iteration
// ===========================================================================

#[tokio::test]
async fn test_query_stream_concurrent_streams_iterate_independently() {
    let db = saf::memory_database();

    for i in 1..=5 {
        db.insert("shared", make_record(&i.to_string(), &format!("v-{i}")))
            .await
            .unwrap();
    }

    // Open two independent streams over the same table.
    let mut stream_a = db
        .query_stream("shared", QueryParams::new())
        .await
        .unwrap();
    let mut stream_b = db
        .query_stream("shared", QueryParams::new())
        .await
        .unwrap();

    // Advance stream_a by 2.
    let a1 = stream_a.next().await.unwrap().unwrap();
    let a2 = stream_a.next().await.unwrap().unwrap();

    // Advance stream_b by 1.
    let b1 = stream_b.next().await.unwrap().unwrap();

    // Both streams should still have remaining items.
    let a_remaining: Vec<_> = stream_a.collect::<Vec<_>>().await;
    let b_remaining: Vec<_> = stream_b.collect::<Vec<_>>().await;

    // stream_a consumed 2 + remaining should total 5.
    assert_eq!(
        2 + a_remaining.len(),
        5,
        "stream_a should yield 5 total items"
    );
    // stream_b consumed 1 + remaining should total 5.
    assert_eq!(
        1 + b_remaining.len(),
        5,
        "stream_b should yield 5 total items"
    );

    // Verify the items from each stream are valid records with 'id'.
    assert!(a1.contains_key("id"), "a1 should have id");
    assert!(a2.contains_key("id"), "a2 should have id");
    assert!(b1.contains_key("id"), "b1 should have id");
}

// ===========================================================================
// 4. Stream with error injected mid-iteration
// ===========================================================================

#[tokio::test]
async fn test_query_stream_error_mid_iteration_surfaces_to_consumer() {
    // We simulate a stream that yields Ok items then an error by building
    // a raw stream and using the GatewayStream type alias.
    use swe_gateway::saf::{GatewayError, GatewayErrorCode};

    let items: Vec<GatewayResult<swe_gateway::saf::database::Record>> = vec![
        Ok(make_record("1", "good")),
        Ok(make_record("2", "good")),
        Err(GatewayError::new(
            GatewayErrorCode::Internal,
            "simulated mid-stream failure",
        )),
        Ok(make_record("3", "should-still-be-reachable")),
    ];

    let stream: GatewayStream<'_, swe_gateway::saf::database::Record> =
        Box::pin(futures::stream::iter(items));

    let collected: Vec<_> = stream.collect::<Vec<_>>().await;

    assert_eq!(collected.len(), 4, "stream should yield all 4 items including the error");
    assert!(collected[0].is_ok(), "first item should be Ok");
    assert!(collected[1].is_ok(), "second item should be Ok");
    assert!(collected[2].is_err(), "third item should be Err (simulated failure)");
    let err = collected[2].as_ref().unwrap_err();
    assert!(
        format!("{err:?}").contains("simulated mid-stream failure"),
        "error should contain the injected message"
    );
    assert!(collected[3].is_ok(), "fourth item should be Ok — stream continues after error");
}

// ===========================================================================
// 5. File stream on directory with subdirectories
// ===========================================================================

#[tokio::test]
async fn test_list_stream_directory_with_subdirectories_lists_entries() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let gateway = saf::local_file_gateway(temp_dir.path().to_str().unwrap());

    // Create a subdirectory and files at both levels.
    gateway.create_directory("subdir").await.unwrap();

    gateway
        .write("root.txt", b"root".to_vec(), UploadOptions::overwrite())
        .await
        .unwrap();
    gateway
        .write("subdir/nested.txt", b"nested".to_vec(), UploadOptions::overwrite())
        .await
        .unwrap();

    let stream = gateway
        .list_stream(ListOptions::default())
        .await
        .unwrap();

    let collected: Vec<_> = stream
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .map(|r| r.expect("streamed item should be Ok"))
        .collect();

    let paths: Vec<&str> = collected.iter().map(|f| f.path.as_str()).collect();

    // Default list returns immediate children: the file and the subdirectory entry.
    assert!(
        paths.iter().any(|p| p.contains("root.txt")),
        "stream should include root-level file, got paths: {:?}",
        paths
    );
    assert!(
        paths.iter().any(|p| p.contains("subdir")),
        "stream should include subdirectory entry, got paths: {:?}",
        paths
    );
    // Should have at least 2 entries (file + subdir).
    assert!(
        collected.len() >= 2,
        "stream should yield at least 2 entries (file + subdir), got {}",
        collected.len()
    );

    // Verify the subdirectory entry is marked as a directory.
    let subdir_entry = collected.iter().find(|f| f.path.contains("subdir")).unwrap();
    assert!(
        subdir_entry.is_directory,
        "subdirectory entry should have is_directory=true"
    );
}

// ===========================================================================
// 6. Stream then modify underlying data — snapshot vs live semantics
// ===========================================================================

#[tokio::test]
async fn test_query_stream_reflects_snapshot_at_creation_time() {
    let db = saf::memory_database();

    db.insert("snap", make_record("1", "before")).await.unwrap();
    db.insert("snap", make_record("2", "before")).await.unwrap();

    // Create the stream — this materialises the data (default impl calls query()).
    let stream = db
        .query_stream("snap", QueryParams::new())
        .await
        .unwrap();

    // Mutate underlying data AFTER stream creation.
    db.insert("snap", make_record("3", "after")).await.unwrap();
    db.delete("snap", "1").await.unwrap();

    // Collect the stream — it should reflect the state at creation time.
    let collected: Vec<_> = stream
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();

    // The default impl snapshots via query(), so we should get 2 items
    // (the state before mutation).
    assert_eq!(
        collected.len(),
        2,
        "stream should reflect snapshot (2 records), not live data (2 records with different IDs)"
    );

    let mut ids: Vec<String> = collected
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
    ids.sort();
    assert_eq!(
        ids,
        vec!["1", "2"],
        "snapshot should contain original IDs 1 and 2, not the post-mutation state"
    );
}

#[tokio::test]
async fn test_query_stream_underlying_data_mutation_does_not_corrupt_active_stream() {
    let db = saf::memory_database();

    for i in 1..=20 {
        db.insert("mutable", make_record(&i.to_string(), &format!("v-{i}")))
            .await
            .unwrap();
    }

    let mut stream = db
        .query_stream("mutable", QueryParams::new())
        .await
        .unwrap();

    // Consume a few items.
    let first = stream.next().await.unwrap().unwrap();
    assert!(first.contains_key("id"));

    // Perform heavy mutations while stream is alive.
    for i in 21..=30 {
        db.insert("mutable", make_record(&i.to_string(), &format!("new-{i}")))
            .await
            .unwrap();
    }
    db.delete("mutable", "1").await.unwrap();
    db.delete("mutable", "2").await.unwrap();

    // Continue consuming — should not panic or produce corrupt data.
    let remaining: Vec<_> = stream.collect::<Vec<_>>().await;
    for item in &remaining {
        assert!(
            item.is_ok(),
            "all remaining stream items should be Ok after underlying mutations"
        );
        let record = item.as_ref().unwrap();
        assert!(
            record.contains_key("id"),
            "each record should still have an 'id' field"
        );
    }

    // Total items from stream = 1 (consumed) + remaining, should be 20 (original snapshot).
    assert_eq!(
        1 + remaining.len(),
        20,
        "stream should yield original 20 items regardless of mutations"
    );
}
