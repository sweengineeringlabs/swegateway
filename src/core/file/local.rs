//! Local filesystem implementation.

use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use std::path::PathBuf;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::api::{
    file::{FileInfo, ListOptions, ListResult, PresignedUrl, UploadOptions},
    traits::{FileGateway, FileInbound, FileOutbound},
    types::{GatewayError, GatewayResult, HealthCheck},
};

/// Local filesystem gateway implementation.
#[derive(Debug, Clone)]
pub(crate) struct LocalFileGateway {
    /// Base directory for all operations.
    base_path: PathBuf,
}

impl LocalFileGateway {
    /// Creates a new local file gateway with the given base path.
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        Self {
            base_path: base_path.into(),
        }
    }

    /// Creates a gateway using the current directory.
    pub fn current_dir() -> std::io::Result<Self> {
        Ok(Self {
            base_path: std::env::current_dir()?,
        })
    }

    /// Resolves a path relative to the base path.
    fn resolve_path(&self, path: &str) -> PathBuf {
        let path = path.trim_start_matches('/');
        self.base_path.join(path)
    }

    /// Converts file metadata to FileInfo.
    fn metadata_to_info(path: &str, metadata: std::fs::Metadata) -> FileInfo {
        let modified: DateTime<Utc> = metadata
            .modified()
            .map(|t| t.into())
            .unwrap_or_else(|_| Utc::now());

        let created: Option<DateTime<Utc>> = metadata.created().ok().map(|t| t.into());

        let content_type = mime_guess::from_path(path)
            .first()
            .map(|m| m.to_string());

        FileInfo {
            path: path.to_string(),
            size: metadata.len(),
            content_type,
            last_modified: modified,
            created_at: created,
            etag: None,
            is_directory: metadata.is_dir(),
            metadata: std::collections::HashMap::new(),
        }
    }
}

impl FileInbound for LocalFileGateway {
    fn read(&self, path: &str) -> BoxFuture<'_, GatewayResult<Vec<u8>>> {
        let full_path = self.resolve_path(path);
        let path_str = path.to_string();
        Box::pin(async move {
            let mut file = fs::File::open(&full_path).await.map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    GatewayError::NotFound(format!("File not found: {}", path_str))
                } else {
                    GatewayError::IoError(e)
                }
            })?;

            let mut contents = Vec::new();
            file.read_to_end(&mut contents).await?;
            Ok(contents)
        })
    }

    fn metadata(&self, path: &str) -> BoxFuture<'_, GatewayResult<FileInfo>> {
        let full_path = self.resolve_path(path);
        let path = path.to_string();
        Box::pin(async move {
            let metadata = fs::metadata(&full_path).await.map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    GatewayError::NotFound(format!("File not found: {}", path))
                } else {
                    GatewayError::IoError(e)
                }
            })?;

            Ok(Self::metadata_to_info(&path, metadata))
        })
    }

    fn list(&self, options: ListOptions) -> BoxFuture<'_, GatewayResult<ListResult>> {
        let base = self.base_path.clone();
        Box::pin(async move {
            let search_path = match &options.prefix {
                Some(prefix) => base.join(prefix.trim_start_matches('/')),
                None => base.clone(),
            };

            let mut files = Vec::new();
            let mut prefixes = Vec::new();

            if search_path.is_dir() {
                let mut entries = fs::read_dir(&search_path).await?;

                while let Some(entry) = entries.next_entry().await? {
                    let metadata = entry.metadata().await?;
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    let relative_path = entry
                        .path()
                        .strip_prefix(&base)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or(file_name.clone());

                    if metadata.is_dir() {
                        if options.delimiter.is_some() {
                            prefixes.push(relative_path);
                        } else {
                            files.push(Self::metadata_to_info(&relative_path, metadata));
                        }
                    } else {
                        files.push(Self::metadata_to_info(&relative_path, metadata));
                    }

                    if let Some(max) = options.max_results {
                        if files.len() >= max {
                            break;
                        }
                    }
                }
            }

            Ok(ListResult {
                files,
                prefixes,
                next_continuation_token: None,
                is_truncated: false,
            })
        })
    }

    fn exists(&self, path: &str) -> BoxFuture<'_, GatewayResult<bool>> {
        let full_path = self.resolve_path(path);
        Box::pin(async move {
            Ok(fs::try_exists(&full_path).await.unwrap_or(false))
        })
    }

    fn presigned_read_url(
        &self,
        path: &str,
        _expires_in_secs: u64,
    ) -> BoxFuture<'_, GatewayResult<PresignedUrl>> {
        let full_path = self.resolve_path(path);
        Box::pin(async move {
            // Local filesystem doesn't support presigned URLs,
            // but we return a file:// URL for compatibility
            Ok(PresignedUrl {
                url: format!("file://{}", full_path.display()),
                expires_at: Utc::now() + chrono::Duration::hours(24),
                method: "GET".to_string(),
            })
        })
    }

    fn health_check(&self) -> BoxFuture<'_, GatewayResult<HealthCheck>> {
        let base = self.base_path.clone();
        Box::pin(async move {
            match fs::try_exists(&base).await {
                Ok(true) => Ok(HealthCheck::healthy()),
                Ok(false) => Ok(HealthCheck::unhealthy("Base path does not exist")),
                Err(e) => Ok(HealthCheck::unhealthy(format!("Cannot access base path: {}", e))),
            }
        })
    }
}

impl FileOutbound for LocalFileGateway {
    fn write(
        &self,
        path: &str,
        contents: Vec<u8>,
        options: UploadOptions,
    ) -> BoxFuture<'_, GatewayResult<FileInfo>> {
        let full_path = self.resolve_path(path);
        let path = path.to_string();
        Box::pin(async move {
            // Check if file exists and overwrite is disabled
            if !options.overwrite && fs::try_exists(&full_path).await.unwrap_or(false) {
                return Err(GatewayError::Conflict(format!(
                    "File already exists: {}",
                    path
                )));
            }

            // Create parent directories if needed
            if let Some(parent) = full_path.parent() {
                fs::create_dir_all(parent).await?;
            }

            // Write the file
            let mut file = fs::File::create(&full_path).await?;
            file.write_all(&contents).await?;
            file.flush().await?;

            // Get metadata for the response
            let metadata = fs::metadata(&full_path).await?;
            Ok(Self::metadata_to_info(&path, metadata))
        })
    }

    fn delete(&self, path: &str) -> BoxFuture<'_, GatewayResult<()>> {
        let full_path = self.resolve_path(path);
        let path_str = path.to_string();
        Box::pin(async move {
            fs::remove_file(&full_path).await.map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    GatewayError::NotFound(format!("File not found: {}", path_str))
                } else {
                    GatewayError::IoError(e)
                }
            })
        })
    }

    fn copy(&self, source: &str, destination: &str) -> BoxFuture<'_, GatewayResult<FileInfo>> {
        let source_path = self.resolve_path(source);
        let dest_path = self.resolve_path(destination);
        let source_str = source.to_string();
        let dest_str = destination.to_string();
        Box::pin(async move {
            // Create parent directories if needed
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent).await?;
            }

            fs::copy(&source_path, &dest_path).await.map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    GatewayError::NotFound(format!("Source file not found: {}", source_str))
                } else {
                    GatewayError::IoError(e)
                }
            })?;

            let metadata = fs::metadata(&dest_path).await?;
            Ok(Self::metadata_to_info(&dest_str, metadata))
        })
    }

    fn rename(&self, source: &str, destination: &str) -> BoxFuture<'_, GatewayResult<FileInfo>> {
        let source_path = self.resolve_path(source);
        let dest_path = self.resolve_path(destination);
        let source_str = source.to_string();
        let dest_str = destination.to_string();
        Box::pin(async move {
            // Create parent directories if needed
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent).await?;
            }

            fs::rename(&source_path, &dest_path).await.map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    GatewayError::NotFound(format!("Source file not found: {}", source_str))
                } else {
                    GatewayError::IoError(e)
                }
            })?;

            let metadata = fs::metadata(&dest_path).await?;
            Ok(Self::metadata_to_info(&dest_str, metadata))
        })
    }

    fn create_directory(&self, path: &str) -> BoxFuture<'_, GatewayResult<()>> {
        let full_path = self.resolve_path(path);
        Box::pin(async move {
            fs::create_dir_all(&full_path).await?;
            Ok(())
        })
    }

    fn delete_directory(&self, path: &str, recursive: bool) -> BoxFuture<'_, GatewayResult<()>> {
        let full_path = self.resolve_path(path);
        Box::pin(async move {
            if recursive {
                fs::remove_dir_all(&full_path).await?;
            } else {
                fs::remove_dir(&full_path).await?;
            }
            Ok(())
        })
    }

    fn presigned_upload_url(
        &self,
        path: &str,
        _expires_in_secs: u64,
    ) -> BoxFuture<'_, GatewayResult<PresignedUrl>> {
        let full_path = self.resolve_path(path);
        Box::pin(async move {
            // Local filesystem doesn't support presigned URLs
            Ok(PresignedUrl {
                url: format!("file://{}", full_path.display()),
                expires_at: Utc::now() + chrono::Duration::hours(24),
                method: "PUT".to_string(),
            })
        })
    }
}

impl FileGateway for LocalFileGateway {}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_write_and_read() {
        let temp_dir = TempDir::new().unwrap();
        let gateway = LocalFileGateway::new(temp_dir.path());

        let content = b"Hello, World!".to_vec();
        gateway
            .write("test.txt", content.clone(), UploadOptions::overwrite())
            .await
            .unwrap();

        let read_content = gateway.read("test.txt").await.unwrap();
        assert_eq!(read_content, content);
    }

    #[tokio::test]
    async fn test_exists() {
        let temp_dir = TempDir::new().unwrap();
        let gateway = LocalFileGateway::new(temp_dir.path());

        assert!(!gateway.exists("test.txt").await.unwrap());

        gateway
            .write("test.txt", b"content".to_vec(), UploadOptions::overwrite())
            .await
            .unwrap();

        assert!(gateway.exists("test.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_delete() {
        let temp_dir = TempDir::new().unwrap();
        let gateway = LocalFileGateway::new(temp_dir.path());

        gateway
            .write("test.txt", b"content".to_vec(), UploadOptions::overwrite())
            .await
            .unwrap();

        gateway.delete("test.txt").await.unwrap();

        assert!(!gateway.exists("test.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_copy() {
        let temp_dir = TempDir::new().unwrap();
        let gateway = LocalFileGateway::new(temp_dir.path());

        let content = b"original content".to_vec();
        gateway
            .write("original.txt", content.clone(), UploadOptions::overwrite())
            .await
            .unwrap();

        gateway.copy("original.txt", "copy.txt").await.unwrap();

        assert!(gateway.exists("original.txt").await.unwrap());
        assert!(gateway.exists("copy.txt").await.unwrap());

        let copied_content = gateway.read("copy.txt").await.unwrap();
        assert_eq!(copied_content, content);
    }

    /// @covers: current_dir
    #[test]
    fn test_current_dir_creates_gateway() {
        let gateway = LocalFileGateway::current_dir();
        assert!(gateway.is_ok(), "current_dir() should return Ok");

        let gateway = gateway.unwrap();
        let expected = std::env::current_dir().unwrap();
        assert_eq!(
            gateway.base_path, expected,
            "current_dir gateway should use std::env::current_dir as base_path"
        );
    }

    /// @covers: current_dir
    #[tokio::test]
    async fn test_current_dir() {
        let gateway = LocalFileGateway::current_dir();
        assert!(gateway.is_ok(), "current_dir() should return Ok");

        let gateway = gateway.unwrap();
        // Verify the gateway is functional by checking health
        let health = gateway.health_check().await.unwrap();
        assert_eq!(
            health.status,
            crate::api::types::HealthStatus::Healthy,
            "gateway from current_dir should be healthy"
        );
    }
}
