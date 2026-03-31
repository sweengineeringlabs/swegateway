//! Output sink implementations.

use std::path::PathBuf;

use futures::future::BoxFuture;

use crate::api::output::OutputSink;
use crate::api::file::UploadOptions;
use crate::api::traits::FileOutbound;
use crate::api::types::{GatewayError, GatewayResult};
use crate::saf::config::{GatewayConfig, SinkType};

/// Writes output data to stdout.
pub(crate) struct StdoutSink;

impl OutputSink for StdoutSink {
    fn write(&self, data: &[u8]) -> BoxFuture<'_, GatewayResult<()>> {
        let output = String::from_utf8_lossy(data).into_owned();
        Box::pin(async move {
            print!("{}", output);
            Ok(())
        })
    }
}

/// Writes output data to a file via the file gateway.
pub(crate) struct FileSink {
    path: PathBuf,
}

impl FileSink {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl OutputSink for FileSink {
    fn write(&self, data: &[u8]) -> BoxFuture<'_, GatewayResult<()>> {
        let parent = self
            .path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf();
        let filename = self
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("output")
            .to_string();
        let data = data.to_vec();

        Box::pin(async move {
            let config = GatewayConfig::default()
                .with_file(|f| f.base_path = parent.to_string_lossy().into_owned());
            let gw = config.file_gateway();
            gw.write(&filename, data, UploadOptions::overwrite()).await?;
            Ok(())
        })
    }
}

/// Config-driven sink that dispatches to stdout or file based on `gateway.toml`.
pub(crate) struct ConfiguredOutputSink {
    config: GatewayConfig,
}

impl ConfiguredOutputSink {
    pub fn new(config: GatewayConfig) -> Self {
        Self { config }
    }
}

impl OutputSink for ConfiguredOutputSink {
    fn write(&self, data: &[u8]) -> BoxFuture<'_, GatewayResult<()>> {
        let sink_type = self.config.sink.sink_type;
        let path = self.config.sink.path.clone();
        let data = data.to_vec();

        Box::pin(async move {
            match sink_type {
                SinkType::Stdout => {
                    StdoutSink.write(&data).await
                }
                SinkType::File => {
                    let path = path.ok_or_else(|| {
                        GatewayError::invalid_input(
                            "sink_type is 'file' but no 'path' specified in gateway.toml",
                        )
                    })?;
                    FileSink::new(path).write(&data).await
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_stdout_sink_writes_without_error() {
        let sink = StdoutSink;
        let result = sink.write(b"hello stdout\n").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_file_sink_creates_file() {
        let dir = std::env::temp_dir().join("swe_gw_output_sink_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("output.txt");

        let sink = FileSink::new(path.clone());
        sink.write(b"file sink test").await.unwrap();

        assert!(path.exists());
        let contents = std::fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "file sink test");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_file_sink_creates_parent_dirs() {
        let dir = std::env::temp_dir().join("swe_gw_output_sink_parents");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("a").join("b").join("out.txt");

        let sink = FileSink::new(path.clone());
        sink.write(b"nested").await.unwrap();

        assert!(path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_configured_sink_stdout() {
        let config = GatewayConfig::default(); // defaults to stdout
        let sink = ConfiguredOutputSink::new(config);
        let result = sink.write(b"configured stdout\n").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_configured_sink_file() {
        let dir = std::env::temp_dir().join("swe_gw_configured_sink_test");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("report.json");

        let config = GatewayConfig::default().with_sink(|s| {
            s.sink_type = SinkType::File;
            s.path = Some(path.clone());
        });
        let sink = ConfiguredOutputSink::new(config);
        sink.write(b"{\"test\": true}").await.unwrap();

        assert!(path.exists());
        let contents = std::fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "{\"test\": true}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_configured_sink_file_missing_path() {
        let config = GatewayConfig::default().with_sink(|s| {
            s.sink_type = SinkType::File;
            s.path = None;
        });
        let sink = ConfiguredOutputSink::new(config);
        let result = sink.write(b"data").await;
        assert!(result.is_err());
    }
}
