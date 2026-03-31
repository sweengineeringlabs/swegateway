//! Generic metrics bridge — a response middleware that extracts fields from
//! the response and records them via a pluggable metrics collector.
//!
//! This is intentionally generic: the caller supplies a [`MetricsCollector`]
//! and a field-extractor closure that knows how to pull metric-relevant data
//! out of a `serde_json::Value` response.

use async_trait::async_trait;
use std::sync::Arc;

use crate::api::middleware::ResponseMiddleware;
use crate::api::types::GatewayResult;

/// Extracted metric fields that the bridge knows how to record.
///
/// The extractor closure is responsible for pulling these values out of the
/// domain-specific response payload.
#[derive(Debug, Clone)]
pub struct MetricFields {
    /// Identifier for the upstream provider (e.g. "openai", "anthropic").
    pub provider: String,
    /// Model identifier (e.g. "gpt-4", "claude-3").
    pub model: String,
    /// Status label (e.g. "ok", "error").
    pub status: String,
    /// Latency in seconds.
    pub latency_secs: f64,
    /// Number of input/prompt tokens consumed.
    pub input_tokens: u64,
    /// Number of output/completion tokens produced.
    pub output_tokens: u64,
}

/// Trait for a generic metrics collector that the bridge delegates to.
///
/// Implementors connect this to their observability stack (Prometheus,
/// OpenTelemetry, in-memory counters for testing, etc.).
pub trait MetricsCollector: Send + Sync {
    /// Record a single completion event with the given fields.
    fn record_completion(
        &self,
        provider: &str,
        model: &str,
        status: &str,
        latency_secs: f64,
        input_tokens: u64,
        output_tokens: u64,
    );
}

/// Type alias for the field-extractor closure.
///
/// Given a response `serde_json::Value`, returns `Some(MetricFields)` if
/// the response contains enough data to record metrics, or `None` to skip.
pub type FieldExtractor =
    Arc<dyn Fn(&serde_json::Value) -> Option<MetricFields> + Send + Sync>;

/// A response middleware that extracts metric fields from each response
/// and records them via a [`MetricsCollector`].
///
/// The bridge itself never interprets the response schema — that knowledge
/// lives entirely in the caller-supplied [`FieldExtractor`].
pub struct MetricsResponseMiddleware {
    collector: Arc<dyn MetricsCollector>,
    extractor: FieldExtractor,
}

impl MetricsResponseMiddleware {
    /// Create a new metrics bridge with the given collector and extractor.
    pub fn new(
        collector: Arc<dyn MetricsCollector>,
        extractor: FieldExtractor,
    ) -> Self {
        Self {
            collector,
            extractor,
        }
    }
}

#[async_trait]
impl ResponseMiddleware for MetricsResponseMiddleware {
    async fn process_response(
        &self,
        response: serde_json::Value,
    ) -> GatewayResult<serde_json::Value> {
        if let Some(fields) = (self.extractor)(&response) {
            self.collector.record_completion(
                &fields.provider,
                &fields.model,
                &fields.status,
                fields.latency_secs,
                fields.input_tokens,
                fields.output_tokens,
            );

            tracing::info!(
                provider = %fields.provider,
                model = %fields.model,
                status = %fields.status,
                latency_secs = fields.latency_secs,
                input_tokens = fields.input_tokens,
                output_tokens = fields.output_tokens,
                "gateway.response.metrics"
            );
        }

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;

    /// In-memory collector that stores recorded events for assertion.
    struct InMemoryCollector {
        events: Mutex<Vec<MetricFields>>,
    }

    impl InMemoryCollector {
        fn new() -> Self {
            Self {
                events: Mutex::new(Vec::new()),
            }
        }

        fn recorded_events(&self) -> Vec<MetricFields> {
            self.events.lock().clone()
        }
    }

    impl MetricsCollector for InMemoryCollector {
        fn record_completion(
            &self,
            provider: &str,
            model: &str,
            status: &str,
            latency_secs: f64,
            input_tokens: u64,
            output_tokens: u64,
        ) {
            self.events.lock().push(MetricFields {
                provider: provider.to_string(),
                model: model.to_string(),
                status: status.to_string(),
                latency_secs,
                input_tokens,
                output_tokens,
            });
        }
    }

    fn sample_extractor() -> FieldExtractor {
        Arc::new(|response: &serde_json::Value| {
            Some(MetricFields {
                provider: response["provider"].as_str()?.to_string(),
                model: response["model"].as_str()?.to_string(),
                status: "ok".to_string(),
                latency_secs: response["latency_ms"].as_f64()? / 1000.0,
                input_tokens: response["usage"]["prompt_tokens"].as_u64()?,
                output_tokens: response["usage"]["completion_tokens"].as_u64()?,
            })
        })
    }

    #[tokio::test]
    async fn test_process_response_records_metrics() {
        let collector = Arc::new(InMemoryCollector::new());
        let bridge = MetricsResponseMiddleware::new(collector.clone(), sample_extractor());

        let response = serde_json::json!({
            "provider": "openai",
            "model": "gpt-4",
            "latency_ms": 420,
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5
            }
        });

        let result = bridge.process_response(response.clone()).await.unwrap();

        // Response is passed through unchanged.
        assert_eq!(result, response);

        // Metrics were recorded.
        let events = collector.recorded_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].provider, "openai");
        assert_eq!(events[0].model, "gpt-4");
        assert_eq!(events[0].input_tokens, 10);
        assert_eq!(events[0].output_tokens, 5);
        assert!((events[0].latency_secs - 0.42).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_process_response_skips_metrics_when_extractor_returns_none() {
        let collector = Arc::new(InMemoryCollector::new());
        let bridge = MetricsResponseMiddleware::new(collector.clone(), sample_extractor());

        // Response missing required fields — extractor returns None.
        let response = serde_json::json!({"id": "resp-1"});
        let result = bridge.process_response(response.clone()).await.unwrap();

        assert_eq!(result, response);
        assert!(collector.recorded_events().is_empty(), "no metrics should be recorded for incomplete response");
    }

    #[tokio::test]
    async fn test_process_response_passthrough_on_extraction_failure() {
        let collector = Arc::new(InMemoryCollector::new());
        // Extractor that always returns None.
        let extractor: FieldExtractor = Arc::new(|_| None);
        let bridge = MetricsResponseMiddleware::new(collector.clone(), extractor);

        let response = serde_json::json!({"data": "anything"});
        let result = bridge.process_response(response.clone()).await.unwrap();
        assert_eq!(result, response);
        assert!(collector.recorded_events().is_empty());
    }
}
