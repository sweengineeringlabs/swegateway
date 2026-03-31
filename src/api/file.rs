//! File gateway types and configuration.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// File storage backend type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileStorageType {
    /// Local filesystem.
    Local,
    /// Amazon S3.
    S3,
    /// Google Cloud Storage.
    Gcs,
    /// Azure Blob Storage.
    Azure,
    /// In-memory storage (for testing).
    Memory,
}

impl Default for FileStorageType {
    fn default() -> Self {
        Self::Local
    }
}

/// Configuration for file storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FileStorageConfig {
    /// Storage backend type.
    pub storage_type: FileStorageType,
    /// Base path or bucket name.
    pub base_path: String,
    /// Region (for cloud storage).
    pub region: Option<String>,
    /// Access key (for cloud storage).
    #[serde(skip_serializing)]
    pub access_key: Option<String>,
    /// Secret key (for cloud storage).
    #[serde(skip_serializing)]
    pub secret_key: Option<String>,
    /// Endpoint URL (for S3-compatible storage).
    pub endpoint: Option<String>,
    /// Additional options.
    #[serde(default)]
    pub options: HashMap<String, String>,
}

impl Default for FileStorageConfig {
    fn default() -> Self {
        Self {
            storage_type: FileStorageType::Local,
            base_path: ".".to_string(),
            region: None,
            access_key: None,
            secret_key: None,
            endpoint: None,
            options: HashMap::new(),
        }
    }
}

impl FileStorageConfig {
    /// Creates a local filesystem configuration.
    pub fn local(base_path: impl Into<String>) -> Self {
        Self {
            storage_type: FileStorageType::Local,
            base_path: base_path.into(),
            ..Default::default()
        }
    }

    /// Creates an S3 configuration.
    pub fn s3(bucket: impl Into<String>, region: impl Into<String>) -> Self {
        Self {
            storage_type: FileStorageType::S3,
            base_path: bucket.into(),
            region: Some(region.into()),
            ..Default::default()
        }
    }

    /// Creates an in-memory configuration for testing.
    pub fn memory() -> Self {
        Self {
            storage_type: FileStorageType::Memory,
            base_path: "/".to_string(),
            ..Default::default()
        }
    }
}

/// Metadata about a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    /// Content type (MIME type).
    pub content_type: Option<String>,
    /// Content encoding.
    pub content_encoding: Option<String>,
    /// Cache control header.
    pub cache_control: Option<String>,
    /// Custom metadata.
    #[serde(default)]
    pub custom: HashMap<String, String>,
}

impl Default for FileMetadata {
    fn default() -> Self {
        Self {
            content_type: None,
            content_encoding: None,
            cache_control: None,
            custom: HashMap::new(),
        }
    }
}

impl FileMetadata {
    /// Creates metadata with the given content type.
    pub fn with_content_type(content_type: impl Into<String>) -> Self {
        Self {
            content_type: Some(content_type.into()),
            ..Default::default()
        }
    }

    /// Adds custom metadata.
    pub fn with_custom(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.custom.insert(key.into(), value.into());
        self
    }
}

/// Information about a file in storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    /// File path relative to the storage root.
    pub path: String,
    /// File size in bytes.
    pub size: u64,
    /// Content type (MIME type).
    pub content_type: Option<String>,
    /// Last modified timestamp.
    pub last_modified: DateTime<Utc>,
    /// Creation timestamp (if available).
    pub created_at: Option<DateTime<Utc>>,
    /// ETag or version identifier.
    pub etag: Option<String>,
    /// Whether this is a directory.
    pub is_directory: bool,
    /// Additional metadata.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl FileInfo {
    /// Creates a new file info.
    pub fn new(path: impl Into<String>, size: u64) -> Self {
        Self {
            path: path.into(),
            size,
            content_type: None,
            last_modified: Utc::now(),
            created_at: None,
            etag: None,
            is_directory: false,
            metadata: HashMap::new(),
        }
    }

    /// Creates a directory info.
    pub fn directory(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            size: 0,
            content_type: None,
            last_modified: Utc::now(),
            created_at: None,
            etag: None,
            is_directory: true,
            metadata: HashMap::new(),
        }
    }
}

/// Options for listing files.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListOptions {
    /// Prefix to filter files.
    pub prefix: Option<String>,
    /// Delimiter for hierarchical listing.
    pub delimiter: Option<String>,
    /// Maximum number of results.
    pub max_results: Option<usize>,
    /// Continuation token for pagination.
    pub continuation_token: Option<String>,
    /// Include metadata in results.
    pub include_metadata: bool,
}

impl ListOptions {
    /// Creates new list options with a prefix.
    pub fn with_prefix(prefix: impl Into<String>) -> Self {
        Self {
            prefix: Some(prefix.into()),
            ..Default::default()
        }
    }

    /// Sets the maximum number of results.
    pub fn with_max_results(mut self, max: usize) -> Self {
        self.max_results = Some(max);
        self
    }
}

/// Result of a list operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResult {
    /// Files found.
    pub files: Vec<FileInfo>,
    /// Common prefixes (directories) when using delimiter.
    pub prefixes: Vec<String>,
    /// Continuation token for next page.
    pub next_continuation_token: Option<String>,
    /// Whether there are more results.
    pub is_truncated: bool,
}

/// Options for file upload.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UploadOptions {
    /// Metadata to attach to the file.
    pub metadata: FileMetadata,
    /// Whether to overwrite existing files.
    pub overwrite: bool,
    /// Checksum for verification.
    pub checksum: Option<String>,
}

impl UploadOptions {
    /// Creates options that allow overwriting.
    pub fn overwrite() -> Self {
        Self {
            overwrite: true,
            ..Default::default()
        }
    }

    /// Sets the content type.
    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.metadata.content_type = Some(content_type.into());
        self
    }
}

/// A presigned URL for file access.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresignedUrl {
    /// The presigned URL.
    pub url: String,
    /// Expiration timestamp.
    pub expires_at: DateTime<Utc>,
    /// HTTP method this URL is valid for.
    pub method: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// @covers: with_content_type
    #[test]
    fn test_file_metadata_with_content_type() {
        let meta = FileMetadata::with_content_type("application/json")
            .with_custom("author", "test");
        assert_eq!(meta.content_type, Some("application/json".to_string()));
        assert_eq!(meta.custom.get("author"), Some(&"test".to_string()));
    }

    /// @covers: overwrite
    #[test]
    fn test_upload_options_overwrite() {
        let opts = UploadOptions::overwrite()
            .with_content_type("text/plain");
        assert!(opts.overwrite);
        assert_eq!(opts.metadata.content_type, Some("text/plain".to_string()));
    }

    #[test]
    fn test_file_info_new() {
        let info = FileInfo::new("docs/readme.md", 1024);
        assert_eq!(info.path, "docs/readme.md");
        assert_eq!(info.size, 1024);
        assert!(!info.is_directory);
    }

    /// @covers: directory
    #[test]
    fn test_file_info_directory() {
        let info = FileInfo::directory("docs/");
        assert!(info.is_directory);
        assert_eq!(info.size, 0);
    }

    /// @covers: local
    #[test]
    fn test_local() {
        let config = FileStorageConfig::local("/data");
        assert_eq!(config.storage_type, FileStorageType::Local);
        assert_eq!(config.base_path, "/data");
    }

    /// @covers: s3
    #[test]
    fn test_s3() {
        let config = FileStorageConfig::s3("my-bucket", "us-east-1");
        assert_eq!(config.storage_type, FileStorageType::S3);
        assert_eq!(config.base_path, "my-bucket");
        assert_eq!(config.region, Some("us-east-1".to_string()));
    }

    /// @covers: memory
    #[test]
    fn test_memory() {
        let config = FileStorageConfig::memory();
        assert_eq!(config.storage_type, FileStorageType::Memory);
    }

    /// @covers: with_custom
    #[test]
    fn test_with_custom() {
        let meta = FileMetadata::default().with_custom("k", "v");
        assert_eq!(meta.custom.get("k"), Some(&"v".to_string()));
    }

    /// @covers: with_prefix
    #[test]
    fn test_with_prefix() {
        let opts = ListOptions::with_prefix("docs/");
        assert_eq!(opts.prefix, Some("docs/".to_string()));
    }

    /// @covers: with_max_results
    #[test]
    fn test_with_max_results() {
        let opts = ListOptions::with_prefix("x").with_max_results(50);
        assert_eq!(opts.max_results, Some(50));
    }
}
