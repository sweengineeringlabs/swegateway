use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Resolved paths for the local registry.
pub struct RegistryPaths {
    /// Root directory (e.g. ~/.cargo/registry.local/)
    pub root: PathBuf,
    /// Index directory (root/index/) — a git repo
    pub index: PathBuf,
    /// Crates directory (root/crates/) — .crate file storage
    pub crates: PathBuf,
}

/// Resolve the registry root path.
///
/// Priority: CLI arg > `CARGO_REGISTRIES_LOCAL_INDEX` env var > `~/.cargo/registry.local/`
pub fn resolve(cli_path: Option<&str>) -> Result<RegistryPaths> {
    let root = if let Some(path) = cli_path {
        PathBuf::from(path)
    } else if let Ok(env_val) = std::env::var("CARGO_REGISTRIES_LOCAL_INDEX") {
        root_from_index_url(&env_val)?
    } else {
        default_root()?
    };

    Ok(paths_from_root(root))
}

/// Build a `file:///` URL pointing to the index directory.
pub fn index_url(paths: &RegistryPaths) -> String {
    let index_str = paths.index.display().to_string().replace('\\', "/");
    format!("file:///{}", index_str.trim_start_matches('/'))
}

/// Ensure the registry directories exist.
pub fn ensure_dirs(paths: &RegistryPaths) -> Result<()> {
    std::fs::create_dir_all(&paths.index).with_context(|| {
        format!(
            "Failed to create index directory: {}",
            paths.index.display()
        )
    })?;
    std::fs::create_dir_all(&paths.crates).with_context(|| {
        format!(
            "Failed to create crates directory: {}",
            paths.crates.display()
        )
    })?;
    Ok(())
}

fn paths_from_root(root: PathBuf) -> RegistryPaths {
    let index = root.join("index");
    let crates = root.join("crates");
    RegistryPaths { root, index, crates }
}

fn root_from_index_url(url: &str) -> Result<PathBuf> {
    let mut path_str = url
        .strip_prefix("file://")
        .unwrap_or(url)
        .to_string();

    // Windows: "/C:/…" → "C:/…"
    if path_str.len() >= 3
        && path_str.starts_with('/')
        && path_str.as_bytes()[2] == b':'
    {
        path_str.remove(0);
    }

    let index_path = PathBuf::from(&path_str);
    index_path
        .parent()
        .map(Path::to_path_buf)
        .context("Cannot determine registry root from CARGO_REGISTRIES_LOCAL_INDEX env var")
}

fn default_root() -> Result<PathBuf> {
    home_dir()
        .map(|h| h.join(".cargo").join("registry.local"))
        .context("Cannot determine home directory")
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}
