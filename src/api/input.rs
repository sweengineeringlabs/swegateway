//! Input source traits for reading files and scanning directories.
//!
//! These are generic infrastructure abstractions — they work with raw paths
//! and bytes without knowledge of any domain types (e.g., `ScanError`).
//! Domain crates implement their own scanner traits as thin adapters
//! that delegate to these input sources.

use std::path::{Path, PathBuf};

use crate::api::types::GatewayResult;

/// Trait for reading files and scanning directories.
///
/// Implementations handle the raw I/O: local filesystem, cloud storage, etc.
/// The caller is responsible for interpreting the file contents.
pub trait InputSource: Send + Sync {
    /// Recursively scan a directory and return all file paths (relative to root).
    ///
    /// Implementations should skip hidden directories, `target/`, and `node_modules/`.
    fn scan_files(&self, root: &Path) -> GatewayResult<Vec<PathBuf>>;

    /// Check if a file exists relative to a root path.
    fn file_exists(&self, root: &Path, relative: &str) -> GatewayResult<bool>;

    /// Read the contents of a file as a UTF-8 string.
    fn read_file(&self, path: &Path) -> GatewayResult<String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify the trait is object-safe (can be used as `dyn InputSource`).
    #[test]
    fn test_input_source_is_object_safe() {
        fn _assert_object_safe(_: &dyn InputSource) {}
    }
}
