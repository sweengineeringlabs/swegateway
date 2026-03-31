//! E2E tests for DaemonRunner lightweight (observability-skipped) mode.
//!
//! Validates BL-007: DaemonRunner observability fallback.

use swe_gateway::saf::{lightweight_daemon, DaemonRunner};

// ---------------------------------------------------------------------------
// Builder configuration tests
// ---------------------------------------------------------------------------

#[test]
fn test_lightweight_daemon_builder_skips_observability() {
    let runner = lightweight_daemon("e2e-lightweight");
    assert!(
        runner.observability_skipped(),
        "lightweight_daemon() must produce a runner with observability skipped"
    );
}

#[test]
fn test_daemon_runner_default_does_not_skip_observability() {
    let runner = DaemonRunner::new("e2e-default");
    assert!(
        !runner.observability_skipped(),
        "DaemonRunner::new() must NOT skip observability by default"
    );
}

#[test]
fn test_without_observability_chains_with_bind() {
    let runner = DaemonRunner::new("e2e-chain")
        .with_bind("127.0.0.1:4444")
        .without_observability()
        .with_backend("otel");
    assert!(runner.observability_skipped());
}

// ---------------------------------------------------------------------------
// Runtime tests — lightweight path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_run_lightweight_daemon_starts_successfully() {
    let result = lightweight_daemon("e2e-lw-run")
        .with_bind("127.0.0.1:0")
        .run(|ctx| async move {
            assert_eq!(ctx.service_name, "e2e-lw-run");
            assert_eq!(ctx.backend, "none", "lightweight run must set backend to 'none'");
            assert_eq!(ctx.obsrv_port, 0, "lightweight run must set obsrv_port to 0");
            assert!(!ctx.daemon_id.is_empty(), "daemon_id must still be generated");
            Ok(())
        })
        .await;

    assert!(result.is_ok(), "lightweight daemon run failed: {:?}", result.err());
}

#[tokio::test]
async fn test_run_default_daemon_preserves_backend() {
    let result = DaemonRunner::new("e2e-default-run")
        .with_bind("127.0.0.1:0")
        .with_backend("in-memory")
        .run(|ctx| async move {
            assert_eq!(ctx.backend, "in-memory", "default run must preserve configured backend");
            assert_eq!(ctx.service_name, "e2e-default-run");
            Ok(())
        })
        .await;

    assert!(result.is_ok(), "default daemon run failed: {:?}", result.err());
}

#[tokio::test]
async fn test_run_lightweight_daemon_server_error_propagates() {
    let result = lightweight_daemon("e2e-lw-err")
        .with_bind("127.0.0.1:0")
        .run(|_ctx| async move {
            Err("deliberate server error".into())
        })
        .await;

    assert!(result.is_err(), "server error must propagate through lightweight runner");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("deliberate server error"),
        "error message should be preserved, got: {msg}"
    );
}
