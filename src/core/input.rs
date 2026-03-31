//! Local filesystem input source implementation.

use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::api::input::InputSource;
use crate::api::types::{GatewayError, GatewayResult};
use crate::saf::config::GatewayConfig;

/// Local filesystem input source.
///
/// Uses `walkdir` for recursive directory scanning and `std::fs`
/// for file existence checks and reads.
///
/// All operations use synchronous `std::fs` so that `InputSource` is safe
/// to call from within an async runtime (e.g., inside a `tokio::JoinSet`).
/// Earlier versions routed through `FileGateway` via a private tokio
/// runtime, which panicked with "Cannot start a runtime from within a
/// runtime" when the caller was already inside an async context.
pub(crate) struct LocalInputSource {
    #[allow(dead_code)]
    config: GatewayConfig,
}

impl LocalInputSource {
    pub fn new(config: GatewayConfig) -> Self {
        Self { config }
    }
}

impl InputSource for LocalInputSource {
    fn scan_files(&self, root: &Path) -> GatewayResult<Vec<PathBuf>> {
        let mut files = Vec::new();
        // Canonicalize to normalize \\?\ prefixes on Windows so strip_prefix works
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());

        for entry in WalkDir::new(&root)
            .into_iter()
            .filter_entry(|e| {
                // Skip hidden dirs, target/, node_modules/ — but not the root itself
                if e.depth() == 0 {
                    return true;
                }
                let name = e.file_name().to_string_lossy();
                if e.file_type().is_dir()
                    && (name.starts_with('.') || name == "target" || name == "node_modules")
                {
                    return false;
                }
                true
            })
        {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            if entry.file_type().is_file() {
                if let Ok(rel) = entry.path().strip_prefix(&root) {
                    files.push(rel.to_path_buf());
                }
            }
        }

        Ok(files)
    }

    fn file_exists(&self, root: &Path, relative: &str) -> GatewayResult<bool> {
        Ok(root.join(relative).exists())
    }

    fn read_file(&self, path: &Path) -> GatewayResult<String> {
        std::fs::read_to_string(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                GatewayError::NotFound(format!("File not found: {}", path.display()))
            } else {
                GatewayError::IoError(e)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_source() -> (TempDir, LocalInputSource) {
        let tmp = TempDir::new().unwrap();
        let config = GatewayConfig::default();
        (tmp, LocalInputSource::new(config))
    }

    #[test]
    fn test_scan_files_empty_dir() {
        let (tmp, source) = make_source();
        let files = source.scan_files(tmp.path()).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn test_scan_files_finds_files() {
        let (tmp, source) = make_source();
        std::fs::write(tmp.path().join("a.rs"), "fn main() {}").unwrap();
        std::fs::create_dir(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src").join("lib.rs"), "").unwrap();

        let mut files = source.scan_files(tmp.path()).unwrap();
        files.sort();
        assert_eq!(files.len(), 2);
        assert!(files.contains(&PathBuf::from("a.rs")));
        let src_lib: PathBuf = ["src", "lib.rs"].iter().collect();
        assert!(files.contains(&src_lib));
    }

    #[test]
    fn test_scan_files_skips_hidden_and_target() {
        let (tmp, source) = make_source();
        std::fs::create_dir(tmp.path().join(".git")).unwrap();
        std::fs::write(tmp.path().join(".git").join("config"), "").unwrap();
        std::fs::create_dir(tmp.path().join("target")).unwrap();
        // "debug" must be a file, not a dir name that looks like a file
        std::fs::write(tmp.path().join("target").join("output.o"), "").unwrap();
        std::fs::write(tmp.path().join("visible.rs"), "").unwrap();

        let files = source.scan_files(tmp.path()).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], PathBuf::from("visible.rs"));
    }

    #[test]
    fn test_file_exists_true() {
        let (tmp, source) = make_source();
        std::fs::write(tmp.path().join("Cargo.toml"), "[package]").unwrap();

        assert!(source.file_exists(tmp.path(), "Cargo.toml").unwrap());
    }

    #[test]
    fn test_file_exists_false() {
        let (tmp, source) = make_source();
        assert!(!source.file_exists(tmp.path(), "nonexistent.rs").unwrap());
    }

    #[test]
    fn test_read_file() {
        let (tmp, source) = make_source();
        let content = "fn main() { println!(\"hello\"); }";
        std::fs::write(tmp.path().join("main.rs"), content).unwrap();

        let read = source.read_file(&tmp.path().join("main.rs")).unwrap();
        assert_eq!(read, content);
    }

    #[test]
    fn test_read_file_not_found() {
        let (tmp, source) = make_source();
        let result = source.read_file(&tmp.path().join("missing.rs"));
        assert!(result.is_err());
    }

    /// Regression test: InputSource methods must not panic when called
    /// from within a tokio runtime (e.g., inside a JoinSet task).
    /// Previously, `file_exists` and `read_file` used `runtime().block_on()`
    /// which panicked with "Cannot start a runtime from within a runtime".
    #[tokio::test]
    async fn test_read_file_inside_async_runtime() {
        let (tmp, source) = make_source();
        let content = "async-safe content";
        std::fs::write(tmp.path().join("async.txt"), content).unwrap();

        // These would panic with the old runtime().block_on() approach
        assert!(source.file_exists(tmp.path(), "async.txt").unwrap());
        assert!(!source.file_exists(tmp.path(), "missing.txt").unwrap());

        let read = source.read_file(&tmp.path().join("async.txt")).unwrap();
        assert_eq!(read, content);
    }

    #[tokio::test]
    async fn test_read_file_not_found_inside_async_runtime() {
        let (tmp, source) = make_source();
        let result = source.read_file(&tmp.path().join("nope.rs"));
        assert!(result.is_err());
    }
}
