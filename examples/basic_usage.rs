//! Basic usage example for swe-gateway.
//!
//! Demonstrates gateway creation, CRUD operations, middleware,
//! streaming, configuration, and DaemonRunner modes.

use swe_gateway::prelude::*;
use swe_gateway::saf;
use swe_gateway::saf::database::QueryParams;
use swe_gateway::saf::file::UploadOptions;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ── Gateway creation ────────────────────────────────────────────
    let db = saf::memory_database();
    let files = saf::local_file_gateway("./data");
    let notifier = saf::silent_notifier();
    let payments = saf::mock_payment_gateway();

    // ── Database CRUD ───────────────────────────────────────────────
    let mut record = serde_json::Map::new();
    record.insert("id".into(), serde_json::json!("1"));
    record.insert("name".into(), serde_json::json!("Alice"));
    record.insert("score".into(), serde_json::json!(95));
    db.insert("users", record).await?;

    let users = db.query("users", QueryParams::new()).await?;
    println!("Users: {}", users.len());

    // ── Database filtering with operators ────────────────────────────
    let high_scorers = db
        .query("users", QueryParams::new().filter("score__gte", 90))
        .await?;
    println!("High scorers: {}", high_scorers.len());

    // ── Async streaming ─────────────────────────────────────────────
    let mut stream = db.query_stream("users", QueryParams::new()).await?;
    while let Some(item) = stream.next().await {
        let record = item?;
        println!("Streamed: {:?}", record.get("name"));
    }

    // ── Middleware ───────────────────────────────────────────────────
    // Retry: 3 attempts, exponential backoff with jitter
    let _retry = saf::retry_middleware()
        .max_attempts(3)
        .build();

    // Rate limiter: 100 burst capacity, 10 tokens/sec refill
    let _limiter = saf::rate_limiter(100, 10.0);

    // ── Configuration with env vars ─────────────────────────────────
    let toml_str = r#"
        [database]
        database_type = "memory"
    "#;
    let config = saf::load_config_from_str(toml_str)?;
    config.validate()?;
    println!("Config valid");

    // ── DaemonRunner (lightweight, no observability) ────────────────
    // saf::lightweight_daemon("example-svc")
    //     .with_bind("127.0.0.1:0")
    //     .run(|ctx| async move {
    //         println!("Daemon {} running", ctx.service_name);
    //         Ok(())
    //     })
    //     .await?;

    println!("All examples completed.");
    Ok(())
}
