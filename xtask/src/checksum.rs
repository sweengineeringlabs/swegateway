use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::Path;

/// Compute the SHA-256 hex digest of a file.
pub fn sha256_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("Failed to read file for checksum: {}", path.display()))?;
    Ok(sha256_bytes(&bytes))
}

/// Compute the SHA-256 hex digest of a byte slice.
pub fn sha256_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    format!("{:x}", result)
}
