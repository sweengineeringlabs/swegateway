//! Output sink traits for writing data to various destinations.
//!
//! These are generic infrastructure abstractions — they write raw bytes
//! without knowledge of any domain types (e.g., `ScanReport`).
//! Domain crates format their data, then delegate to these sinks.

use futures::future::BoxFuture;

use crate::api::types::GatewayResult;

/// Trait for writing output data to a destination.
///
/// Implementations handle the raw I/O: stdout, file, network, etc.
/// The caller is responsible for formatting (JSON, text, etc.) before
/// passing data to the sink.
pub trait OutputSink: Send + Sync {
    /// Write raw bytes to this sink.
    fn write(&self, data: &[u8]) -> BoxFuture<'_, GatewayResult<()>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify the trait is object-safe (can be used as `dyn OutputSink`).
    #[test]
    fn test_output_sink_is_object_safe() {
        fn _assert_object_safe(_: &dyn OutputSink) {}
    }
}
