//! DaemonRunner — shared startup sequence for gateway daemons.
//!
//! Handles the common lifecycle:
//! 1. Generate daemon ID
//! 2. Wrap in MDC logging context
//! 3. Select observability backend from config
//! 4. Spawn obsrv sidecar if selected
//! 5. Call user-provided server function with context
//!
//! Used by both llmboot (`llm serve`) and microvm (`xkvm daemon`).

use std::future::Future;
use std::net::SocketAddr;
use swe_observ_processes::{ObsrvProcess, DEFAULT_OBSRV_PORT};

/// Context passed to the user's server function.
pub struct DaemonContext {
    /// Unique daemon session ID.
    pub daemon_id: String,
    /// Bind address for the server.
    pub bind: SocketAddr,
    /// Service name (for tracing, metrics).
    pub service_name: String,
    /// Observability backend selected.
    pub backend: String,
    /// Obsrv sidecar port (0 if disabled).
    pub obsrv_port: u16,
}

/// Builder for configuring and running a daemon.
pub struct DaemonRunner {
    service_name: String,
    bind: String,
    backend: String,
    obsrv_port: u16,
    skip_observability: bool,
}

impl DaemonRunner {
    /// Create a new runner with defaults.
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
            bind: "0.0.0.0:9000".into(),
            backend: "sidecar".into(),
            obsrv_port: DEFAULT_OBSRV_PORT,
            skip_observability: false,
        }
    }

    /// Set the bind address.
    pub fn with_bind(mut self, bind: impl Into<String>) -> Self {
        self.bind = bind.into();
        self
    }

    /// Set the observability backend ("sidecar", "in-memory", "otel", "file").
    pub fn with_backend(mut self, backend: impl Into<String>) -> Self {
        self.backend = backend.into();
        self
    }

    /// Set the obsrv sidecar port (0 to disable).
    pub fn with_obsrv_port(mut self, port: u16) -> Self {
        self.obsrv_port = port;
        self
    }

    /// Skip observability setup (MDC context, sidecar spawn).
    ///
    /// When enabled, the runner skips MDC logging context creation and
    /// sidecar lifecycle management. The daemon context will use default
    /// no-op values: backend is set to `"none"` and obsrv_port to `0`.
    ///
    /// Useful for lightweight processes, CLI tools, or test harnesses
    /// that do not need full observability infrastructure.
    pub fn without_observability(mut self) -> Self {
        self.skip_observability = true;
        self
    }

    /// Returns whether observability is skipped for this runner.
    pub fn observability_skipped(&self) -> bool {
        self.skip_observability
    }

    /// Run the daemon with the given server function.
    ///
    /// The runner handles MDC context, sidecar lifecycle, and logging.
    /// The caller provides the actual server logic via the closure.
    pub async fn run<F, Fut>(self, server_fn: F) -> Result<(), Box<dyn std::error::Error>>
    where
        F: FnOnce(DaemonContext) -> Fut,
        Fut: Future<Output = Result<(), Box<dyn std::error::Error>>>,
    {
        let bind: SocketAddr = self.bind.parse().map_err(|e| {
            format!("Invalid bind address '{}': {}", self.bind, e)
        })?;

        let daemon_id = uuid::Uuid::new_v4().to_string();

        if self.skip_observability {
            tracing::info!(
                daemon_id = %daemon_id,
                service = %self.service_name,
                "starting daemon (lightweight — observability skipped)"
            );

            let ctx = DaemonContext {
                daemon_id,
                bind,
                service_name: self.service_name,
                backend: "none".into(),
                obsrv_port: 0,
            };

            return server_fn(ctx).await;
        }

        let log_ctx = mdc_logging::LogContext::builder()
            .session_id(daemon_id.clone())
            .agent_id(&self.service_name)
            .build();

        mdc_logging::with_log_context(log_ctx, async move {
            tracing::info!(
                daemon_id = %daemon_id,
                service = %self.service_name,
                "starting daemon"
            );

            // Spawn obsrv sidecar based on backend selection
            let _obsrv = match self.backend.as_str() {
                "sidecar" => {
                    let process = ObsrvProcess::spawn(self.obsrv_port);
                    if process.is_some() {
                        tracing::info!(port = self.obsrv_port, "obsrv sidecar active");
                    } else {
                        tracing::info!("obsrv not found — fallback to in-memory");
                    }
                    process
                }
                _ => {
                    tracing::info!(backend = %self.backend, "observability backend selected");
                    None
                }
            };

            let ctx = DaemonContext {
                daemon_id,
                bind,
                service_name: self.service_name,
                backend: self.backend,
                obsrv_port: self.obsrv_port,
            };

            server_fn(ctx).await
        }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_defaults() {
        let runner = DaemonRunner::new("test-service");
        assert_eq!(runner.service_name, "test-service");
        assert_eq!(runner.bind, "0.0.0.0:9000");
        assert_eq!(runner.backend, "sidecar");
        assert_eq!(runner.obsrv_port, DEFAULT_OBSRV_PORT);
        assert!(!runner.skip_observability, "observability should be enabled by default");
    }

    #[test]
    fn test_builder_chain() {
        let runner = DaemonRunner::new("my-svc")
            .with_bind("127.0.0.1:8080")
            .with_backend("in-memory")
            .with_obsrv_port(0);
        assert_eq!(runner.bind, "127.0.0.1:8080");
        assert_eq!(runner.backend, "in-memory");
        assert_eq!(runner.obsrv_port, 0);
    }

    #[test]
    fn test_without_observability_sets_flag() {
        let runner = DaemonRunner::new("lightweight-svc")
            .without_observability();
        assert!(runner.skip_observability, "without_observability should set skip flag");
        assert!(runner.observability_skipped(), "observability_skipped() should return true");
    }

    #[test]
    fn test_without_observability_chains_with_other_builders() {
        let runner = DaemonRunner::new("chained-svc")
            .with_bind("127.0.0.1:3000")
            .without_observability()
            .with_backend("otel");
        assert_eq!(runner.bind, "127.0.0.1:3000");
        assert!(runner.skip_observability);
        assert_eq!(runner.backend, "otel");
    }

    #[tokio::test]
    async fn test_run_without_observability_uses_noop_defaults() {
        let result = DaemonRunner::new("lightweight-test")
            .with_bind("127.0.0.1:0")
            .without_observability()
            .run(|ctx| async move {
                assert_eq!(ctx.service_name, "lightweight-test");
                assert_eq!(ctx.backend, "none", "backend should be 'none' when observability is skipped");
                assert_eq!(ctx.obsrv_port, 0, "obsrv_port should be 0 when observability is skipped");
                assert!(!ctx.daemon_id.is_empty(), "daemon_id should still be generated");
                Ok(())
            })
            .await;
        assert!(result.is_ok(), "lightweight run should succeed: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_run_with_in_memory_backend() {
        let result = DaemonRunner::new("test")
            .with_bind("127.0.0.1:0")
            .with_backend("in-memory")
            .run(|ctx| async move {
                assert_eq!(ctx.service_name, "test");
                assert_eq!(ctx.backend, "in-memory");
                Ok(())
            })
            .await;
        assert!(result.is_ok());
    }
}
